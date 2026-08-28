use std::marker::PhantomData;
use std::time::Duration;
use uuid::Uuid;

use crate::ports::journal::{EventRecord, JournalHead};
use crate::ports::lock::{LockOwner, WriterLock};
use crate::ports::operation::{OperationRecord, OperationStatus, PreparedOperationRecord};

/// Storage capabilities required by the sole mutation coordinator.
///
/// Implementations must commit each method atomically. In particular `commit_verified` must append
/// verified domain facts, the terminal event, and every affected projection in one transaction.
pub(crate) trait RepositoryStore {
    type Error;

    fn head(&self) -> Result<JournalHead, Self::Error>;
    fn prepare(
        &mut self,
        expected_head: JournalHead,
        event: &EventRecord,
        operation: &PreparedOperationRecord,
    ) -> Result<OperationRecord, Self::Error>;
    fn record_transition(
        &mut self,
        expected_head: JournalHead,
        event: &EventRecord,
        operation_id: Uuid,
        status: OperationStatus,
        result: Option<&[u8]>,
    ) -> Result<OperationRecord, Self::Error>;
    fn commit_verified(
        &mut self,
        expected_head: JournalHead,
        events: &[EventRecord],
        operation_id: Uuid,
        result: &[u8],
    ) -> Result<OperationRecord, Self::Error>;
}

/// Proof that an operation was durably prepared while the repository writer lock is held.
/// The constructor is private to this module, so mutation adapters cannot be called early.
pub(crate) struct PreparedOperation<'lock> {
    record: OperationRecord,
    _lock: PhantomData<&'lock mut ()>,
}

impl PreparedOperation<'_> {
    pub(crate) fn record(&self) -> &OperationRecord { &self.record }
}

#[derive(Debug)]
pub(crate) enum CoordinationError<LockError, StoreError, EffectError> {
    Lock(LockError),
    Store(StoreError),
    Effect(EffectError),
    VerificationFailed,
}

pub(crate) struct Coordinator<L, S> { lock: L, store: S, lock_timeout: Duration }

impl<L, S> Coordinator<L, S> {
    pub(crate) fn new(lock: L, store: S, lock_timeout: Duration) -> Self { Self { lock, store, lock_timeout } }
    pub(crate) fn store(&self) -> &S { &self.store }
    pub(crate) fn store_mut(&mut self) -> &mut S { &mut self.store }
}

impl<L, S> Coordinator<L, S>
where
    L: WriterLock,
    S: RepositoryStore,
{
    /// Runs the durable part of one mutation. Event construction and substrate-specific planning
    /// remain outside, but effect execution cannot receive its capability before prepare commits.
    pub(crate) fn execute<E, V, EffectError>(
        &mut self,
        owner: LockOwner,
        prepared_event: EventRecord,
        prepared_record: PreparedOperationRecord,
        applying_event: EventRecord,
        effects: E,
        verify: V,
        terminal_events: Vec<EventRecord>,
        result: Vec<u8>,
    ) -> Result<OperationRecord, CoordinationError<L::Error, S::Error, EffectError>>
    where
        E: for<'lock> FnOnce(&PreparedOperation<'lock>) -> Result<(), EffectError>,
        V: FnOnce() -> bool,
    {
        let _guard = self.lock.try_acquire(self.lock_timeout, owner).map_err(CoordinationError::Lock)?;
        let head = self.store.head().map_err(CoordinationError::Store)?;
        let prepared = self.store.prepare(head, &prepared_event, &prepared_record).map_err(CoordinationError::Store)?;
        let prepared = PreparedOperation { record: prepared, _lock: PhantomData };
        let head = self.store.head().map_err(CoordinationError::Store)?;
        self.store.record_transition(head, &applying_event, prepared_record.operation_id, OperationStatus::Applying, None).map_err(CoordinationError::Store)?;
        effects(&prepared).map_err(CoordinationError::Effect)?;
        if !verify() { return Err(CoordinationError::VerificationFailed); }
        let head = self.store.head().map_err(CoordinationError::Store)?;
        self.store.commit_verified(head, &terminal_events, prepared_record.operation_id, &result).map_err(CoordinationError::Store)
    }
}

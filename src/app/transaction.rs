use std::fmt;
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
    type Projection;

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
        projections: &[Self::Projection],
        operation_id: Uuid,
        result: &[u8],
    ) -> Result<OperationRecord, Self::Error>;
}
impl<S: RepositoryStore + ?Sized> RepositoryStore for &mut S {
    type Error = S::Error;
    type Projection = S::Projection;

    fn head(&self) -> Result<JournalHead, Self::Error> {
        (**self).head()
    }
    fn prepare(
        &mut self,
        expected_head: JournalHead,
        event: &EventRecord,
        operation: &PreparedOperationRecord,
    ) -> Result<OperationRecord, Self::Error> {
        (**self).prepare(expected_head, event, operation)
    }
    fn record_transition(
        &mut self,
        expected_head: JournalHead,
        event: &EventRecord,
        operation_id: Uuid,
        status: OperationStatus,
        result: Option<&[u8]>,
    ) -> Result<OperationRecord, Self::Error> {
        (**self).record_transition(expected_head, event, operation_id, status, result)
    }
    fn commit_verified(
        &mut self,
        expected_head: JournalHead,
        events: &[EventRecord],
        projections: &[Self::Projection],
        operation_id: Uuid,
        result: &[u8],
    ) -> Result<OperationRecord, Self::Error> {
        (**self).commit_verified(expected_head, events, projections, operation_id, result)
    }
}

/// Proof that an operation was durably prepared while the repository writer lock is held.
/// The constructor is private to this module, so mutation adapters cannot be called early.
pub(crate) struct PreparedOperation<'lock> {
    record: OperationRecord,
    _lock: PhantomData<&'lock mut ()>,
}

impl PreparedOperation<'_> {
    pub(crate) fn record(&self) -> &OperationRecord {
        &self.record
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleEvent {
    Prepared,
    Applying,
    ConflictPaused,
    Aborting,
    Aborted,
    Verifying,
    RepairRequired,
}
/// Deterministic fault-injection boundaries around durable saga state changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TransactionBoundary {
    BeforePrepare,
    AfterPrepareBeforeFirstEffect,
    AfterEachEffect,
    BeforeVerify,
    AfterVerifyBeforeCommit,
    CommitAmbiguity,
}
impl TransactionBoundary {
    pub(crate) const fn failpoint_name(self) -> &'static str {
        match self {
            Self::BeforePrepare => "FP-0-before-prepare",
            Self::AfterPrepareBeforeFirstEffect => "FP-1-after-prepare-before-first-effect",
            Self::AfterEachEffect => "FP-2-after-each-effect",
            Self::BeforeVerify => "FP-3-before-verify",
            Self::AfterVerifyBeforeCommit => "FP-4-after-verify-before-commit",
            Self::CommitAmbiguity => "FP-5-commit-ambiguity",
        }
    }
}

/// A process-local test fault. Configuration failures carry no operation because
/// they are rejected before the coordinator acquires the writer lock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TransactionFault {
    Configuration(String),
    Injected(TransactionBoundary),
}

impl fmt::Display for TransactionFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => formatter.write_str(message),
            Self::Injected(boundary) => write!(
                formatter,
                "injected failure at {}",
                boundary.failpoint_name()
            ),
        }
    }
}

impl std::error::Error for TransactionFault {}

#[derive(Debug)]
pub(crate) enum EffectFailure<E> {
    /// The effect closure observed that no declared external effect occurred.
    NotApplied(E),
    /// The effect produced a durable conflict artifact and must await explicit resolution.
    ConflictPaused { source: E, result: Vec<u8> },
    /// An effect may be visible or cannot be classified safely.
    Indeterminate(E),
}

pub(crate) struct VerifiedCommit<P> {
    pub(crate) events: Vec<EventRecord>,
    pub(crate) projections: Vec<P>,
    pub(crate) result: Vec<u8>,
}

#[derive(Debug)]
pub(crate) enum CoordinationError<
    LockError,
    StoreError,
    EffectError,
    VerificationError,
    CommitError,
> {
    Lock(LockError),
    Store(StoreError),
    Fault {
        source: TransactionFault,
        operation: Option<OperationRecord>,
    },
    EffectAborted {
        source: EffectError,
        operation: OperationRecord,
    },
    ConflictPaused {
        source: EffectError,
        operation: OperationRecord,
    },
    EffectRepairRequired {
        source: EffectError,
        operation: OperationRecord,
    },
    Verification {
        source: VerificationError,
        operation: OperationRecord,
    },
    VerificationFailed(OperationRecord),
    CommitData {
        source: CommitError,
        operation: OperationRecord,
    },
    RecoveryRequired(OperationRecord),
    /// An external effect may be visible, but recording its repair state failed.
    Indeterminate {
        operation: OperationRecord,
        source: StoreError,
    },
}

pub(crate) struct Coordinator<L, S> {
    lock: L,
    store: S,
    lock_timeout: Duration,
}

impl<L, S> Coordinator<L, S> {
    pub(crate) fn new(lock: L, store: S, lock_timeout: Duration) -> Self {
        Self {
            lock,
            store,
            lock_timeout,
        }
    }
    pub(crate) fn store(&self) -> &S {
        &self.store
    }
    pub(crate) fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }
}

impl<L, S> Coordinator<L, S>
where
    L: WriterLock,
    S: RepositoryStore,
{
    /// Runs one prepared saga. The effect closure cannot be called without the unforgeable
    /// capability produced after the prepared record commits while the writer lock is held.
    pub(crate) fn execute<Fault, F, E, V, C, T, EffectError, VerificationError, CommitError>(
        &mut self,
        owner: LockOwner,
        prepared_record: PreparedOperationRecord,
        mut fault: Fault,
        mut lifecycle_event: F,
        effects: E,
        verify: V,
        verified_commit: C,
    ) -> Result<
        OperationRecord,
        CoordinationError<L::Error, S::Error, EffectError, VerificationError, CommitError>,
    >
    where
        Fault: FnMut(TransactionBoundary) -> Result<(), TransactionFault>,
        F: FnMut(LifecycleEvent, u32, JournalHead) -> EventRecord,
        E: for<'lock> FnOnce(&PreparedOperation<'lock>) -> Result<T, EffectFailure<EffectError>>,
        V: FnOnce(&T) -> Result<bool, VerificationError>,
        C: FnOnce(
            &PreparedOperation<'_>,
            &T,
            JournalHead,
        ) -> Result<VerifiedCommit<S::Projection>, CommitError>,
    {
        let _guard = self
            .lock
            .try_acquire(self.lock_timeout, owner)
            .map_err(CoordinationError::Lock)?;
        let head = self.store.head().map_err(CoordinationError::Store)?;
        fault(TransactionBoundary::BeforePrepare).map_err(|source| CoordinationError::Fault {
            source,
            operation: None,
        })?;
        let prepared_event = lifecycle_event(LifecycleEvent::Prepared, 0, head);
        let mut record = self
            .store
            .prepare(head, &prepared_event, &prepared_record)
            .map_err(CoordinationError::Store)?;

        if record.status == OperationStatus::Committed {
            return Ok(record);
        }
        if matches!(
            record.status,
            OperationStatus::AwaitingResolution
                | OperationStatus::Aborting
                | OperationStatus::Aborted
                | OperationStatus::RepairRequired
        ) {
            return Err(CoordinationError::RecoveryRequired(record));
        }

        if record.status == OperationStatus::Prepared {
            if let Err(source) = fault(TransactionBoundary::AfterPrepareBeforeFirstEffect) {
                let aborted = self
                    .abort_unapplied(&record, 1, &mut lifecycle_event)
                    .map_err(|store_error| CoordinationError::Indeterminate {
                        operation: record,
                        source: store_error,
                    })?;
                return Err(CoordinationError::Fault {
                    source,
                    operation: Some(aborted),
                });
            }
            let head = self.store.head().map_err(CoordinationError::Store)?;
            let event = lifecycle_event(LifecycleEvent::Applying, 1, head);
            record = self
                .store
                .record_transition(
                    head,
                    &event,
                    prepared_record.operation_id,
                    OperationStatus::Applying,
                    None,
                )
                .map_err(CoordinationError::Store)?;
        }

        let prepared = PreparedOperation {
            record: record.clone(),
            _lock: PhantomData,
        };
        let effect = if record.status == OperationStatus::Applying {
            match effects(&prepared) {
                Ok(effect) => effect,
                Err(EffectFailure::NotApplied(source)) => {
                    let aborted = self
                        .abort_unapplied(&record, 2, &mut lifecycle_event)
                        .map_err(|store_error| CoordinationError::Indeterminate {
                            operation: record,
                            source: store_error,
                        })?;
                    return Err(CoordinationError::EffectAborted {
                        source,
                        operation: aborted,
                    });
                }
                Err(EffectFailure::ConflictPaused { source, result }) => {
                    let head =
                        self.store
                            .head()
                            .map_err(|source| CoordinationError::Indeterminate {
                                operation: record.clone(),
                                source,
                            })?;
                    let event = lifecycle_event(LifecycleEvent::ConflictPaused, 2, head);
                    let paused = self
                        .store
                        .record_transition(
                            head,
                            &event,
                            prepared_record.operation_id,
                            OperationStatus::AwaitingResolution,
                            Some(&result),
                        )
                        .map_err(|source| CoordinationError::Indeterminate {
                            operation: record,
                            source,
                        })?;
                    return Err(CoordinationError::ConflictPaused {
                        source,
                        operation: paused,
                    });
                }
                Err(EffectFailure::Indeterminate(source)) => {
                    let repair = self.mark_repair_required(&record, 2, &mut lifecycle_event)?;
                    return Err(CoordinationError::EffectRepairRequired {
                        source,
                        operation: repair,
                    });
                }
            }
        } else {
            return Err(CoordinationError::RecoveryRequired(record));
        };

        if let Err(source) = fault(TransactionBoundary::AfterEachEffect) {
            let repair = self.mark_repair_required(&record, 2, &mut lifecycle_event)?;
            return Err(CoordinationError::Fault {
                source,
                operation: Some(repair),
            });
        }

        let head = match self.store.head() {
            Ok(head) => head,
            Err(source) => {
                return Err(CoordinationError::Indeterminate {
                    operation: record,
                    source,
                });
            }
        };
        let event = lifecycle_event(LifecycleEvent::Verifying, 2, head);
        record = match self.store.record_transition(
            head,
            &event,
            prepared_record.operation_id,
            OperationStatus::Verifying,
            None,
        ) {
            Ok(record) => record,
            Err(source) => {
                return match self.try_mark_repair_required(&record, 2, &mut lifecycle_event) {
                    Ok(repair) => Err(CoordinationError::RecoveryRequired(repair)),
                    Err(_) => Err(CoordinationError::Indeterminate {
                        operation: record,
                        source,
                    }),
                };
            }
        };

        if let Err(source) = fault(TransactionBoundary::BeforeVerify) {
            let repair = self.mark_repair_required(&record, 3, &mut lifecycle_event)?;
            return Err(CoordinationError::Fault {
                source,
                operation: Some(repair),
            });
        }

        let prepared = PreparedOperation {
            record: record.clone(),
            _lock: PhantomData,
        };
        let verified = match verify(&effect) {
            Ok(verified) => verified,
            Err(source) => {
                let repair = self.mark_repair_required(&record, 3, &mut lifecycle_event)?;
                return Err(CoordinationError::Verification {
                    source,
                    operation: repair,
                });
            }
        };
        if !verified {
            let repair = self.mark_repair_required(&record, 3, &mut lifecycle_event)?;
            return Err(CoordinationError::VerificationFailed(repair));
        }

        if let Err(source) = fault(TransactionBoundary::AfterVerifyBeforeCommit) {
            let repair = self.mark_repair_required(&record, 3, &mut lifecycle_event)?;
            return Err(CoordinationError::Fault {
                source,
                operation: Some(repair),
            });
        }

        let head = self
            .store
            .head()
            .map_err(|source| CoordinationError::Indeterminate {
                operation: record.clone(),
                source,
            })?;
        let commit = verified_commit(&prepared, &effect, head).map_err(|source| {
            CoordinationError::CommitData {
                source,
                operation: record.clone(),
            }
        })?;
        let committed = self
            .store
            .commit_verified(
                head,
                &commit.events,
                &commit.projections,
                prepared_record.operation_id,
                &commit.result,
            )
            .map_err(|source| CoordinationError::Indeterminate {
                operation: record,
                source,
            })?;
        fault(TransactionBoundary::CommitAmbiguity).map_err(|source| CoordinationError::Fault {
            source,
            operation: Some(committed.clone()),
        })?;
        Ok(committed)
    }

    fn abort_unapplied<F>(
        &mut self,
        operation: &OperationRecord,
        ordinal: u32,
        lifecycle_event: &mut F,
    ) -> Result<OperationRecord, S::Error>
    where
        F: FnMut(LifecycleEvent, u32, JournalHead) -> EventRecord,
    {
        let head = self.store.head()?;
        let event = lifecycle_event(LifecycleEvent::Aborting, ordinal, head);
        self.store.record_transition(
            head,
            &event,
            operation.operation_id,
            OperationStatus::Aborting,
            None,
        )?;
        let head = self.store.head()?;
        let event = lifecycle_event(LifecycleEvent::Aborted, ordinal + 1, head);
        self.store.record_transition(
            head,
            &event,
            operation.operation_id,
            OperationStatus::Aborted,
            None,
        )
    }

    fn mark_repair_required<F, LockError, EffectError, VerificationError, CommitError>(
        &mut self,
        operation: &OperationRecord,
        ordinal: u32,
        lifecycle_event: &mut F,
    ) -> Result<
        OperationRecord,
        CoordinationError<LockError, S::Error, EffectError, VerificationError, CommitError>,
    >
    where
        F: FnMut(LifecycleEvent, u32, JournalHead) -> EventRecord,
    {
        self.try_mark_repair_required(operation, ordinal, lifecycle_event)
            .map_err(|source| CoordinationError::Indeterminate {
                operation: operation.clone(),
                source,
            })
    }

    fn try_mark_repair_required<F>(
        &mut self,
        operation: &OperationRecord,
        ordinal: u32,
        lifecycle_event: &mut F,
    ) -> Result<OperationRecord, S::Error>
    where
        F: FnMut(LifecycleEvent, u32, JournalHead) -> EventRecord,
    {
        let head = self.store.head()?;
        let event = lifecycle_event(LifecycleEvent::RepairRequired, ordinal, head);
        self.store.record_transition(
            head,
            &event,
            operation.operation_id,
            OperationStatus::RepairRequired,
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use crate::ports::journal::{ActorKind, GENESIS_HASH, PayloadCodec};

    #[derive(Default)]
    struct MemoryStore {
        head: u64,
        operation: Option<OperationRecord>,
        events: Vec<String>,
    }

    impl MemoryStore {
        fn append(&mut self, event: &EventRecord) {
            assert_eq!(event.operation_ordinal, self.events.len() as u32);
            self.events.push(event.event_type.clone());
            self.head += 1;
        }
    }

    impl RepositoryStore for MemoryStore {
        type Projection = ();
        type Error = &'static str;

        fn head(&self) -> Result<JournalHead, Self::Error> {
            Ok(JournalHead {
                local_seq: self.head,
                event_hash: GENESIS_HASH,
            })
        }

        fn prepare(
            &mut self,
            expected_head: JournalHead,
            event: &EventRecord,
            operation: &PreparedOperationRecord,
        ) -> Result<OperationRecord, Self::Error> {
            assert_eq!(expected_head.local_seq, self.head);
            self.append(event);
            let record = OperationRecord {
                operation_id: operation.operation_id,
                request_hash: operation.request_hash,
                command_kind: operation.command_kind.clone(),
                status: OperationStatus::Prepared,
                prepared_seq: self.head,
                terminal_seq: None,
                precondition_fingerprint: operation.precondition_fingerprint.clone(),
                expected_effects: operation.expected_effects.clone(),
                recovery_artifact_hash: operation.recovery_artifact_hash,
                result: None,
                last_event_seq: self.head,
            };
            self.operation = Some(record.clone());
            Ok(record)
        }

        fn record_transition(
            &mut self,
            expected_head: JournalHead,
            event: &EventRecord,
            operation_id: Uuid,
            status: OperationStatus,
            result: Option<&[u8]>,
        ) -> Result<OperationRecord, Self::Error> {
            assert_eq!(expected_head.local_seq, self.head);
            assert_eq!(
                self.operation
                    .as_ref()
                    .expect("prepared operation")
                    .operation_id,
                operation_id
            );
            self.append(event);
            let operation = self.operation.as_mut().expect("prepared operation");
            operation.status = status;
            operation.last_event_seq = self.head;
            operation.terminal_seq = status.is_terminal().then_some(self.head);
            operation.result = result.map(<[u8]>::to_vec);
            Ok(operation.clone())
        }

        fn commit_verified(
            &mut self,
            expected_head: JournalHead,
            events: &[EventRecord],
            _projections: &[Self::Projection],
            operation_id: Uuid,
            result: &[u8],
        ) -> Result<OperationRecord, Self::Error> {
            assert_eq!(expected_head.local_seq, self.head);
            assert_eq!(
                self.operation
                    .as_ref()
                    .expect("prepared operation")
                    .operation_id,
                operation_id
            );
            for event in events {
                self.append(event);
            }
            let operation = self.operation.as_mut().expect("prepared operation");
            operation.status = OperationStatus::Committed;
            operation.last_event_seq = self.head;
            operation.terminal_seq = Some(self.head);
            operation.result = Some(result.to_vec());
            Ok(operation.clone())
        }
    }

    struct MemoryLock;

    impl WriterLock for MemoryLock {
        type Guard = ();
        type Error = &'static str;

        fn try_acquire(
            &self,
            _timeout: Duration,
            _owner: LockOwner,
        ) -> Result<Self::Guard, Self::Error> {
            Ok(())
        }
    }

    fn prepared() -> PreparedOperationRecord {
        PreparedOperationRecord {
            operation_id: Uuid::from_u128(7),
            request_hash: [1; 32],
            command_kind: "capture".into(),
            precondition_fingerprint: vec![2],
            expected_effects: vec![3],
            recovery_artifact_hash: Some([4; 32]),
        }
    }

    fn event(phase: LifecycleEvent, ordinal: u32, _head: JournalHead) -> EventRecord {
        EventRecord {
            event_id: Uuid::from_u128(u128::from(ordinal) + 1),
            repo_id: Uuid::from_u128(8),
            event_type: format!("{phase:?}"),
            event_schema_version: 1,
            envelope_version: 1,
            operation_id: Uuid::from_u128(7),
            operation_ordinal: ordinal,
            actor_id: Uuid::from_u128(9),
            actor_kind: ActorKind::System,
            recorded_at_utc: "2026-08-29T00:00:00Z".into(),
            observed_at_utc: None,
            repository_fingerprint: Vec::new(),
            payload_codec: PayloadCodec::CanonicalJsonV1,
            payload: Vec::new(),
            provenance: Vec::new(),
            evidence_manifest: Vec::new(),
            dedup_key: None,
            previous_event_hash: GENESIS_HASH,
            event_hash: [1; 32],
        }
    }
    fn committed_event(ordinal: u32, head: JournalHead) -> EventRecord {
        let mut committed = event(LifecycleEvent::Prepared, ordinal, head);
        committed.event_type = "Committed".into();
        committed
    }

    #[test]
    fn failpoint_lifecycle_table_preserves_durable_statuses_and_events() {
        let cases = [
            (TransactionBoundary::BeforePrepare, None, &[][..], 0, 0, 0),
            (
                TransactionBoundary::AfterPrepareBeforeFirstEffect,
                Some(OperationStatus::Aborted),
                &["Prepared", "Aborting", "Aborted"][..],
                0,
                0,
                0,
            ),
            (
                TransactionBoundary::AfterEachEffect,
                Some(OperationStatus::RepairRequired),
                &["Prepared", "Applying", "RepairRequired"][..],
                1,
                0,
                0,
            ),
            (
                TransactionBoundary::BeforeVerify,
                Some(OperationStatus::RepairRequired),
                &["Prepared", "Applying", "Verifying", "RepairRequired"][..],
                1,
                0,
                0,
            ),
            (
                TransactionBoundary::AfterVerifyBeforeCommit,
                Some(OperationStatus::RepairRequired),
                &["Prepared", "Applying", "Verifying", "RepairRequired"][..],
                1,
                1,
                0,
            ),
            (
                TransactionBoundary::CommitAmbiguity,
                Some(OperationStatus::Committed),
                &["Prepared", "Applying", "Verifying", "Committed"][..],
                1,
                1,
                1,
            ),
        ];

        for (
            boundary,
            expected_status,
            expected_events,
            expected_effects,
            expected_verifies,
            expected_commits,
        ) in cases
        {
            let effects = Cell::new(0);
            let verifies = Cell::new(0);
            let commits = Cell::new(0);
            let mut coordinator =
                Coordinator::new(MemoryLock, MemoryStore::default(), Duration::ZERO);
            let result = coordinator.execute(
                LockOwner {
                    process_id: 1,
                    operation: Some("test".into()),
                },
                prepared(),
                |observed| {
                    if observed == boundary {
                        Err(TransactionFault::Injected(observed))
                    } else {
                        Ok(())
                    }
                },
                event,
                |_| {
                    effects.set(effects.get() + 1);
                    Ok::<_, EffectFailure<&'static str>>(())
                },
                |_| {
                    verifies.set(verifies.get() + 1);
                    Ok::<_, &'static str>(true)
                },
                |_, _, head| {
                    commits.set(commits.get() + 1);
                    Ok::<_, &'static str>(VerifiedCommit {
                        events: vec![committed_event(3, head)],
                        projections: Vec::new(),
                        result: b"ok".to_vec(),
                    })
                },
            );

            let operation = match result {
                Err(CoordinationError::Fault {
                    source: TransactionFault::Injected(actual),
                    operation,
                }) => {
                    assert_eq!(actual, boundary);
                    operation
                }
                other => panic!("unexpected result for {boundary:?}: {other:?}"),
            };
            assert_eq!(
                operation.as_ref().map(|record| record.status),
                expected_status,
                "{boundary:?}"
            );
            assert_eq!(
                coordinator
                    .store()
                    .operation
                    .as_ref()
                    .map(|record| record.status),
                expected_status,
                "{boundary:?}"
            );
            assert_eq!(
                coordinator
                    .store()
                    .events
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                expected_events,
                "{boundary:?}"
            );
            assert_eq!(effects.get(), expected_effects, "{boundary:?}");
            assert_eq!(verifies.get(), expected_verifies, "{boundary:?}");
            assert_eq!(commits.get(), expected_commits, "{boundary:?}");
        }
    }

    #[test]
    fn disabled_fault_callback_preserves_success_path() {
        let mut coordinator = Coordinator::new(MemoryLock, MemoryStore::default(), Duration::ZERO);
        let result = coordinator
            .execute(
                LockOwner {
                    process_id: 1,
                    operation: None,
                },
                prepared(),
                |_| Ok(()),
                event,
                |_| Ok::<_, EffectFailure<&'static str>>(()),
                |_| Ok::<_, &'static str>(true),
                |_, _, head| {
                    Ok::<_, &'static str>(VerifiedCommit {
                        events: vec![committed_event(3, head)],
                        projections: Vec::new(),
                        result: b"ok".to_vec(),
                    })
                },
            )
            .expect("disabled failpoints must not alter successful execution");
        assert_eq!(result.status, OperationStatus::Committed);
        assert_eq!(
            coordinator.store().events,
            ["Prepared", "Applying", "Verifying", "Committed"]
        );
    }
}

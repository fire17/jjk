use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::adapters::os::failpoint::{Failpoint, FailpointEvaluator};
use crate::adapters::os::lock::{LockError, OsWriterLock};
use crate::app::transaction::{
    CoordinationError, Coordinator, EffectFailure, LifecycleEvent, PreparedOperation,
    RepositoryStore, TransactionBoundary, TransactionFault, VerifiedCommit,
};
use crate::ports::journal::{ActorKind, EventRecord, JournalHead, PayloadCodec};
use crate::ports::lock::LockOwner;
use crate::ports::operation::{OperationRecord, PreparedOperationRecord};

/// Immutable intent stored before a runtime mutator receives permission to run.
pub(crate) struct RuntimeMutationRequest {
    pub(crate) operation_id: Uuid,
    pub(crate) repo_id: Uuid,
    pub(crate) actor_id: Uuid,
    pub(crate) actor_kind: ActorKind,
    pub(crate) command_kind: String,
    pub(crate) recorded_at_utc: String,
    pub(crate) repository_fingerprint: Vec<u8>,
    pub(crate) request: Vec<u8>,
    /// Canonical effect plan, including exact recovery material required by the command.
    pub(crate) expected_effects: Vec<u8>,
    /// Optional separately-durable recovery artifact. The operation binds its digest.
    pub(crate) recovery_artifact: Option<Vec<u8>>,
    pub(crate) provenance: Vec<u8>,
    pub(crate) lock_timeout: Duration,
}

/// One verified domain fact published only in the same transaction as OperationCommitted.
pub(crate) struct RuntimeFact {
    pub(crate) event_type: String,
    pub(crate) payload: Vec<u8>,
    pub(crate) provenance: Vec<u8>,
    pub(crate) evidence_manifest: Vec<u8>,
    pub(crate) dedup_key: Option<String>,
}

/// Facts, derived projection writes, and result published after verification.
pub(crate) struct RuntimeMutationCommit<P> {
    pub(crate) facts: Vec<RuntimeFact>,
    pub(crate) projections: Vec<P>,
    pub(crate) result: Vec<u8>,
}

pub(crate) type RuntimeMutationError<StoreError, EffectError, VerificationError, CommitError> =
    CoordinationError<LockError, StoreError, EffectError, VerificationError, CommitError>;

/// Executes one runtime mutation under the repository OS writer lock.
///
/// `effects` is the only closure allowed to touch Git or the filesystem. Its first argument is an
/// unforgeable proof that intent is durable. `NotApplied` is terminally aborted; `Indeterminate`
/// and every post-effect uncertainty are durably marked repair-required.
pub(crate) fn execute<S, E, V, C, T, EffectError, VerificationError, CommitError>(
    git_common_dir: &Path,
    store: &mut S,
    request: RuntimeMutationRequest,
    effects: E,
    verify: V,
    commit: C,
) -> Result<
    OperationRecord,
    RuntimeMutationError<S::Error, EffectError, VerificationError, CommitError>,
>
where
    S: RepositoryStore,
    E: for<'prepared> FnOnce(
        &PreparedOperation<'prepared>,
    ) -> Result<T, EffectFailure<EffectError>>,
    V: FnOnce(&T) -> Result<bool, VerificationError>,
    C: FnOnce(&T) -> Result<RuntimeMutationCommit<S::Projection>, CommitError>,
{
    let factory = EventFactory::new(&request);
    let prepared = PreparedOperationRecord {
        operation_id: request.operation_id,
        request_hash: digest(&request.request),
        command_kind: request.command_kind.clone(),
        precondition_fingerprint: request.repository_fingerprint.clone(),
        expected_effects: request.expected_effects.clone(),
        recovery_artifact_hash: request.recovery_artifact.as_deref().map(digest),
    };
    let owner = LockOwner {
        process_id: std::process::id(),
        operation: Some(format!("{}:{}", request.command_kind, request.operation_id)),
    };
    let mut failpoints =
        FailpointEvaluator::from_env().map_err(|error| CoordinationError::Fault {
            source: TransactionFault::Configuration(error.to_string()),
            operation: None,
        })?;
    let mut coordinator = Coordinator::new(
        OsWriterLock::new(git_common_dir),
        store,
        request.lock_timeout,
    );
    let terminal_factory = factory.clone();
    coordinator.execute(
        owner,
        prepared,
        move |boundary| {
            let failpoint = match boundary {
                TransactionBoundary::BeforePrepare => Failpoint::BeforePrepare,
                TransactionBoundary::AfterPrepareBeforeFirstEffect => {
                    Failpoint::AfterPrepareBeforeFirstEffect
                }
                TransactionBoundary::AfterEachEffect => Failpoint::AfterEachEffect,
                TransactionBoundary::BeforeVerify => Failpoint::BeforeVerify,
                TransactionBoundary::AfterVerifyBeforeCommit => Failpoint::AfterVerifyBeforeCommit,
                TransactionBoundary::CommitAmbiguity => Failpoint::CommitAmbiguity,
            };
            failpoints
                .check(failpoint)
                .map_err(|_| TransactionFault::Injected(boundary))
        },
        move |phase, ordinal, head| factory.lifecycle(phase, ordinal, head),
        effects,
        verify,
        move |_prepared, effect, head| {
            let commit = commit(effect)?;
            Ok(terminal_factory.verified_commit(head, commit))
        },
    )
}

#[derive(Clone)]
struct EventFactory {
    operation_id: Uuid,
    repo_id: Uuid,
    actor_id: Uuid,
    actor_kind: ActorKind,
    command_kind: String,
    recorded_at_utc: String,
    repository_fingerprint: Vec<u8>,
    provenance: Vec<u8>,
    request_hash: [u8; 32],
    expected_effects_hash: [u8; 32],
}

impl EventFactory {
    fn new(request: &RuntimeMutationRequest) -> Self {
        Self {
            operation_id: request.operation_id,
            repo_id: request.repo_id,
            actor_id: request.actor_id,
            actor_kind: request.actor_kind,
            command_kind: request.command_kind.clone(),
            recorded_at_utc: request.recorded_at_utc.clone(),
            repository_fingerprint: request.repository_fingerprint.clone(),
            provenance: request.provenance.clone(),
            request_hash: digest(&request.request),
            expected_effects_hash: digest(&request.expected_effects),
        }
    }

    fn lifecycle(&self, phase: LifecycleEvent, ordinal: u32, head: JournalHead) -> EventRecord {
        let (event_type, phase_name) = match phase {
            LifecycleEvent::Prepared => ("OperationPrepared", "prepared"),
            LifecycleEvent::Applying => ("ApplyStarted", "applying"),
            LifecycleEvent::ConflictPaused => ("ConflictPaused", "awaiting_resolution"),
            LifecycleEvent::Aborting => ("AbortStarted", "aborting"),
            LifecycleEvent::Aborted => ("OperationAborted", "aborted"),
            LifecycleEvent::Verifying => ("VerificationStarted", "verifying"),
            LifecycleEvent::RepairRequired => ("RepairRequired", "repair_required"),
        };
        let payload = serde_json::to_vec(&serde_json::json!({
            "command_kind": self.command_kind,
            "phase": phase_name,
            "request_sha256": hex::encode(self.request_hash),
            "expected_effects_sha256": hex::encode(self.expected_effects_hash),
        }))
        .expect("serializing lifecycle metadata cannot fail");
        self.event(
            event_type,
            payload,
            ordinal,
            head,
            self.provenance.clone(),
            Vec::new(),
            None,
        )
    }

    fn verified_commit<P>(
        &self,
        mut head: JournalHead,
        commit: RuntimeMutationCommit<P>,
    ) -> VerifiedCommit<P> {
        let mut events = Vec::with_capacity(commit.facts.len() + 1);
        let mut ordinal = 3_u32;
        for fact in commit.facts {
            let event = self.event(
                &fact.event_type,
                fact.payload,
                ordinal,
                head,
                fact.provenance,
                fact.evidence_manifest,
                fact.dedup_key,
            );
            head.event_hash = event.event_hash;
            events.push(event);
            ordinal += 1;
        }
        let result_hash = digest(&commit.result);
        events.push(
            self.event(
                "OperationCommitted",
                serde_json::to_vec(&serde_json::json!({
                    "command_kind": self.command_kind,
                    "result_sha256": hex::encode(result_hash),
                }))
                .expect("serializing commit metadata cannot fail"),
                ordinal,
                head,
                self.provenance.clone(),
                Vec::new(),
                None,
            ),
        );
        VerifiedCommit {
            events,
            projections: commit.projections,
            result: commit.result,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn event(
        &self,
        event_type: &str,
        payload: Vec<u8>,
        ordinal: u32,
        head: JournalHead,
        provenance: Vec<u8>,
        evidence_manifest: Vec<u8>,
        dedup_key: Option<String>,
    ) -> EventRecord {
        let event_id = Uuid::now_v7();
        let mut record = EventRecord {
            event_id,
            repo_id: self.repo_id,
            event_type: event_type.to_owned(),
            event_schema_version: 1,
            envelope_version: 1,
            operation_id: self.operation_id,
            operation_ordinal: ordinal,
            actor_id: self.actor_id,
            actor_kind: self.actor_kind,
            recorded_at_utc: self.recorded_at_utc.clone(),
            observed_at_utc: None,
            repository_fingerprint: self.repository_fingerprint.clone(),
            payload_codec: PayloadCodec::CanonicalJsonV1,
            payload,
            provenance,
            evidence_manifest,
            dedup_key,
            previous_event_hash: head.event_hash,
            event_hash: [0; 32],
        };
        record.event_hash = event_digest(&record);
        record
    }
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn event_digest(event: &EventRecord) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"jjk-runtime-mutation-event-v1\0");
    hash.update(event.event_id.as_bytes());
    hash.update(event.repo_id.as_bytes());
    hash.update(event.operation_id.as_bytes());
    hash.update(event.operation_ordinal.to_be_bytes());
    hash.update(event.previous_event_hash);
    hash.update((event.event_type.len() as u64).to_be_bytes());
    hash.update(event.event_type.as_bytes());
    hash.update((event.repository_fingerprint.len() as u64).to_be_bytes());
    hash.update(&event.repository_fingerprint);
    hash.update((event.payload.len() as u64).to_be_bytes());
    hash.update(&event.payload);
    hash.update((event.provenance.len() as u64).to_be_bytes());
    hash.update(&event.provenance);
    hash.update((event.evidence_manifest.len() as u64).to_be_bytes());
    hash.update(&event.evidence_manifest);
    hash.finalize().into()
}

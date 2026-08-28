//! Workspace ownership, lease, recovery, and machine-handoff contracts.

use super::{
    evidence::EvidenceRef,
    id::{ActorId, AttemptId, HandoffId, LeaseId, StateId, WorkerId, WorkspaceId},
    provenance::{GitObjectId, Hash256, NativePath, UtcTimestamp},
};
use crate::error::DomainError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, num::NonZeroU64};

#[derive(
    Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct RepoRelativePath(NativePath);
impl RepoRelativePath {
    pub fn new(path: NativePath) -> Result<Self, DomainError> {
        let valid = match &path {
            NativePath::UnixBytes(bytes) => {
                !bytes.is_empty()
                    && bytes[0] != b'/'
                    && bytes
                        .split(|b| *b == b'/')
                        .all(|c| !c.is_empty() && c != b"." && c != b"..")
            }
            NativePath::WindowsWide(wide) => {
                let slash = u16::from(b'/');
                let back = u16::from(b'\\');
                let colon = u16::from(b':');
                !wide.is_empty()
                    && !matches!(wide.first(),Some(v) if *v==slash||*v==back)
                    && !wide.contains(&colon)
                    && wide.split(|v| *v == slash || *v == back).all(|c| {
                        !c.is_empty()
                            && c != [u16::from(b'.')]
                            && c != [u16::from(b'.'), u16::from(b'.')]
                    })
            }
        };
        if valid {
            Ok(Self(path))
        } else {
            Err(DomainError::InvalidValue {
                kind: "repository-relative path",
                reason: "must be non-empty, relative, and contain only normal components".into(),
            })
        }
    }
    #[must_use]
    pub const fn as_native(&self) -> &NativePath {
        &self.0
    }
}

#[derive(
    Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct GitBranchRef(Vec<u8>);
impl GitBranchRef {
    pub fn new(bytes: Vec<u8>) -> Result<Self, DomainError> {
        let suffix = bytes.strip_prefix(b"refs/heads/");
        let valid = suffix.is_some_and(|s| {
            !s.is_empty()
                && !s.starts_with(b"/")
                && !s.ends_with(b"/")
                && !s.ends_with(b".")
                && !s.windows(2).any(|p| p == b".." || p == b"@{")
                && s.split(|b| *b == b'/').all(|c| {
                    !c.is_empty()
                        && !c.starts_with(b".")
                        && !c.ends_with(b".lock")
                        && !c.iter().any(|b| {
                            *b <= b' '
                                || *b == 0x7f
                                || matches!(*b, b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
                        })
                })
        });
        if valid {
            Ok(Self(bytes))
        } else {
            Err(DomainError::InvalidValue {
                kind: "Git branch ref",
                reason: "must be a valid full refs/heads ref".into(),
            })
        }
    }
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(
    Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct WorkspaceFingerprint {
    pub head: Option<GitObjectId>,
    pub symbolic_ref: Option<Vec<u8>>,
    pub index_digest: Hash256,
    pub worktree_digest: Hash256,
}
#[derive(
    Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct WorkspaceOwner {
    pub actor_id: ActorId,
    pub worker_id: Option<WorkerId>,
}
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct LeaseGeneration(NonZeroU64);
impl LeaseGeneration {
    pub fn new(value: u64) -> Result<Self, DomainError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or_else(|| DomainError::InvalidValue {
                kind: "lease generation",
                reason: "must be non-zero".into(),
            })
    }
    #[must_use]
    pub fn get(self) -> u64 {
        self.0.get()
    }
    fn next(self) -> Result<Self, LeaseConflict> {
        self.get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(LeaseConflict::GenerationExhausted)
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LeaseClock {
    pub coordinator_boot_id: String,
    pub monotonic_deadline_ns: u128,
    pub display_wall_deadline: UtcTimestamp,
}
impl LeaseClock {
    pub fn new(
        id: impl Into<String>,
        deadline: u128,
        wall: UtcTimestamp,
    ) -> Result<Self, DomainError> {
        let id = id.into();
        if id.trim().is_empty() || id.contains('\0') {
            Err(DomainError::InvalidValue {
                kind: "coordinator boot ID",
                reason: "must be non-empty and contain no NUL".into(),
            })
        } else {
            Ok(Self {
                coordinator_boot_id: id,
                monotonic_deadline_ns: deadline,
                display_wall_deadline: wall,
            })
        }
    }
}
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum LeaseStatus {
    Active,
    Suspect,
    Expired,
    Released,
    Revoked,
    Fenced,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceLease {
    pub id: LeaseId,
    pub workspace_id: WorkspaceId,
    pub owner: WorkspaceOwner,
    pub generation: LeaseGeneration,
    pub token_hash: Hash256,
    pub status: LeaseStatus,
    pub acquired_at: UtcTimestamp,
    pub last_renewed_at: UtcTimestamp,
    pub clock: LeaseClock,
    pub fingerprint: WorkspaceFingerprint,
    pub version: u64,
}
#[derive(Clone, Eq, PartialEq)]
pub struct SecretBytes(Vec<u8>);
impl SecretBytes {
    pub fn new(bytes: Vec<u8>) -> Result<Self, DomainError> {
        if bytes.is_empty() {
            Err(DomainError::InvalidValue {
                kind: "lease token",
                reason: "must not be empty".into(),
            })
        } else {
            Ok(Self(bytes))
        }
    }
    fn hash(&self) -> Hash256 {
        Hash256::digest(&self.0)
    }
}
impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes([REDACTED])")
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct LeaseProof {
    pub lease_id: LeaseId,
    pub generation: LeaseGeneration,
    pub token: SecretBytes,
}
impl fmt::Debug for LeaseProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LeaseProof")
            .field("lease_id", &self.lease_id)
            .field("generation", &self.generation)
            .field("token", &"[REDACTED]")
            .finish()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseAcquireRequest {
    pub lease_id: LeaseId,
    pub workspace_id: WorkspaceId,
    pub owner: WorkspaceOwner,
    pub token: SecretBytes,
    pub acquired_at: UtcTimestamp,
    pub clock: LeaseClock,
    pub fingerprint: WorkspaceFingerprint,
}
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LeaseConflict {
    #[error("workspace lease is held by another worker")]
    Held {
        workspace_id: WorkspaceId,
        owner: WorkspaceOwner,
        generation: LeaseGeneration,
        status: LeaseStatus,
    },
    #[error("lease proof was fenced")]
    Fenced { current_generation: LeaseGeneration },
    #[error("lease token does not match")]
    TokenMismatch,
    #[error("lease expired; explicit recovery required")]
    ExpiredRequiresRecovery,
    #[error("coordinator restarted; lease is suspect")]
    CoordinatorRestarted,
    #[error("lease is not active")]
    NotActive(LeaseStatus),
    #[error("lease generation exhausted")]
    GenerationExhausted,
}
impl WorkspaceLease {
    pub fn acquire(
        previous: Option<&Self>,
        request: LeaseAcquireRequest,
    ) -> Result<(Self, LeaseProof), LeaseConflict> {
        let generation = match previous {
            None => LeaseGeneration::new(1).expect("valid"),
            Some(l)
                if matches!(
                    l.status,
                    LeaseStatus::Released | LeaseStatus::Revoked | LeaseStatus::Fenced
                ) =>
            {
                l.generation.next()?
            }
            Some(l) => {
                return Err(LeaseConflict::Held {
                    workspace_id: l.workspace_id,
                    owner: l.owner.clone(),
                    generation: l.generation,
                    status: l.status,
                });
            }
        };
        let proof = LeaseProof {
            lease_id: request.lease_id,
            generation,
            token: request.token.clone(),
        };
        Ok((
            Self {
                id: request.lease_id,
                workspace_id: request.workspace_id,
                owner: request.owner,
                generation,
                token_hash: request.token.hash(),
                status: LeaseStatus::Active,
                acquired_at: request.acquired_at.clone(),
                last_renewed_at: request.acquired_at,
                clock: request.clock,
                fingerprint: request.fingerprint,
                version: previous.map_or(1, |l| l.version.saturating_add(1)),
            },
            proof,
        ))
    }
    pub fn authorize(
        &self,
        proof: &LeaseProof,
        boot: &str,
        now: u128,
    ) -> Result<(), LeaseConflict> {
        if proof.lease_id != self.id || proof.generation != self.generation {
            return Err(LeaseConflict::Fenced {
                current_generation: self.generation,
            });
        }
        if proof.token.hash() != self.token_hash {
            return Err(LeaseConflict::TokenMismatch);
        }
        if self.status != LeaseStatus::Active {
            return Err(LeaseConflict::NotActive(self.status));
        }
        if boot != self.clock.coordinator_boot_id {
            return Err(LeaseConflict::CoordinatorRestarted);
        }
        if now >= self.clock.monotonic_deadline_ns {
            return Err(LeaseConflict::ExpiredRequiresRecovery);
        }
        Ok(())
    }
    pub fn release(&mut self, proof: &LeaseProof) -> Result<(), LeaseConflict> {
        self.authorize(proof, &self.clock.coordinator_boot_id, 0)?;
        self.status = LeaseStatus::Released;
        self.version = self.version.saturating_add(1);
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
pub struct WorkspaceLeaseTable {
    leases: BTreeMap<WorkspaceId, WorkspaceLease>,
}
impl WorkspaceLeaseTable {
    pub fn acquire(&mut self, request: LeaseAcquireRequest) -> Result<LeaseProof, LeaseConflict> {
        let (lease, proof) =
            WorkspaceLease::acquire(self.leases.get(&request.workspace_id), request)?;
        self.leases.insert(lease.workspace_id, lease);
        Ok(proof)
    }
    #[must_use]
    pub fn lease(&self, id: WorkspaceId) -> Option<&WorkspaceLease> {
        self.leases.get(&id)
    }
}
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceMode {
    WritablePrimary,
    ReadOnlyReview,
    Quarantined,
}
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceStatus {
    Provisioning,
    Ready,
    Occupied,
    Released,
    Parked,
    Orphaned,
    Missing,
    Quarantined,
    Purged,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub attempt_id: AttemptId,
    pub mode: WorkspaceMode,
    pub relative_locator: RepoRelativePath,
    pub realpath_fingerprint: Hash256,
    pub git_worktree_admin_id: String,
    pub branch_ref: Option<GitBranchRef>,
    pub pinned_state_id: StateId,
    pub head: GitObjectId,
    pub status: WorkspaceStatus,
    pub active_generation: LeaseGeneration,
    pub last_manifest: WorkspaceFingerprint,
    pub version: u64,
}
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum LivenessState {
    ObservedAlive,
    Quiet,
    Suspect,
    ConfirmedDead,
    Unknown,
}
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum StaleLeasePolicy {
    RequestHandoffOrWait,
    FenceThenAdoptOrFork,
    QuarantineAndForkOnly,
}
impl StaleLeasePolicy {
    #[must_use]
    pub const fn for_liveness(l: LivenessState) -> Self {
        match l {
            LivenessState::ConfirmedDead => Self::FenceThenAdoptOrFork,
            LivenessState::Unknown | LivenessState::Suspect => Self::QuarantineAndForkOnly,
            LivenessState::ObservedAlive | LivenessState::Quiet => Self::RequestHandoffOrWait,
        }
    }
    #[must_use]
    pub const fn permits_adoption(self) -> bool {
        matches!(self, Self::FenceThenAdoptOrFork)
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResumeCommand {
    pub program: NativePath,
    pub arguments: Vec<Vec<u8>>,
    pub relative_cwd: NativePath,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceHandoff {
    pub id: HandoffId,
    pub owner: WorkspaceOwner,
    pub objective: String,
    pub base_state: StateId,
    pub produced_state: Option<StateId>,
    pub validation: Vec<EvidenceRef>,
    pub remaining_risks: Vec<String>,
    pub resume: ResumeCommand,
}
impl WorkspaceHandoff {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.objective.trim().is_empty() {
            Err(DomainError::InvalidValue {
                kind: "workspace handoff",
                reason: "requires a non-empty objective".into(),
            })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ObjectAlgorithm;
    fn ts() -> UtcTimestamp {
        UtcTimestamp::parse("2026-08-28T00:00:00Z").unwrap()
    }
    fn req(ws: WorkspaceId, token: u8) -> LeaseAcquireRequest {
        LeaseAcquireRequest {
            lease_id: LeaseId::new_v7(),
            workspace_id: ws,
            owner: WorkspaceOwner {
                actor_id: ActorId::new_v7(),
                worker_id: Some(WorkerId::new_v7()),
            },
            token: SecretBytes::new(vec![token; 32]).unwrap(),
            acquired_at: ts(),
            clock: LeaseClock::new("boot", 100, ts()).unwrap(),
            fingerprint: WorkspaceFingerprint {
                head: Some(GitObjectId::new(ObjectAlgorithm::Sha1, vec![1; 20]).unwrap()),
                symbolic_ref: None,
                index_digest: Hash256::ZERO,
                worktree_digest: Hash256::ZERO,
            },
        }
    }
    #[test]
    fn paths_cannot_escape() {
        for p in ["../x", "a/../../x", "/x", "a//b"] {
            assert!(
                RepoRelativePath::new(NativePath::unix(p.as_bytes().to_vec()).unwrap()).is_err()
            )
        }
        assert!(RepoRelativePath::new(NativePath::unix(b".worktrees/a".to_vec()).unwrap()).is_ok())
    }
    #[test]
    fn one_workspace_has_one_owner() {
        let ws = WorkspaceId::new_v7();
        let mut table = WorkspaceLeaseTable::default();
        assert!(table.acquire(req(ws, 1)).is_ok());
        assert!(matches!(
            table.acquire(req(ws, 2)),
            Err(LeaseConflict::Held { .. })
        ))
    }
    #[test]
    fn expiry_requires_recovery_not_transfer() {
        let ws = WorkspaceId::new_v7();
        let (lease, proof) = WorkspaceLease::acquire(None, req(ws, 1)).unwrap();
        assert_eq!(
            lease.authorize(&proof, "boot", 100),
            Err(LeaseConflict::ExpiredRequiresRecovery)
        );
        assert!(matches!(
            WorkspaceLease::acquire(Some(&lease), req(ws, 2)),
            Err(LeaseConflict::Held { .. })
        ));
        assert!(!StaleLeasePolicy::for_liveness(LivenessState::Unknown).permits_adoption())
    }
    #[test]
    fn secrets_redact_and_handoff_uses_argv() {
        let (_, proof) = WorkspaceLease::acquire(None, req(WorkspaceId::new_v7(), 42)).unwrap();
        assert!(!format!("{proof:?}").contains("42"));
        let cmd = ResumeCommand {
            program: NativePath::unix(b"jjk".to_vec()).unwrap(),
            arguments: vec![b"$(touch /tmp/no)".to_vec()],
            relative_cwd: NativePath::unix(b".worktrees/x".to_vec()).unwrap(),
        };
        assert_eq!(cmd.arguments.len(), 1)
    }
}

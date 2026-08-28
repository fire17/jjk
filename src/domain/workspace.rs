use std::num::NonZeroU64;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use crate::error::DomainError;
use super::{evidence::EvidenceRef,id::{ActorId,AttemptId,HandoffId,StateId,WorkerId,WorkspaceId},provenance::{GitObjectId,Hash256,NativePath,UtcTimestamp}};
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] pub struct WorkspaceFingerprint { pub head: Option<GitObjectId>, pub symbolic_ref: Option<Vec<u8>>, pub index_digest: Hash256, pub worktree_digest: Hash256 }
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] pub struct WorkspaceOwner { pub actor_id: ActorId, pub worker_id: Option<WorkerId> }
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] #[serde(transparent)] pub struct LeaseEpoch(NonZeroU64);
impl LeaseEpoch { pub fn new(value:u64)->Result<Self,DomainError>{NonZeroU64::new(value).map(Self).ok_or_else(||DomainError::InvalidValue{kind:"lease epoch",reason:"must be non-zero".into()})} #[must_use] pub fn get(self)->u64{self.0.get()} }
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)] pub struct WorkspaceLease { pub workspace_id: WorkspaceId, pub owner: WorkspaceOwner, pub epoch: LeaseEpoch, pub acquired_at: UtcTimestamp, pub expires_at: UtcTimestamp, pub fingerprint: WorkspaceFingerprint }
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)] pub struct Workspace { pub id: WorkspaceId, pub attempt_id: AttemptId, pub relative_locator: NativePath, pub active_state_id: Option<StateId>, pub lease: Option<WorkspaceLease> }
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)] pub struct ResumeCommand { pub program: NativePath, pub arguments: Vec<Vec<u8>>, pub relative_cwd: NativePath }
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)] pub struct WorkspaceHandoff { pub id: HandoffId, pub owner: WorkspaceOwner, pub objective: String, pub base_state: StateId, pub produced_state: Option<StateId>, pub validation: Vec<EvidenceRef>, pub remaining_risks: Vec<String>, pub resume: ResumeCommand }

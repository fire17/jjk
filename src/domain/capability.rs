use super::id::RepoId;
use crate::error::DomainError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityKind {
    Git,
    Jujutsu,
    Sha256Repositories,
    Worktrees,
    AtomicRefTransactions,
    Symlinks,
    FileMode,
}
#[derive(
    Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct DegradationReason {
    pub code: String,
    pub message: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum CapabilityStatus {
    Available { version: Option<String> },
    Unavailable { reason: DegradationReason },
    Degraded { reason: DegradationReason },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityReport {
    pub repo_id: RepoId,
    pub capabilities: BTreeMap<CapabilityKind, CapabilityStatus>,
}
impl CapabilityReport {
    #[must_use]
    pub fn new(repo_id: RepoId) -> Self {
        Self {
            repo_id,
            capabilities: BTreeMap::new(),
        }
    }
    pub fn set(&mut self, kind: CapabilityKind, status: CapabilityStatus) {
        self.capabilities.insert(kind, status);
    }
    #[must_use]
    pub fn status(&self, kind: CapabilityKind) -> Option<&CapabilityStatus> {
        self.capabilities.get(&kind)
    }
    pub fn require(&self, kind: CapabilityKind) -> Result<(), DomainError> {
        match self.status(kind) {
            Some(CapabilityStatus::Available { .. }) => Ok(()),
            Some(CapabilityStatus::Degraded { reason })
            | Some(CapabilityStatus::Unavailable { reason }) => {
                Err(DomainError::CapabilityUnavailable {
                    capability: format!("{kind:?}"),
                    reason: reason.message.clone(),
                })
            }
            None => Err(DomainError::CapabilityUnavailable {
                capability: format!("{kind:?}"),
                reason: "not probed".into(),
            }),
        }
    }
}

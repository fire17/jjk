use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::DomainError;

use super::id::{ActorId, AttemptId, BranchId, OperationId, StateId, WorkspaceId};

#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Active,
    Candidate,
    Chosen,
    Rejected,
    Parked,
    Archived,
}

#[derive(
    Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct Attempt {
    pub id: AttemptId,
    pub base_state_id: Option<StateId>,
    pub current_tip_state_id: Option<StateId>,
    pub objective: String,
    pub status: AttemptStatus,
    pub owner: Option<ActorId>,
    pub created_by: Option<OperationId>,
    pub branch_id: Option<BranchId>,
    pub workspace_id: Option<WorkspaceId>,
}

impl Attempt {
    pub fn new(
        id: AttemptId,
        first_state_id: StateId,
        objective: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let mut attempt = Self::empty_root(id, objective)?;
        attempt.current_tip_state_id = Some(first_state_id);
        Ok(attempt)
    }

    pub fn empty_root(id: AttemptId, objective: impl Into<String>) -> Result<Self, DomainError> {
        Self::build(id, None, objective)
    }

    pub fn fork(
        id: AttemptId,
        base_state_id: StateId,
        objective: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Self::build(id, Some(base_state_id), objective)
    }

    fn build(
        id: AttemptId,
        base_state_id: Option<StateId>,
        objective: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let objective = objective.into();
        if objective.trim().is_empty() {
            return Err(DomainError::InvalidValue {
                kind: "attempt objective",
                reason: "must be non-empty".into(),
            });
        }
        Ok(Self {
            id,
            base_state_id,
            current_tip_state_id: None,
            objective,
            status: AttemptStatus::Active,
            owner: None,
            created_by: None,
            branch_id: None,
            workspace_id: None,
        })
    }

    #[must_use]
    pub const fn archived(&self) -> bool {
        matches!(self.status, AttemptStatus::Archived)
    }
}

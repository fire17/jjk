use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use crate::error::DomainError;
use super::id::{AttemptId, BranchId, StateId, WorkspaceId};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Attempt { pub id: AttemptId, pub root_state_id: StateId, pub current_tip_state_id: Option<StateId>, pub objective: String, pub branch_id: Option<BranchId>, pub workspace_id: Option<WorkspaceId>, pub archived: bool }
impl Attempt { pub fn new(id: AttemptId, root_state_id: StateId, objective: impl Into<String>) -> Result<Self, DomainError> { let objective = objective.into(); if objective.trim().is_empty() { return Err(DomainError::InvalidValue { kind: "attempt objective", reason: "must be non-empty".into() }); } Ok(Self { id, root_state_id, current_tip_state_id: Some(root_state_id), objective, branch_id: None, workspace_id: None, archived: false }) } }

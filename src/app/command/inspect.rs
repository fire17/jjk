//! Snapshot-consistent inspection helpers, including workspace divergence checks.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use crate::domain::{WorkspaceFingerprint, WorkspaceId};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceDivergence { Matching, Missing, IdentityMismatch, ContentsChanged }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceInspection {
    pub workspace_id: WorkspaceId,
    pub registered: WorkspaceFingerprint,
    pub observed: Option<WorkspaceFingerprint>,
    pub divergence: WorkspaceDivergence,
    pub mutation_allowed: bool,
}

#[must_use]
pub fn inspect_workspace(workspace_id: WorkspaceId, registered: WorkspaceFingerprint, observed: Option<WorkspaceFingerprint>) -> WorkspaceInspection {
    let divergence = match observed.as_ref() {
        None => WorkspaceDivergence::Missing,
        Some(value) if value == &registered => WorkspaceDivergence::Matching,
        Some(value) if value.head != registered.head || value.symbolic_ref != registered.symbolic_ref => WorkspaceDivergence::IdentityMismatch,
        Some(_) => WorkspaceDivergence::ContentsChanged,
    };
    let mutation_allowed = divergence == WorkspaceDivergence::Matching;
    WorkspaceInspection { workspace_id, registered, observed, divergence, mutation_allowed }
}

use crate::app::query::{CurrentReadModel, DiffReadModel, DiffScope, QueryError, QueryService, ReadSnapshotSource, ShowReadModel, StatusReadModel};
use crate::domain::StateId;

pub fn current(source: &impl ReadSnapshotSource) -> Result<CurrentReadModel, QueryError> { QueryService::new(source).current() }
pub fn status(source: &impl ReadSnapshotSource) -> Result<StatusReadModel, QueryError> { QueryService::new(source).status() }
pub fn show(source: &impl ReadSnapshotSource, state: StateId) -> Result<ShowReadModel, QueryError> { QueryService::new(source).show(state) }
pub fn diff(source: &impl ReadSnapshotSource, from: Option<StateId>, to: StateId, atomic: bool) -> Result<DiffReadModel, QueryError> {
    QueryService::new(source).diff(from, to, if atomic { DiffScope::Atomic } else { DiffScope::FullSnapshot })
}

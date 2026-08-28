use crate::app::query::{CurrentReadModel, DiffReadModel, DiffScope, QueryError, QueryService, ReadSnapshotSource, ShowReadModel, StatusReadModel};
use crate::domain::StateId;

pub fn current(source: &impl ReadSnapshotSource) -> Result<CurrentReadModel, QueryError> { QueryService::new(source).current() }
pub fn status(source: &impl ReadSnapshotSource) -> Result<StatusReadModel, QueryError> { QueryService::new(source).status() }
pub fn show(source: &impl ReadSnapshotSource, state: StateId) -> Result<ShowReadModel, QueryError> { QueryService::new(source).show(state) }
pub fn diff(source: &impl ReadSnapshotSource, from: Option<StateId>, to: StateId, atomic: bool) -> Result<DiffReadModel, QueryError> {
    QueryService::new(source).diff(from, to, if atomic { DiffScope::Atomic } else { DiffScope::FullSnapshot })
}

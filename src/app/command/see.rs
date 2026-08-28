use crate::app::query::{GraphReadModel, QueryError, QueryService, ReadSnapshotSource, StoryReadModel};

pub fn graph(source: &impl ReadSnapshotSource, include_archived: bool) -> Result<GraphReadModel, QueryError> {
    QueryService::new(source).graph(include_archived)
}
pub fn story(source: &impl ReadSnapshotSource) -> Result<StoryReadModel, QueryError> { QueryService::new(source).story() }

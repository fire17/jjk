use uuid::Uuid;
use crate::ports::ids::IdSource;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UuidV7Source;
impl IdSource for UuidV7Source { fn new_v7(&self) -> Uuid { Uuid::now_v7() } }

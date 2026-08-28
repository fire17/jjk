use crate::ports::journal::JournalHead;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectionRecord {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub last_event_seq: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectionSnapshot {
    pub projection_name: String,
    pub reducer_version: u32,
    pub head: JournalHead,
    pub digest: [u8; 32],
    pub records: Vec<ProjectionRecord>,
}

/// One deterministic projection write tied to an event in the same append batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectionUpdate {
    pub projection_name: String,
    pub reducer_version: u32,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub event_index: usize,
}

pub(crate) trait ProjectionStore {
    type Error;

    fn projection_snapshot(
        &self,
        projection_name: &str,
        reducer_version: u32,
    ) -> Result<ProjectionSnapshot, Self::Error>;
}

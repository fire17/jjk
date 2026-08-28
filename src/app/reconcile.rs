#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReconciliationWatermark {
    pub repository_fingerprint: Vec<u8>,
    pub observed_through_seq: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReconciliationNeed { Current, Reconcile }

pub(crate) fn need(previous: Option<&ReconciliationWatermark>, current_fingerprint: &[u8]) -> ReconciliationNeed {
    match previous {
        Some(previous) if previous.repository_fingerprint == current_fingerprint => ReconciliationNeed::Current,
        _ => ReconciliationNeed::Reconcile,
    }
}

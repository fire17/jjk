use crate::ports::operation::{OperationRecord, OperationStatus, OperationStore};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RepairDisposition {
    AbortUnapplied,
    InspectAndResume,
    ResumeVerification,
    AwaitExplicitResolution,
    RestoreThenAbort,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingRepair {
    pub operation: OperationRecord,
    pub disposition: RepairDisposition,
}

pub(crate) fn discover<S: OperationStore>(store: &S) -> Result<Vec<PendingRepair>, S::Error> {
    store.pending_operations().map(|operations| operations.into_iter().map(|operation| {
        let disposition = classify(operation.status);
        PendingRepair { operation, disposition }
    }).collect())
}

pub(crate) const fn classify(status: OperationStatus) -> RepairDisposition {
    match status {
        OperationStatus::Prepared => RepairDisposition::AbortUnapplied,
        OperationStatus::Applying => RepairDisposition::InspectAndResume,
        OperationStatus::AwaitingResolution | OperationStatus::RepairRequired => RepairDisposition::AwaitExplicitResolution,
        OperationStatus::Verifying => RepairDisposition::ResumeVerification,
        OperationStatus::Aborting => RepairDisposition::RestoreThenAbort,
        OperationStatus::Committed | OperationStatus::Aborted => unreachable_terminal(),
    }
}

const fn unreachable_terminal() -> ! { panic!("terminal operations are not pending") }

use crate::ports::operation::{
    OperationRecord, OperationStatus, OperationStore, RecoveryDisposition,
};

pub(crate) type RepairDisposition = RecoveryDisposition;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingRepair {
    pub operation: OperationRecord,
    pub disposition: RepairDisposition,
}

pub(crate) fn discover<S: OperationStore>(store: &S) -> Result<Vec<PendingRepair>, S::Error> {
    store.recovery_candidates().map(|operations| {
        operations
            .into_iter()
            .map(|candidate| PendingRepair {
                operation: candidate.operation,
                disposition: candidate.disposition,
            })
            .collect()
    })
}

pub(crate) const fn classify(status: OperationStatus) -> RepairDisposition {
    match status.recovery_disposition() {
        Some(disposition) => disposition,
        None => unreachable_terminal(),
    }
}

const fn unreachable_terminal() -> ! {
    panic!("terminal operations are not pending")
}

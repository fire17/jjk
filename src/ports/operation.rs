use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationStatus {
    Prepared,
    Applying,
    AwaitingResolution,
    Verifying,
    Committed,
    Aborting,
    Aborted,
    RepairRequired,
}

impl OperationStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Applying => "applying",
            Self::AwaitingResolution => "awaiting_resolution",
            Self::Verifying => "verifying",
            Self::Committed => "committed",
            Self::Aborting => "aborting",
            Self::Aborted => "aborted",
            Self::RepairRequired => "repair_required",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "prepared" => Self::Prepared,
            "applying" => Self::Applying,
            "awaiting_resolution" => Self::AwaitingResolution,
            "verifying" => Self::Verifying,
            "committed" => Self::Committed,
            "aborting" => Self::Aborting,
            "aborted" => Self::Aborted,
            "repair_required" => Self::RepairRequired,
            _ => return None,
        })
    }

    pub(crate) const fn is_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::Aborted)
    }

    pub(crate) const fn recovery_disposition(self) -> Option<RecoveryDisposition> {
        match self {
            Self::Prepared => Some(RecoveryDisposition::AbortUnapplied),
            Self::Applying => Some(RecoveryDisposition::InspectAndResume),
            Self::AwaitingResolution | Self::RepairRequired => {
                Some(RecoveryDisposition::AwaitExplicitResolution)
            }
            Self::Verifying => Some(RecoveryDisposition::ResumeVerification),
            Self::Aborting => Some(RecoveryDisposition::RestoreThenAbort),
            Self::Committed | Self::Aborted => None,
        }
    }

    pub(crate) const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Prepared,
                Self::Applying | Self::Aborting | Self::RepairRequired
            ) | (
                Self::Applying,
                Self::Applying
                    | Self::AwaitingResolution
                    | Self::Verifying
                    | Self::Aborting
                    | Self::RepairRequired
            ) | (
                Self::AwaitingResolution,
                Self::Applying | Self::Aborting | Self::RepairRequired
            ) | (Self::Verifying, Self::Committed | Self::RepairRequired)
                | (Self::Aborting, Self::Aborted | Self::RepairRequired)
                | (
                    Self::RepairRequired,
                    Self::Applying | Self::Verifying | Self::Aborting
                )
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryDisposition {
    AbortUnapplied,
    InspectAndResume,
    ResumeVerification,
    AwaitExplicitResolution,
    RestoreThenAbort,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreparedOperationRecord {
    pub operation_id: Uuid,
    pub request_hash: [u8; 32],
    pub command_kind: String,
    pub precondition_fingerprint: Vec<u8>,
    pub expected_effects: Vec<u8>,
    pub recovery_artifact_hash: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OperationRecord {
    pub operation_id: Uuid,
    pub request_hash: [u8; 32],
    pub command_kind: String,
    pub status: OperationStatus,
    pub prepared_seq: u64,
    pub terminal_seq: Option<u64>,
    pub precondition_fingerprint: Vec<u8>,
    pub expected_effects: Vec<u8>,
    pub recovery_artifact_hash: Option<[u8; 32]>,
    pub result: Option<Vec<u8>>,
    pub last_event_seq: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryCandidate {
    pub operation: OperationRecord,
    pub disposition: RecoveryDisposition,
}

pub(crate) trait OperationStore {
    type Error;

    fn operation(&self, operation_id: Uuid) -> Result<Option<OperationRecord>, Self::Error>;
    fn pending_operations(&self) -> Result<Vec<OperationRecord>, Self::Error>;

    fn recovery_candidates(&self) -> Result<Vec<RecoveryCandidate>, Self::Error> {
        self.pending_operations().map(|operations| {
            operations
                .into_iter()
                .filter_map(|operation| {
                    operation
                        .status
                        .recovery_disposition()
                        .map(|disposition| RecoveryCandidate {
                            operation,
                            disposition,
                        })
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_table_rejects_terminal_and_skipped_states() {
        assert!(OperationStatus::Prepared.can_transition_to(OperationStatus::Applying));
        assert!(OperationStatus::Applying.can_transition_to(OperationStatus::Applying));
        assert!(OperationStatus::Applying.can_transition_to(OperationStatus::Verifying));
        assert!(OperationStatus::Verifying.can_transition_to(OperationStatus::Committed));
        assert!(!OperationStatus::Prepared.can_transition_to(OperationStatus::Committed));
        assert!(!OperationStatus::Committed.can_transition_to(OperationStatus::Applying));
        assert!(!OperationStatus::Aborted.can_transition_to(OperationStatus::RepairRequired));
    }

    #[test]
    fn every_nonterminal_status_has_a_recovery_disposition() {
        for status in [
            OperationStatus::Prepared,
            OperationStatus::Applying,
            OperationStatus::AwaitingResolution,
            OperationStatus::Verifying,
            OperationStatus::Aborting,
            OperationStatus::RepairRequired,
        ] {
            assert!(status.recovery_disposition().is_some(), "{status:?}");
        }
        assert_eq!(OperationStatus::Committed.recovery_disposition(), None);
        assert_eq!(OperationStatus::Aborted.recovery_disposition(), None);
    }
}

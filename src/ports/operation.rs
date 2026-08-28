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

    pub(crate) const fn is_pending(self) -> bool {
        !self.is_terminal()
    }

    pub(crate) const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Prepared, Self::Applying | Self::Aborting | Self::RepairRequired)
                | (Self::Applying, Self::AwaitingResolution | Self::Verifying | Self::Aborting | Self::RepairRequired)
                | (Self::AwaitingResolution, Self::Applying | Self::Aborting | Self::RepairRequired)
                | (Self::Verifying, Self::Committed | Self::RepairRequired)
                | (Self::Aborting, Self::Aborted | Self::RepairRequired)
                | (Self::RepairRequired, Self::Applying | Self::Verifying | Self::Aborting)
        )
    }
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

pub(crate) trait OperationStore {
    type Error;

    fn operation(&self, operation_id: Uuid) -> Result<Option<OperationRecord>, Self::Error>;
    fn pending_operations(&self) -> Result<Vec<OperationRecord>, Self::Error>;
}

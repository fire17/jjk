use thiserror::Error;

/// Errors produced by pure domain validation and replay.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    #[error("invalid {kind}: {reason}")]
    InvalidValue { kind: &'static str, reason: String },
    #[error("identifier prefix mismatch: expected {expected}, found {found}")]
    IdPrefixMismatch {
        expected: &'static str,
        found: String,
    },
    #[error("identifier is not a UUIDv7")]
    NotUuidV7,
    #[error("duplicate {kind} `{id}`")]
    Duplicate { kind: &'static str, id: String },
    #[error("missing {kind} `{id}`")]
    Missing { kind: &'static str, id: String },
    #[error("illegal graph edge: {reason}")]
    IllegalEdge { reason: String },
    #[error("logical-parent edge would create a cycle")]
    LogicalParentCycle,
    #[error("state already has a logical parent")]
    LogicalParentCardinality,
    #[error("illegal operation phase transition from {from} via {transition}")]
    IllegalOperationTransition { from: String, transition: String },
    #[error("operation effect ordinals must be contiguous from zero")]
    NonContiguousEffects,
    #[error("event sequence gap: expected {expected}, found {found}")]
    EventSequenceGap { expected: u64, found: u64 },
    #[error("event hash chain mismatch at sequence {sequence}")]
    EventHashMismatch { sequence: u64 },
    #[error("event operation ordinal gap: expected {expected}, found {found}")]
    OperationOrdinalGap { expected: u32, found: u32 },
    #[error("event payload does not match declared event type")]
    EventTypeMismatch,
    #[error("projection invariant failed: {reason}")]
    ProjectionInvariant { reason: String },
    #[error("capability `{capability}` is unavailable: {reason}")]
    CapabilityUnavailable { capability: String, reason: String },
    #[error("conflicting effect receipt for `{effect_id}`")]
    EffectReceiptConflict { effect_id: String },
}

/// Stable top-level error type. More boundary-specific variants are added by outer layers.
#[derive(Debug, Error)]
pub enum JjkError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("JJK is not yet initialized")]
    NotInitialized,
    #[error("the repository requires recovery before this operation")]
    RecoveryRequired,
}

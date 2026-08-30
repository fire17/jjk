use uuid::Uuid;

/// The zero hash anchoring a new journal generation.
pub(crate) const GENESIS_HASH: [u8; 32] = [0; 32];

/// Immutable event bytes accepted by a journal implementation.
///
/// Domain code owns construction and validation of typed envelopes. This port carries the
/// persistence representation without exposing an adapter or SQL type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EventRecord {
    pub event_id: Uuid,
    pub repo_id: Uuid,
    pub event_type: String,
    pub event_schema_version: u16,
    pub envelope_version: u16,
    pub operation_id: Uuid,
    pub operation_ordinal: u32,
    pub actor_id: Uuid,
    pub actor_kind: ActorKind,
    pub recorded_at_utc: String,
    pub observed_at_utc: Option<String>,
    pub repository_fingerprint: Vec<u8>,
    pub payload_codec: PayloadCodec,
    pub payload: Vec<u8>,
    pub provenance: Vec<u8>,
    pub evidence_manifest: Vec<u8>,
    pub dedup_key: Option<String>,
    pub previous_event_hash: [u8; 32],
    pub event_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActorKind {
    Human,
    Agent,
    System,
    Import,
}

impl ActorKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::System => "system",
            Self::Import => "import",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PayloadCodec {
    CanonicalCborV1,
    CanonicalJsonV1,
}

impl PayloadCodec {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalCborV1 => "cbor-canonical-v1",
            Self::CanonicalJsonV1 => "json-canonical-v1",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct JournalHead {
    pub local_seq: u64,
    pub event_hash: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StoredEvent {
    pub local_seq: u64,
    pub record: EventRecord,
}

impl EventRecord {
    pub(crate) fn validate_for_append(
        &self,
        expected_repo_id: Uuid,
        expected_envelope_version: u16,
    ) -> Result<(), &'static str> {
        if self.repo_id != expected_repo_id {
            return Err("event belongs to a different repository");
        }
        if self.event_schema_version == 0 {
            return Err("event schema version must be positive");
        }
        if self.envelope_version != expected_envelope_version {
            return Err("event envelope version is unsupported");
        }
        if self.event_type.is_empty() {
            return Err("event type must not be empty");
        }
        if self.recorded_at_utc.is_empty() {
            return Err("recorded timestamp must not be empty");
        }
        if self.event_hash == GENESIS_HASH {
            return Err("event hash must not be the genesis hash");
        }
        Ok(())
    }
}

pub(crate) trait Journal {
    type Error;

    fn head(&self) -> Result<JournalHead, Self::Error>;
    fn events_after(&self, local_seq: u64, limit: usize) -> Result<Vec<StoredEvent>, Self::Error>;
}

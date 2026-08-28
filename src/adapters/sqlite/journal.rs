use rusqlite::{Connection, OptionalExtension, Transaction, params};
use uuid::Uuid;

use super::{SqliteStore, StoreError};
use crate::ports::journal::{
    ActorKind, EventRecord, GENESIS_HASH, Journal, JournalHead, PayloadCodec, StoredEvent,
};

pub(super) fn head_in(connection: &Connection) -> Result<JournalHead, StoreError> {
    let head: Option<(u64, Vec<u8>)> = connection
        .query_row(
            "SELECT local_seq, event_hash FROM events ORDER BY local_seq DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match head {
        None => Ok(JournalHead {
            local_seq: 0,
            event_hash: GENESIS_HASH,
        }),
        Some((local_seq, hash)) => Ok(JournalHead {
            local_seq,
            event_hash: array::<32>(hash, "event_hash")?,
        }),
    }
}

pub(super) fn insert(tx: &Transaction<'_>, event: &EventRecord) -> Result<u64, StoreError> {
    tx.execute(
        "INSERT INTO events (event_id, repo_id, event_type, event_schema_version, envelope_version, operation_id, operation_ordinal, actor_id, actor_kind, recorded_at_utc, observed_at_utc, repository_fingerprint, payload_codec, payload, provenance, evidence_manifest, dedup_key, previous_event_hash, event_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        params![event.event_id.as_bytes(), event.repo_id.as_bytes(), event.event_type, event.event_schema_version, event.envelope_version, event.operation_id.as_bytes(), event.operation_ordinal, event.actor_id.as_bytes(), event.actor_kind.as_str(), event.recorded_at_utc, event.observed_at_utc, event.repository_fingerprint, event.payload_codec.as_str(), event.payload, event.provenance, event.evidence_manifest, event.dedup_key, event.previous_event_hash.as_slice(), event.event_hash.as_slice()],
    )?;
    Ok(u64::try_from(tx.last_insert_rowid())
        .map_err(|_| StoreError::InvalidData("negative event sequence".into()))?)
}

impl Journal for SqliteStore {
    type Error = StoreError;

    fn head(&self) -> Result<JournalHead, Self::Error> {
        head_in(&self.connection)
    }

    fn events_after(&self, local_seq: u64, limit: usize) -> Result<Vec<StoredEvent>, Self::Error> {
        let limit = i64::try_from(limit)
            .map_err(|_| StoreError::InvalidData("event query limit is too large".into()))?;
        let mut statement = self.connection.prepare("SELECT local_seq, event_id, repo_id, event_type, event_schema_version, envelope_version, operation_id, operation_ordinal, actor_id, actor_kind, recorded_at_utc, observed_at_utc, repository_fingerprint, payload_codec, payload, provenance, evidence_manifest, dedup_key, previous_event_hash, event_hash FROM events WHERE local_seq > ?1 ORDER BY local_seq LIMIT ?2")?;
        let rows = statement.query_map(params![local_seq, limit], decode)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }
}

fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredEvent> {
    let actor: String = row.get(9)?;
    let codec: String = row.get(13)?;
    Ok(StoredEvent {
        local_seq: row.get(0)?,
        record: EventRecord {
            event_id: uuid(row.get(1)?)?,
            repo_id: uuid(row.get(2)?)?,
            event_type: row.get(3)?,
            event_schema_version: row.get(4)?,
            envelope_version: row.get(5)?,
            operation_id: uuid(row.get(6)?)?,
            operation_ordinal: row.get(7)?,
            actor_id: uuid(row.get(8)?)?,
            actor_kind: match actor.as_str() {
                "human" => ActorKind::Human,
                "agent" => ActorKind::Agent,
                "system" => ActorKind::System,
                "import" => ActorKind::Import,
                _ => return Err(rusqlite::Error::InvalidQuery),
            },
            recorded_at_utc: row.get(10)?,
            observed_at_utc: row.get(11)?,
            repository_fingerprint: row.get(12)?,
            payload_codec: match codec.as_str() {
                "cbor-canonical-v1" => PayloadCodec::CanonicalCborV1,
                "json-canonical-v1" => PayloadCodec::CanonicalJsonV1,
                _ => return Err(rusqlite::Error::InvalidQuery),
            },
            payload: row.get(14)?,
            provenance: row.get(15)?,
            evidence_manifest: row.get(16)?,
            dedup_key: row.get(17)?,
            previous_event_hash: array_sql(row.get(18)?)?,
            event_hash: array_sql(row.get(19)?)?,
        },
    })
}

fn uuid(bytes: Vec<u8>) -> rusqlite::Result<Uuid> {
    Uuid::from_slice(&bytes).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            bytes.len(),
            rusqlite::types::Type::Blob,
            Box::new(error),
        )
    })
}
fn array_sql<const N: usize>(bytes: Vec<u8>) -> rusqlite::Result<[u8; N]> {
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        rusqlite::Error::FromSqlConversionFailure(
            bytes.len(),
            rusqlite::types::Type::Blob,
            "invalid fixed blob length".into(),
        )
    })
}
fn array<const N: usize>(bytes: Vec<u8>, field: &str) -> Result<[u8; N], StoreError> {
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        StoreError::InvalidData(format!("{field} has length {}, expected {N}", bytes.len()))
    })
}

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use uuid::Uuid;

use super::StoreError;
use crate::ports::operation::{OperationRecord, OperationStatus, OperationStore, PreparedOperationRecord};
use super::SqliteStore;

pub(super) fn insert_prepared(tx: &Transaction<'_>, operation: &PreparedOperationRecord, seq: u64) -> Result<(), StoreError> {
    tx.execute("INSERT INTO operations (operation_id, request_hash, command_kind, status, prepared_seq, precondition_fingerprint, expected_effects, recovery_artifact_hash, last_event_seq) VALUES (?1, ?2, ?3, 'prepared', ?4, ?5, ?6, ?7, ?4)", params![operation.operation_id.as_bytes(), operation.request_hash.as_slice(), operation.command_kind, seq, operation.precondition_fingerprint, operation.expected_effects, operation.recovery_artifact_hash.as_ref().map(<[u8;32]>::as_slice)])?;
    Ok(())
}

pub(super) fn transition(tx: &Transaction<'_>, operation_id: Uuid, status: OperationStatus, seq: u64, result: Option<&[u8]>) -> Result<(), StoreError> {
    let terminal = status.is_terminal().then_some(seq);
    let changed = tx.execute("UPDATE operations SET status = ?2, terminal_seq = ?3, result = COALESCE(?4, result), last_event_seq = ?5 WHERE operation_id = ?1", params![operation_id.as_bytes(), status.as_str(), terminal, result, seq])?;
    if changed != 1 { return Err(StoreError::InvalidData(format!("operation {operation_id} does not exist"))); }
    Ok(())
}

pub(super) fn get(connection: &Connection, operation_id: Uuid) -> Result<Option<OperationRecord>, StoreError> {
    connection.query_row("SELECT operation_id, request_hash, command_kind, status, prepared_seq, terminal_seq, precondition_fingerprint, expected_effects, recovery_artifact_hash, result, last_event_seq FROM operations WHERE operation_id = ?1", params![operation_id.as_bytes()], decode).optional().map_err(StoreError::from)
}

impl OperationStore for SqliteStore {
    type Error = StoreError;
    fn operation(&self, operation_id: Uuid) -> Result<Option<OperationRecord>, Self::Error> { get(&self.connection, operation_id) }
    fn pending_operations(&self) -> Result<Vec<OperationRecord>, Self::Error> {
        let mut statement = self.connection.prepare("SELECT operation_id, request_hash, command_kind, status, prepared_seq, terminal_seq, precondition_fingerprint, expected_effects, recovery_artifact_hash, result, last_event_seq FROM operations WHERE status NOT IN ('committed', 'aborted') ORDER BY prepared_seq")?;
        statement.query_map([], decode)?.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }
}

fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<OperationRecord> {
    let id: Vec<u8> = row.get(0)?; let request: Vec<u8> = row.get(1)?; let status: String = row.get(3)?; let recovery: Option<Vec<u8>> = row.get(8)?;
    Ok(OperationRecord {
        operation_id: Uuid::from_slice(&id).map_err(|error| convert(id.len(), error))?,
        request_hash: request.try_into().map_err(|value: Vec<u8>| convert(value.len(), std::io::Error::new(std::io::ErrorKind::InvalidData, "request hash length")))?,
        command_kind: row.get(2)?, status: OperationStatus::parse(&status).ok_or(rusqlite::Error::InvalidQuery)?, prepared_seq: row.get(4)?, terminal_seq: row.get(5)?,
        precondition_fingerprint: row.get(6)?, expected_effects: row.get(7)?,
        recovery_artifact_hash: recovery.map(|value| value.try_into().map_err(|value: Vec<u8>| convert(value.len(), std::io::Error::new(std::io::ErrorKind::InvalidData, "recovery hash length")))).transpose()?,
        result: row.get(9)?, last_event_seq: row.get(10)?,
    })
}
fn convert<E: std::error::Error + Send + Sync + 'static>(length: usize, error: E) -> rusqlite::Error { rusqlite::Error::FromSqlConversionFailure(length, rusqlite::types::Type::Blob, Box::new(error)) }

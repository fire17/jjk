use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};

use super::{SqliteStore, StoreError};
use crate::ports::projection::{ProjectionRecord, ProjectionSnapshot, ProjectionStore};

pub(super) fn advance_all(tx: &Transaction<'_>, seq: u64, hash: [u8; 32]) -> Result<(), StoreError> {
    let mut names = tx.prepare("SELECT projection_name FROM projection_meta")?;
    let names = names.query_map([], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    for name in names {
        let digest = digest(tx, &name)?;
        tx.execute("UPDATE projection_meta SET projected_through_seq = ?2, projected_through_hash = ?3, projection_digest = ?4 WHERE projection_name = ?1", params![name, seq, hash.as_slice(), digest.as_slice()])?;
    }
    Ok(())
}

pub(crate) fn put_record(tx: &Transaction<'_>, projection: &str, reducer_version: u32, key: &[u8], value: &[u8], seq: u64) -> Result<(), StoreError> {
    tx.execute("INSERT INTO projection_meta (projection_name, reducer_version, projected_through_seq, projected_through_hash, projection_digest) VALUES (?1, ?2, 0, zeroblob(32), zeroblob(32)) ON CONFLICT(projection_name) DO NOTHING", params![projection, reducer_version])?;
    let actual: u32 = tx.query_row("SELECT reducer_version FROM projection_meta WHERE projection_name = ?1", [projection], |row| row.get(0))?;
    if actual != reducer_version { return Err(StoreError::ReducerVersionMismatch { name: projection.into(), expected: reducer_version, actual }); }
    tx.execute("INSERT INTO projection_records (projection_name, record_key, record_value, last_event_seq) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(projection_name, record_key) DO UPDATE SET record_value=excluded.record_value, last_event_seq=excluded.last_event_seq WHERE projection_records.last_event_seq < excluded.last_event_seq", params![projection, key, value, seq])?;
    Ok(())
}

impl ProjectionStore for SqliteStore {
    type Error = StoreError;
    fn projection_snapshot(&self, name: &str, reducer_version: u32) -> Result<ProjectionSnapshot, Self::Error> {
        let tx = self.connection.unchecked_transaction()?;
        let head = super::journal::head_in(&tx)?;
        let meta: Option<(u32, u64, Vec<u8>, Vec<u8>)> = tx.query_row("SELECT reducer_version, projected_through_seq, projected_through_hash, projection_digest FROM projection_meta WHERE projection_name = ?1", [name], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).optional()?;
        let (actual_version, seq, hash, stored_digest) = meta.ok_or_else(|| StoreError::ProjectionStale { name: name.into(), journal_seq: head.local_seq, projection_seq: 0 })?;
        if actual_version != reducer_version { return Err(StoreError::ReducerVersionMismatch { name: name.into(), expected: reducer_version, actual: actual_version }); }
        let projected_hash: [u8; 32] = fixed(hash, "projection hash")?;
        if seq != head.local_seq || projected_hash != head.event_hash { return Err(StoreError::ProjectionStale { name: name.into(), journal_seq: head.local_seq, projection_seq: seq }); }
        let mut statement = tx.prepare("SELECT record_key, record_value, last_event_seq FROM projection_records WHERE projection_name = ?1 ORDER BY record_key")?;
        let records = statement.query_map([name], |row| Ok(ProjectionRecord { key: row.get(0)?, value: row.get(1)?, last_event_seq: row.get(2)? }))?.collect::<Result<Vec<_>, _>>()?;
        let actual_digest = digest(&tx, name)?;
        let stored_digest: [u8; 32] = fixed(stored_digest, "projection digest")?;
        if stored_digest != actual_digest { return Err(StoreError::InvalidData(format!("projection {name} digest mismatch"))); }
        drop(statement); tx.commit()?;
        Ok(ProjectionSnapshot { projection_name: name.into(), reducer_version, head, digest: actual_digest, records })
    }
}

fn digest(connection: &Connection, name: &str) -> Result<[u8; 32], StoreError> {
    let mut statement = connection.prepare("SELECT record_key, record_value, last_event_seq FROM projection_records WHERE projection_name = ?1 ORDER BY record_key")?;
    let mut rows = statement.query([name])?;
    let mut hasher = Sha256::new();
    while let Some(row) = rows.next()? {
        let key: Vec<u8> = row.get(0)?; let value: Vec<u8> = row.get(1)?; let seq: u64 = row.get(2)?;
        hasher.update((key.len() as u64).to_be_bytes()); hasher.update(key);
        hasher.update((value.len() as u64).to_be_bytes()); hasher.update(value); hasher.update(seq.to_be_bytes());
    }
    Ok(hasher.finalize().into())
}
fn fixed<const N: usize>(bytes: Vec<u8>, field: &str) -> Result<[u8; N], StoreError> { bytes.try_into().map_err(|value: Vec<u8>| StoreError::InvalidData(format!("{field} has length {}, expected {N}", value.len()))) }

//! Control-plane history stored as one small delta record per operation (format v2).
//!
//! Format v1 kept every whole-table snapshot inside one JSON record, so the record grew as
//! `operations × states` and every navigation parsed all of it. Version 2 keeps:
//!
//! - `runtime-control-index-v2` (key `0x00`): `{cursor, len}`;
//! - `runtime-control-delta-v2` (key = big-endian cursor): the table changes from cursor-1 to
//!   cursor plus the post-operation Git control snapshot (delta 0 is relative to empty tables);
//! - `runtime-control-latest-v2` (key `0x00`): the complete projection tables at `cursor`, so
//!   the next delta is computed without replaying history;
//! - `runtime-control-activation-v2` (key = worktree id ‖ state id): the newest cursor whose
//!   snapshot had that state active in that worktree, for O(1) exact-snapshot lookup.
//!
//! Undo/redo materialize the target tables by applying deltas in memory from `latest`, so the
//! cost is proportional to the distance moved, never to the length of history. A v1 record is
//! migrated in place the first time the history is written.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::domain::StateId;

use super::{
    RUNTIME_CONTROL_PROJECTION, RuntimeAttemptProjection, RuntimeControlHistory,
    RuntimeControlRestore, RuntimeControlSnapshot, RuntimeGitSnapshot, RuntimeProvenanceProjection,
    RuntimeStateProjection, RuntimeWorktreeProjection, StoreError, capture_control_snapshot,
    projection, restore_control_snapshot, runtime_state_from_projection,
};

const INDEX: &str = "runtime-control-index-v2";
const DELTA: &str = "runtime-control-delta-v2";
const LATEST: &str = "runtime-control-latest-v2";
const ACTIVATION: &str = "runtime-control-activation-v2";
/// Content-addressed JSON blobs shared by delta records (ref lists, navigation histories).
const BLOB: &str = "runtime-control-blob-v2";
const VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, serde::Serialize)]
pub(super) struct ControlIndex {
    cursor: usize,
    len: usize,
    /// Set by migration: v1 never recorded navigations, so the Git snapshot stored at
    /// `cursor` may not describe the live checkout. The next recorded operation replaces it
    /// with its own pre-operation snapshot.
    #[serde(default)]
    refresh_git: bool,
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
struct ControlDelta {
    states_added: Vec<RuntimeStateProjection>,
    states_removed: Vec<RuntimeStateProjection>,
    /// `(before, after)` pairs for rows whose content changed.
    states_changed: Vec<(RuntimeStateProjection, RuntimeStateProjection)>,
    attempts_added: Vec<RuntimeAttemptProjection>,
    attempts_removed: Vec<RuntimeAttemptProjection>,
    attempts_changed: Vec<(RuntimeAttemptProjection, RuntimeAttemptProjection)>,
    provenance_added: Vec<RuntimeProvenanceProjection>,
    provenance_removed: Vec<RuntimeProvenanceProjection>,
    worktrees_before: Vec<RuntimeWorktreeProjection>,
    worktrees_after: Vec<RuntimeWorktreeProjection>,
    /// Blob ids of the navigation records before/after (see [`BLOB`]).
    navigation_before: Vec<u8>,
    navigation_after: Vec<u8>,
    /// Git control snapshot after the operation that produced this cursor, with `refs`
    /// emptied and stored under `refs_blob` so unchanged ref lists are shared.
    git: Option<RuntimeGitSnapshot>,
    refs_blob: Option<Vec<u8>>,
}

fn put_blob<T: serde::Serialize>(
    tx: &rusqlite::Transaction<'_>,
    value: &T,
    seq: u64,
) -> Result<Vec<u8>, StoreError> {
    use sha2::Digest as _;
    let bytes = serde_json::to_vec(value).map_err(invalid)?;
    let id = sha2::Sha256::digest(&bytes).to_vec();
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM projection_records WHERE projection_name = ?1 AND record_key = ?2)",
        params![BLOB, id],
        |row| row.get(0),
    )?;
    if !exists {
        projection::put_record(tx, BLOB, VERSION, &id, &bytes, seq)?;
    }
    Ok(id)
}

fn get_blob<T: serde::de::DeserializeOwned>(
    connection: &Connection,
    id: &[u8],
) -> Result<T, StoreError> {
    read(connection, BLOB, id)?
        .ok_or_else(|| StoreError::InvalidData("control history blob is missing".into()))
}

/// Delta with its blobs resolved (navigation lists and ref list inline).
struct LoadedDelta {
    delta: ControlDelta,
    navigation_before: Vec<(Vec<u8>, Vec<u8>)>,
    navigation_after: Vec<(Vec<u8>, Vec<u8>)>,
    git: Option<RuntimeGitSnapshot>,
}

fn load_delta(connection: &Connection, cursor: usize) -> Result<LoadedDelta, StoreError> {
    let delta = delta(connection, cursor)?;
    let navigation_before = get_blob(connection, &delta.navigation_before)?;
    let navigation_after = get_blob(connection, &delta.navigation_after)?;
    let git = match (&delta.git, &delta.refs_blob) {
        (Some(git), Some(refs)) => {
            let mut git = git.clone();
            git.refs = get_blob(connection, refs)?;
            Some(git)
        }
        (Some(git), None) => Some(git.clone()),
        (None, _) => None,
    };
    Ok(LoadedDelta {
        delta,
        navigation_before,
        navigation_after,
        git,
    })
}

fn store_delta(
    tx: &rusqlite::Transaction<'_>,
    cursor: usize,
    before: &RuntimeControlSnapshot,
    after: &RuntimeControlSnapshot,
    git: Option<RuntimeGitSnapshot>,
    seq: u64,
) -> Result<(), StoreError> {
    let mut delta = diff(before, after);
    delta.navigation_before = put_blob(tx, &before.navigation, seq)?;
    delta.navigation_after = put_blob(tx, &after.navigation, seq)?;
    if let Some(mut git) = git {
        let refs = std::mem::take(&mut git.refs);
        delta.refs_blob = Some(put_blob(tx, &refs, seq)?);
        delta.git = Some(git);
    }
    write(tx, DELTA, &cursor_key(cursor), &delta, seq)
}

fn cursor_key(cursor: usize) -> [u8; 8] {
    (cursor as u64).to_be_bytes()
}

fn invalid(error: impl std::fmt::Display) -> StoreError {
    StoreError::InvalidData(error.to_string())
}

fn read<T: serde::de::DeserializeOwned>(
    connection: &Connection,
    name: &str,
    key: &[u8],
) -> Result<Option<T>, StoreError> {
    let bytes: Option<Vec<u8>> = connection
        .query_row(
            "SELECT record_value FROM projection_records WHERE projection_name = ?1 AND record_key = ?2",
            params![name, key],
            |row| row.get(0),
        )
        .optional()?;
    bytes
        .map(|bytes| serde_json::from_slice(&bytes).map_err(invalid))
        .transpose()
}

fn write<T: serde::Serialize>(
    tx: &rusqlite::Transaction<'_>,
    name: &str,
    key: &[u8],
    value: &T,
    seq: u64,
) -> Result<(), StoreError> {
    let bytes = serde_json::to_vec(value).map_err(invalid)?;
    // `put_record` registers the projection and its reducer version but only upserts when
    // the event sequence advanced; control records may be rewritten several times within one
    // event (initialization, then the operation's own delta), so force the final value.
    projection::put_record(tx, name, VERSION, key, &bytes, seq)?;
    tx.execute(
        "UPDATE projection_records SET record_value = ?3, last_event_seq = ?4 WHERE projection_name = ?1 AND record_key = ?2",
        params![name, key, bytes, seq],
    )?;
    Ok(())
}

fn index(connection: &Connection) -> Result<Option<ControlIndex>, StoreError> {
    read(connection, INDEX, &[0])
}

fn delta(connection: &Connection, cursor: usize) -> Result<ControlDelta, StoreError> {
    read(connection, DELTA, &cursor_key(cursor))?
        .ok_or_else(|| StoreError::InvalidData(format!("control delta {cursor} is missing")))
}

fn latest(connection: &Connection) -> Result<RuntimeControlSnapshot, StoreError> {
    read(connection, LATEST, &[0])?
        .ok_or_else(|| StoreError::InvalidData("control history has no latest snapshot".into()))
}

fn empty_snapshot() -> RuntimeControlSnapshot {
    RuntimeControlSnapshot {
        states: Vec::new(),
        attempts: Vec::new(),
        worktrees: Vec::new(),
        provenance: Vec::new(),
        navigation: Vec::new(),
        git: None,
    }
}

const STATE_REF_PREFIX: &[u8] = b"refs/jjk/states/";

/// Drops `refs/jjk/states/*` before a Git snapshot is stored: those refs are a function of
/// the states table at the same cursor (every non-imported state owns exactly one), so
/// storing them would make every record grow with the number of states.
fn strip_state_refs(git: Option<RuntimeGitSnapshot>) -> Option<RuntimeGitSnapshot> {
    git.map(|mut git| {
        git.refs
            .retain(|reference| !reference.name.starts_with(STATE_REF_PREFIX));
        git
    })
}

/// Rebuilds `refs/jjk/states/*` for a stored Git snapshot from the states table at the same
/// cursor. Snapshots migrated from v1 or written by 0.3.x may still carry them; those are
/// kept as they are.
fn complete_state_refs(
    mut git: RuntimeGitSnapshot,
    states: &[RuntimeStateProjection],
) -> Result<RuntimeGitSnapshot, StoreError> {
    if git
        .refs
        .iter()
        .any(|reference| reference.name.starts_with(STATE_REF_PREFIX))
    {
        return Ok(git);
    }
    for state in states {
        if state.kind == "imported" {
            continue;
        }
        let bytes: [u8; 16] = state.state_id.as_slice().try_into().map_err(|_| {
            StoreError::InvalidData("state id in control history is not 16 bytes".into())
        })?;
        let id = StateId::from_bytes(bytes).map_err(invalid)?;
        let mut name = STATE_REF_PREFIX.to_vec();
        name.extend_from_slice(id.to_string().as_bytes());
        git.refs.push(super::RuntimeGitRef {
            name,
            target: state.git_oid.as_bytes().to_vec(),
            symbolic: None,
        });
    }
    git.refs.sort_by(|left, right| left.name.cmp(&right.name));
    git.refs.dedup_by(|left, right| left.name == right.name);
    Ok(git)
}

fn provenance_key(row: &RuntimeProvenanceProjection) -> Vec<u8> {
    serde_json::to_vec(row).unwrap_or_default()
}

fn diff(before: &RuntimeControlSnapshot, after: &RuntimeControlSnapshot) -> ControlDelta {
    let mut delta = ControlDelta {
        worktrees_before: before.worktrees.clone(),
        worktrees_after: after.worktrees.clone(),
        ..ControlDelta::default()
    };
    let before_states: BTreeMap<_, _> = before.states.iter().map(|s| (&s.state_id, s)).collect();
    let after_states: BTreeMap<_, _> = after.states.iter().map(|s| (&s.state_id, s)).collect();
    for (id, row) in &after_states {
        match before_states.get(id) {
            None => delta.states_added.push((*row).clone()),
            Some(old) if *old != *row => {
                delta.states_changed.push(((*old).clone(), (*row).clone()))
            }
            Some(_) => {}
        }
    }
    for (id, row) in &before_states {
        if !after_states.contains_key(id) {
            delta.states_removed.push((*row).clone());
        }
    }
    let before_attempts: BTreeMap<_, _> =
        before.attempts.iter().map(|a| (&a.attempt_id, a)).collect();
    let after_attempts: BTreeMap<_, _> =
        after.attempts.iter().map(|a| (&a.attempt_id, a)).collect();
    for (id, row) in &after_attempts {
        match before_attempts.get(id) {
            None => delta.attempts_added.push((*row).clone()),
            Some(old) if *old != *row => {
                delta
                    .attempts_changed
                    .push(((*old).clone(), (*row).clone()));
            }
            Some(_) => {}
        }
    }
    for (id, row) in &before_attempts {
        if !after_attempts.contains_key(id) {
            delta.attempts_removed.push((*row).clone());
        }
    }
    let before_provenance: BTreeSet<_> = before.provenance.iter().map(provenance_key).collect();
    let after_provenance: BTreeSet<_> = after.provenance.iter().map(provenance_key).collect();
    for row in &after.provenance {
        if !before_provenance.contains(&provenance_key(row)) {
            delta.provenance_added.push(row.clone());
        }
    }
    for row in &before.provenance {
        if !after_provenance.contains(&provenance_key(row)) {
            delta.provenance_removed.push(row.clone());
        }
    }
    delta
}

/// Applies `loaded` to `snapshot` in memory, forward (cursor-1 → cursor) or inverse.
fn apply(snapshot: &mut RuntimeControlSnapshot, loaded: &LoadedDelta, forward: bool) {
    let delta = &loaded.delta;
    let (added, removed, changed_from_to): (
        &Vec<RuntimeStateProjection>,
        &Vec<RuntimeStateProjection>,
        Vec<(&RuntimeStateProjection, &RuntimeStateProjection)>,
    ) = if forward {
        (
            &delta.states_added,
            &delta.states_removed,
            delta.states_changed.iter().map(|(b, a)| (b, a)).collect(),
        )
    } else {
        (
            &delta.states_removed,
            &delta.states_added,
            delta.states_changed.iter().map(|(b, a)| (a, b)).collect(),
        )
    };
    let mut states: BTreeMap<Vec<u8>, RuntimeStateProjection> = snapshot
        .states
        .drain(..)
        .map(|s| (s.state_id.clone(), s))
        .collect();
    for row in removed {
        states.remove(&row.state_id);
    }
    for row in added {
        states.insert(row.state_id.clone(), row.clone());
    }
    for (_, to) in changed_from_to {
        states.insert(to.state_id.clone(), to.clone());
    }
    snapshot.states = states.into_values().collect();

    let (added, removed, changed_from_to): (
        &Vec<RuntimeAttemptProjection>,
        &Vec<RuntimeAttemptProjection>,
        Vec<(&RuntimeAttemptProjection, &RuntimeAttemptProjection)>,
    ) = if forward {
        (
            &delta.attempts_added,
            &delta.attempts_removed,
            delta.attempts_changed.iter().map(|(b, a)| (b, a)).collect(),
        )
    } else {
        (
            &delta.attempts_removed,
            &delta.attempts_added,
            delta.attempts_changed.iter().map(|(b, a)| (a, b)).collect(),
        )
    };
    let mut attempts: BTreeMap<Vec<u8>, RuntimeAttemptProjection> = snapshot
        .attempts
        .drain(..)
        .map(|a| (a.attempt_id.clone(), a))
        .collect();
    for row in removed {
        attempts.remove(&row.attempt_id);
    }
    for row in added {
        attempts.insert(row.attempt_id.clone(), row.clone());
    }
    for (_, to) in changed_from_to {
        attempts.insert(to.attempt_id.clone(), to.clone());
    }
    snapshot.attempts = attempts.into_values().collect();

    let (added, removed) = if forward {
        (&delta.provenance_added, &delta.provenance_removed)
    } else {
        (&delta.provenance_removed, &delta.provenance_added)
    };
    let removed_keys: BTreeSet<_> = removed.iter().map(provenance_key).collect();
    snapshot
        .provenance
        .retain(|row| !removed_keys.contains(&provenance_key(row)));
    snapshot.provenance.extend(added.iter().cloned());
    snapshot.provenance.sort_by_key(provenance_key);

    if forward {
        snapshot.worktrees.clone_from(&delta.worktrees_after);
        snapshot.navigation.clone_from(&loaded.navigation_after);
    } else {
        snapshot.worktrees.clone_from(&delta.worktrees_before);
        snapshot.navigation.clone_from(&loaded.navigation_before);
    }
    snapshot.git = None;
}

/// Complete projection tables as they were at `target`.
fn snapshot_at(
    connection: &Connection,
    index: ControlIndex,
    target: usize,
) -> Result<RuntimeControlSnapshot, StoreError> {
    if target >= index.len {
        return Err(StoreError::InvalidData(
            "control history cursor is invalid".into(),
        ));
    }
    let mut snapshot = latest(connection)?;
    if target > index.cursor {
        for cursor in index.cursor + 1..=target {
            apply(&mut snapshot, &load_delta(connection, cursor)?, true);
        }
    } else {
        for cursor in (target + 1..=index.cursor).rev() {
            apply(&mut snapshot, &load_delta(connection, cursor)?, false);
        }
    }
    Ok(snapshot)
}

fn record_activations(
    tx: &rusqlite::Transaction<'_>,
    snapshot: &RuntimeControlSnapshot,
    cursor: usize,
    seq: u64,
) -> Result<(), StoreError> {
    for worktree in &snapshot.worktrees {
        let Some(state) = &worktree.active_state_id else {
            continue;
        };
        let mut key = worktree.worktree_id.clone();
        key.extend_from_slice(state);
        write(tx, ACTIVATION, &key, &cursor, seq)?;
    }
    Ok(())
}

/// Converts a v1 inline history into v2 records inside `tx`, then deletes the v1 record.
fn migrate_legacy(tx: &rusqlite::Transaction<'_>, seq: u64) -> Result<bool, StoreError> {
    let Some(history) = read::<RuntimeControlHistory>(tx, RUNTIME_CONTROL_PROJECTION, &[0])? else {
        return Ok(false);
    };
    let mut previous = empty_snapshot();
    for (cursor, snapshot) in history.snapshots.iter().enumerate() {
        store_delta(
            tx,
            cursor,
            &previous,
            snapshot,
            strip_state_refs(snapshot.git.clone()),
            seq,
        )?;
        record_activations(tx, snapshot, cursor, seq)?;
        previous = snapshot.clone();
        previous.git = None;
    }
    let cursor = history
        .cursor
        .min(history.snapshots.len().saturating_sub(1));
    // `latest` must describe the tables as they are now: v1 never recorded navigations or
    // visibility changes, so its snapshot at `cursor` can be stale relative to the live rows.
    // Migration runs at the start of a write transaction, before the operation's own
    // projections apply, so the live tables are exactly the pre-operation state.
    let live = capture_control_snapshot(tx)?;
    write(tx, LATEST, &[0], &live, seq)?;
    write(
        tx,
        INDEX,
        &[0],
        &ControlIndex {
            cursor,
            len: history.snapshots.len().max(1),
            refresh_git: true,
        },
        seq,
    )?;
    if history.snapshots.is_empty() {
        store_delta(tx, 0, &previous, &previous, None, seq)?;
    }
    tx.execute(
        "DELETE FROM projection_records WHERE projection_name = ?1",
        [RUNTIME_CONTROL_PROJECTION],
    )?;
    Ok(true)
}

/// Migrates a v1 history if one exists and no v2 index does. Call at the start of every
/// write transaction, before the operation's projections are applied.
pub(super) fn migrate_if_needed(
    tx: &rusqlite::Transaction<'_>,
    seq: u64,
) -> Result<(), StoreError> {
    if index(tx)?.is_some() {
        return Ok(());
    }
    migrate_legacy(tx, seq).map(|_| ())
}

/// Guarantees a v2 history exists: migrates v1 or seeds cursor 0 from the live tables.
pub(super) fn ensure_initial(
    tx: &rusqlite::Transaction<'_>,
    seq: u64,
) -> Result<ControlIndex, StoreError> {
    if let Some(index) = index(tx)? {
        return Ok(index);
    }
    if migrate_legacy(tx, seq)? {
        return index(tx)?
            .ok_or_else(|| StoreError::InvalidData("control migration left no index".into()));
    }
    let current = capture_control_snapshot(tx)?;
    store_delta(tx, 0, &empty_snapshot(), &current, None, seq)?;
    record_activations(tx, &current, 0, seq)?;
    write(tx, LATEST, &[0], &current, seq)?;
    let index = ControlIndex {
        cursor: 0,
        len: 1,
        refresh_git: false,
    };
    write(tx, INDEX, &[0], &index, seq)?;
    Ok(index)
}

/// Records the operation that just changed the projection tables: one delta at `cursor + 1`,
/// discarding any redo tail.
pub(super) fn record(
    tx: &rusqlite::Transaction<'_>,
    seq: u64,
    before: &RuntimeGitSnapshot,
    after: &RuntimeGitSnapshot,
) -> Result<(), StoreError> {
    let index = ensure_initial(tx, seq)?;
    if index.len == 1 || index.refresh_git {
        // The very first operation binds the pre-operation Git snapshot to cursor 0, so
        // undoing everything restores the exact original control state; after a v1
        // migration the same pre-operation snapshot replaces the possibly stale one.
        let mut current = delta(tx, index.cursor)?;
        if current.git.is_none() || index.refresh_git {
            let mut git = strip_state_refs(Some(before.clone()));
            if let Some(git) = git.as_mut() {
                let refs = std::mem::take(&mut git.refs);
                current.refs_blob = Some(put_blob(tx, &refs, seq)?);
            }
            current.git = git;
            write(tx, DELTA, &cursor_key(index.cursor), &current, seq)?;
        }
    }
    for stale in index.cursor + 1..index.len {
        tx.execute(
            "DELETE FROM projection_records WHERE projection_name = ?1 AND record_key = ?2",
            params![DELTA, cursor_key(stale)],
        )?;
    }
    let previous = latest(tx)?;
    let current = capture_control_snapshot(tx)?;
    let cursor = index.cursor + 1;
    store_delta(
        tx,
        cursor,
        &previous,
        &current,
        strip_state_refs(Some(after.clone())),
        seq,
    )?;
    record_activations(tx, &current, cursor, seq)?;
    write(tx, LATEST, &[0], &current, seq)?;
    write(
        tx,
        INDEX,
        &[0],
        &ControlIndex {
            cursor,
            len: cursor + 1,
            refresh_git: false,
        },
        seq,
    )
}

/// Plans an undo (`direction < 0`) or redo without touching the tables.
pub(super) fn plan(
    connection: &Connection,
    direction: i8,
    workspace_id: Uuid,
) -> Result<RuntimeControlRestore, StoreError> {
    let index = index(connection)?
        .ok_or_else(|| StoreError::InvalidData("no control history exists".into()))?;
    let to = if direction < 0 {
        index.cursor.checked_sub(1)
    } else {
        index.cursor.checked_add(1).filter(|next| *next < index.len)
    }
    .ok_or_else(|| {
        StoreError::InvalidData(
            if direction < 0 {
                "no earlier control snapshot to undo to"
            } else {
                "no later control snapshot to redo to"
            }
            .into(),
        )
    })?;
    let target = snapshot_at(connection, index, to)?;
    let git = load_delta(connection, to)?.git.ok_or_else(|| {
        StoreError::InvalidData("control snapshot predates exact Git artifact capture".into())
    })?;
    let git = complete_state_refs(git, &target.states)?;
    let active = target
        .worktrees
        .iter()
        .find(|row| row.worktree_id == workspace_id.as_bytes())
        .and_then(|row| row.active_state_id.as_ref());
    let current = active
        .and_then(|id| target.states.iter().find(|state| &state.state_id == id))
        .map(runtime_state_from_projection)
        .transpose()?;
    Ok(RuntimeControlRestore {
        current,
        git,
        from_cursor: index.cursor,
        to_cursor: to,
    })
}

/// Moves the projection tables to `to_cursor`.
pub(super) fn restore(
    tx: &rusqlite::Transaction<'_>,
    seq: u64,
    to_cursor: usize,
) -> Result<(), StoreError> {
    let index = ensure_initial(tx, seq)?;
    let target = snapshot_at(tx, index, to_cursor)?;
    restore_control_snapshot(tx, &target)?;
    write(tx, LATEST, &[0], &target, seq)?;
    write(
        tx,
        INDEX,
        &[0],
        &ControlIndex {
            cursor: to_cursor,
            len: index.len,
            refresh_git: index.refresh_git,
        },
        seq,
    )
}

/// Newest exact Git snapshot taken while `state_id` was active in `workspace_id`.
pub(super) fn git_snapshot_for_state(
    connection: &Connection,
    workspace_id: Uuid,
    state_id: Uuid,
) -> Result<Option<RuntimeGitSnapshot>, StoreError> {
    let Some(index) = index(connection)? else {
        // Unmigrated v1 history: read it the old way (migration happens on the next write).
        let Some(history) =
            read::<RuntimeControlHistory>(connection, RUNTIME_CONTROL_PROJECTION, &[0])?
        else {
            return Ok(None);
        };
        return Ok(history.snapshots.iter().rev().find_map(|snapshot| {
            snapshot
                .worktrees
                .iter()
                .any(|row| {
                    row.worktree_id == workspace_id.as_bytes()
                        && row.active_state_id.as_deref() == Some(state_id.as_bytes())
                })
                .then(|| snapshot.git.clone())
                .flatten()
        }));
    };
    let mut key = workspace_id.as_bytes().to_vec();
    key.extend_from_slice(state_id.as_bytes());
    if let Some(cursor) = read::<usize>(connection, ACTIVATION, &key)? {
        if cursor < index.len {
            if let Some(git) = load_delta(connection, cursor)?.git {
                let states = snapshot_at(connection, index, cursor)?.states;
                return Ok(Some(complete_state_refs(git, &states)?));
            }
        }
    }
    // The activation index can point past a truncated redo tail; scan what still exists.
    for cursor in (0..index.len).rev() {
        let loaded = load_delta(connection, cursor)?;
        let active = loaded.delta.worktrees_after.iter().any(|row| {
            row.worktree_id == workspace_id.as_bytes()
                && row.active_state_id.as_deref() == Some(state_id.as_bytes())
        });
        if active {
            return loaded
                .git
                .map(|git| {
                    let states = snapshot_at(connection, index, cursor)?.states;
                    complete_state_refs(git, &states)
                })
                .transpose();
        }
    }
    Ok(None)
}

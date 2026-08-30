PRAGMA application_id = 1246382897;

CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY CHECK (version > 0),
    name TEXT NOT NULL UNIQUE,
    checksum BLOB NOT NULL CHECK (length(checksum) = 32),
    applied_at_utc TEXT NOT NULL
) STRICT;

CREATE TABLE journal_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    repo_id BLOB NOT NULL CHECK (length(repo_id) = 16),
    repository_root_token BLOB NOT NULL,
    envelope_version INTEGER NOT NULL CHECK (envelope_version > 0),
    storage_schema_version INTEGER NOT NULL CHECK (storage_schema_version > 0),
    journal_generation INTEGER NOT NULL DEFAULT 1 CHECK (journal_generation > 0),
    created_at_utc TEXT NOT NULL
) STRICT;

CREATE TABLE events (
    local_seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id BLOB NOT NULL UNIQUE CHECK (length(event_id) = 16),
    repo_id BLOB NOT NULL CHECK (length(repo_id) = 16),
    event_type TEXT NOT NULL,
    event_schema_version INTEGER NOT NULL CHECK (event_schema_version > 0),
    envelope_version INTEGER NOT NULL CHECK (envelope_version > 0),
    operation_id BLOB NOT NULL CHECK (length(operation_id) = 16),
    operation_ordinal INTEGER NOT NULL CHECK (operation_ordinal >= 0),
    actor_id BLOB NOT NULL CHECK (length(actor_id) = 16),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('human', 'agent', 'system', 'import')),
    recorded_at_utc TEXT NOT NULL,
    observed_at_utc TEXT,
    repository_fingerprint BLOB NOT NULL,
    payload_codec TEXT NOT NULL CHECK (payload_codec IN ('cbor-canonical-v1', 'json-canonical-v1')),
    payload BLOB NOT NULL,
    provenance BLOB NOT NULL,
    evidence_manifest BLOB NOT NULL,
    dedup_key TEXT UNIQUE,
    previous_event_hash BLOB NOT NULL CHECK (length(previous_event_hash) = 32),
    event_hash BLOB NOT NULL UNIQUE CHECK (length(event_hash) = 32),
    UNIQUE (operation_id, operation_ordinal)
) STRICT;

CREATE TRIGGER events_no_update BEFORE UPDATE ON events BEGIN
    SELECT RAISE(ABORT, 'JJK journal events are immutable');
END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events BEGIN
    SELECT RAISE(ABORT, 'JJK journal events are immutable');
END;

CREATE TABLE event_causes (
    event_id BLOB NOT NULL REFERENCES events(event_id),
    cause_event_id BLOB NOT NULL REFERENCES events(event_id),
    relation TEXT NOT NULL CHECK (relation IN ('caused-by', 'command-after', 'composes', 'validates', 'promotes', 'restores')),
    PRIMARY KEY (event_id, cause_event_id, relation)
) STRICT;
CREATE TRIGGER event_causes_no_update BEFORE UPDATE ON event_causes BEGIN
    SELECT RAISE(ABORT, 'JJK journal causes are immutable');
END;
CREATE TRIGGER event_causes_no_delete BEFORE DELETE ON event_causes BEGIN
    SELECT RAISE(ABORT, 'JJK journal causes are immutable');
END;

CREATE TABLE artifacts (
    artifact_hash BLOB PRIMARY KEY CHECK (length(artifact_hash) = 32),
    media_type TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    storage_kind TEXT NOT NULL CHECK (storage_kind IN ('inline', 'file', 'git-object', 'external')),
    inline_bytes BLOB,
    relative_path TEXT,
    created_seq INTEGER NOT NULL REFERENCES events(local_seq),
    CHECK ((storage_kind = 'inline') = (inline_bytes IS NOT NULL)),
    CHECK (relative_path IS NULL OR instr(relative_path, '/') != 1)
) STRICT;
CREATE TRIGGER artifacts_no_update BEFORE UPDATE ON artifacts BEGIN
    SELECT RAISE(ABORT, 'JJK journal artifacts are immutable');
END;
CREATE TRIGGER artifacts_no_delete BEFORE DELETE ON artifacts BEGIN
    SELECT RAISE(ABORT, 'JJK journal artifacts are immutable');
END;

CREATE TABLE operations (
    operation_id BLOB PRIMARY KEY CHECK (length(operation_id) = 16),
    request_hash BLOB NOT NULL CHECK (length(request_hash) = 32),
    command_kind TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('prepared', 'applying', 'awaiting_resolution', 'verifying', 'committed', 'aborting', 'aborted', 'repair_required')),
    prepared_seq INTEGER NOT NULL REFERENCES events(local_seq),
    terminal_seq INTEGER REFERENCES events(local_seq),
    precondition_fingerprint BLOB NOT NULL,
    expected_effects BLOB NOT NULL,
    recovery_artifact_hash BLOB,
    result BLOB,
    last_event_seq INTEGER NOT NULL REFERENCES events(local_seq),
    CHECK (recovery_artifact_hash IS NULL OR length(recovery_artifact_hash) = 32),
    CHECK ((status IN ('committed', 'aborted')) = (terminal_seq IS NOT NULL))
) STRICT;

CREATE TABLE operation_effects (
    operation_id BLOB NOT NULL REFERENCES operations(operation_id),
    effect_ordinal INTEGER NOT NULL CHECK (effect_ordinal >= 0),
    effect_kind TEXT NOT NULL,
    expected BLOB NOT NULL,
    observed_receipt BLOB,
    status TEXT NOT NULL CHECK (status IN ('planned', 'observed', 'diverged', 'reversed')),
    last_event_seq INTEGER NOT NULL REFERENCES events(local_seq),
    PRIMARY KEY (operation_id, effect_ordinal)
) STRICT;

CREATE TABLE projection_meta (
    projection_name TEXT PRIMARY KEY,
    reducer_version INTEGER NOT NULL CHECK (reducer_version > 0),
    projected_through_seq INTEGER NOT NULL CHECK (projected_through_seq >= 0),
    projected_through_hash BLOB NOT NULL CHECK (length(projected_through_hash) = 32),
    projection_digest BLOB NOT NULL CHECK (length(projection_digest) = 32)
) STRICT;

CREATE TABLE projection_records (
    projection_name TEXT NOT NULL REFERENCES projection_meta(projection_name),
    record_key BLOB NOT NULL,
    record_value BLOB NOT NULL,
    last_event_seq INTEGER NOT NULL CHECK (last_event_seq > 0),
    PRIMARY KEY (projection_name, record_key)
) STRICT;

CREATE TABLE states (
    state_id BLOB PRIMARY KEY CHECK (length(state_id) = 16),
    created_seq INTEGER NOT NULL REFERENCES events(local_seq),
    kind TEXT NOT NULL,
    git_algorithm TEXT NOT NULL,
    git_oid TEXT NOT NULL,
    jj_change_id TEXT,
    jj_commit_id TEXT,
    attempt_id BLOB NOT NULL CHECK (length(attempt_id) = 16),
    label TEXT NOT NULL,
    message TEXT,
    archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    last_event_seq INTEGER NOT NULL REFERENCES events(local_seq)
) STRICT;

CREATE TABLE state_logical_parents (
    child_state_id BLOB PRIMARY KEY REFERENCES states(state_id),
    parent_state_id BLOB NOT NULL REFERENCES states(state_id),
    created_seq INTEGER NOT NULL REFERENCES events(local_seq),
    CHECK (child_state_id != parent_state_id)
) STRICT;

CREATE TABLE attempts (
    attempt_id BLOB PRIMARY KEY CHECK (length(attempt_id) = 16),
    root_state_id BLOB NOT NULL REFERENCES states(state_id),
    objective TEXT NOT NULL,
    current_tip_state_id BLOB REFERENCES states(state_id),
    archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    last_event_seq INTEGER NOT NULL REFERENCES events(local_seq)
) STRICT;

CREATE TABLE state_provenance_edges (
    source_state_id BLOB NOT NULL REFERENCES states(state_id),
    result_state_id BLOB NOT NULL REFERENCES states(state_id),
    relation TEXT NOT NULL CHECK (relation IN ('derived-from', 'composed-from', 'promotion-source')),
    provenance_id BLOB NOT NULL CHECK (length(provenance_id) = 16),
    created_seq INTEGER NOT NULL REFERENCES events(local_seq),
    PRIMARY KEY (source_state_id, result_state_id, relation, provenance_id)
) STRICT;

CREATE TABLE branch_bindings (
    branch_id BLOB PRIMARY KEY CHECK (length(branch_id) = 16),
    attempt_id BLOB REFERENCES attempts(attempt_id),
    refname BLOB NOT NULL,
    observed_git_oid TEXT,
    target_state_id BLOB REFERENCES states(state_id),
    canonical_role TEXT,
    last_event_seq INTEGER NOT NULL REFERENCES events(local_seq)
) STRICT;

CREATE TABLE worktree_current (
    worktree_id BLOB PRIMARY KEY CHECK (length(worktree_id) = 16),
    attempt_id BLOB REFERENCES attempts(attempt_id),
    active_state_id BLOB REFERENCES states(state_id),
    relative_locator BLOB NOT NULL,
    head_oid TEXT,
    index_tree_oid TEXT,
    dirty_digest BLOB,
    last_event_seq INTEGER NOT NULL REFERENCES events(local_seq)
) STRICT;

CREATE INDEX events_operation_seq ON events(operation_id, local_seq);
CREATE INDEX events_type_seq ON events(event_type, local_seq);
CREATE INDEX events_actor_seq ON events(actor_id, local_seq);
CREATE INDEX operations_status_seq ON operations(status, prepared_seq);
CREATE INDEX states_attempt_seq ON states(attempt_id, created_seq);
CREATE INDEX projection_records_seq ON projection_records(projection_name, last_event_seq);

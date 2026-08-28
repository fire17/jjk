# JJK Event Model

**Status:** decision-grade architecture for the v0.1 rewrite  
**Scope:** JJK semantic truth, its relationship to Git/JJ truth, replayable projections, and recovery  
**Normative language:** MUST, MUST NOT, SHOULD, and MAY are requirements levels.

## Context

JJK is a semantic state layer above Git and, optionally, Jujutsu. Git remains the universal object, ref, and collaboration substrate. JJ may add local change identity and operation-log recovery. JJK owns meaning: states, attempts, annotations, composition provenance, validation evidence, canonical promotion, archives, backups, and Timeshift records.

The previous implementation kept mutable aggregate JSON (`repo.json`) beside whole-control-plane history snapshots. That made the latest aggregate convenient to read, but made facts, current views, and recovery copies look equally authoritative. The rewrite needs one durable semantic history that can be replayed, audited, migrated, and repaired after a crash between SQLite and Git/filesystem mutations.

The required cross-layer mutation protocol is:

`discover → lock → reconcile → resolve → plan → durable prepare → mutate Git/JJ/files → append events+projections → verify → commit/repair`

SQLite with WAL is the default local journal because it supplies an embedded Rust-compatible implementation, checksums/integrity checks, transactions across the journal and projections, indexed queries, online backup APIs, and crash recovery without a service. It is not treated as universally safe:

- WAL requires reliable local shared-memory and file-lock semantics; JJK MUST detect unsupported/network filesystems and use SQLite rollback-journal mode with an exclusive writer, or refuse mutation when durability cannot be proved.
- Copying only the main `.sqlite` file while WAL is active is not a backup. JJK MUST use the SQLite backup API or `VACUUM INTO`, then verify the result.
- SQLite cannot atomically commit with Git refs, JJ operations, or arbitrary files. Durable operation preparation plus deterministic reconciliation closes that seam; pretending it is a distributed transaction would not.

## Decisions

### EM-D001 — Divide authority by domain, never by convenience

| Domain fact | Authority | JJK journal role |
|---|---|---|
| Git object bytes, object ancestry, current Git refs/index/worktree | Git object database and repository at the instant inspected | Records immutable observations, requested effects, and verified outcomes |
| JJ change/commit IDs and operation-log facts | JJ store when the adapter is enabled | Records immutable observations and links to JJK identities |
| State meaning, attempt topology, annotations, provenance, validation, promotion, archive, backup, Timeshift | The logical JJK event journal | Sole authority |
| Current graph, tips, search indexes, current annotations, operation status | Materialized projections | Disposable derived caches |
| Exported backup/freeze contents | The verified export artifact | A portable copy; it becomes live truth only through an explicit restore/import event |

“SQLite is authoritative” means **the ordered JJK semantic journal**, not every SQLite table. Projection rows MUST NOT be edited as commands and MUST NOT be interpreted when their replay watermark is stale. Git truth is not copied into JJK and then superseded; it is observed with a fingerprint. A later observation may supersede an earlier observation without altering it.

### EM-D002 — One immutable typed envelope

Every accepted event is an immutable `EventEnvelope`. Payloads are closed, typed records selected by `(event_type, event_schema_version)`. IDs are typed newtypes; raw strings MUST NOT cross the domain boundary.

```rust
struct EventEnvelope<P> {
    event_id: EventId,                 // UUIDv7, globally unique
    repo_id: RepoId,                   // UUIDv7, never path-derived
    local_seq: u64,                    // SQLite-assigned total commit order in this journal
    event_type: EventType,             // stable PascalCase registry key
    event_schema_version: u16,         // payload version, begins at 1
    envelope_version: u16,             // envelope version, begins at 1
    operation_id: OperationId,         // one user/agent/system mutation
    operation_ordinal: u32,            // unique, contiguous within the operation
    actor: ActorRef,
    recorded_at_utc: Timestamp,        // RFC 3339 UTC; informational, not ordering
    observed_at_utc: Option<Timestamp>,// source occurrence time, when known
    repository_fingerprint: RepositoryFingerprint,
    payload_codec: PayloadCodec,       // "cbor-canonical-v1"
    payload: P,
    provenance: Provenance,
    evidence: Vec<EvidenceRef>,
    dedup_key: Option<DedupKey>,
    previous_event_hash: Hash256,
    event_hash: Hash256,
}
```

`event_hash = SHA-256("jjk-event-v1" || canonical_envelope_fields_except_hash || canonical_payload || previous_event_hash)`. Canonical payload bytes use deterministic CBOR. CLI/API diagnostic JSON is a rendering of those bytes, never the hashed source representation.

Causal parents are normalized because an event may combine several histories:

```sql
CREATE TABLE event_causes (
  event_id       BLOB NOT NULL REFERENCES events(event_id),
  cause_event_id BLOB NOT NULL REFERENCES events(event_id),
  relation       TEXT NOT NULL CHECK (relation IN
                 ('caused-by','command-after','composes','validates','promotes','restores')),
  PRIMARY KEY (event_id, cause_event_id, relation)
) STRICT;
```

A cause in the same journal MUST have a lower `local_seq`. Remote/source provenance uses `ExternalEventRef` in the payload rather than a dangling local foreign key.

### EM-D003 — Concrete journal storage

```sql
PRAGMA application_id = 0x4A4A4B31;          -- "JJK1"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;                   -- local-locking-capable filesystems only
PRAGMA synchronous = FULL;
PRAGMA wal_autocheckpoint = 1000;

CREATE TABLE journal_meta (
  singleton              INTEGER PRIMARY KEY CHECK (singleton = 1),
  repo_id                 BLOB NOT NULL CHECK (length(repo_id) = 16),
  repository_root_token   BLOB NOT NULL,     -- stable identity, not an absolute path
  envelope_version        INTEGER NOT NULL,
  storage_schema_version  INTEGER NOT NULL,
  journal_generation      INTEGER NOT NULL DEFAULT 1,
  created_at_utc          TEXT NOT NULL
) STRICT;

CREATE TABLE events (
  local_seq                 INTEGER PRIMARY KEY AUTOINCREMENT,
  event_id                  BLOB NOT NULL UNIQUE CHECK (length(event_id) = 16),
  repo_id                   BLOB NOT NULL CHECK (length(repo_id) = 16),
  event_type                TEXT NOT NULL,
  event_schema_version      INTEGER NOT NULL CHECK (event_schema_version > 0),
  envelope_version          INTEGER NOT NULL CHECK (envelope_version > 0),
  operation_id              BLOB NOT NULL CHECK (length(operation_id) = 16),
  operation_ordinal         INTEGER NOT NULL CHECK (operation_ordinal >= 0),
  actor_id                  BLOB NOT NULL CHECK (length(actor_id) = 16),
  actor_kind                TEXT NOT NULL CHECK (actor_kind IN ('human','agent','system','import')),
  recorded_at_utc           TEXT NOT NULL,
  observed_at_utc           TEXT,
  repository_fingerprint    BLOB NOT NULL,
  payload_codec             TEXT NOT NULL CHECK (payload_codec = 'cbor-canonical-v1'),
  payload                   BLOB NOT NULL,
  provenance                BLOB NOT NULL,
  evidence_manifest         BLOB NOT NULL,
  dedup_key                 TEXT UNIQUE,
  previous_event_hash       BLOB NOT NULL CHECK (length(previous_event_hash) = 32),
  event_hash                BLOB NOT NULL UNIQUE CHECK (length(event_hash) = 32),
  UNIQUE (operation_id, operation_ordinal)
) STRICT;

CREATE TRIGGER events_no_update BEFORE UPDATE ON events
BEGIN SELECT RAISE(ABORT, 'JJK journal events are immutable'); END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events
BEGIN SELECT RAISE(ABORT, 'JJK journal events are immutable'); END;

CREATE TABLE artifacts (
  artifact_hash  BLOB PRIMARY KEY CHECK (length(artifact_hash) = 32),
  media_type     TEXT NOT NULL,
  byte_length    INTEGER NOT NULL CHECK (byte_length >= 0),
  storage_kind   TEXT NOT NULL CHECK (storage_kind IN ('inline','file','git-object','external')),
  inline_bytes   BLOB,
  relative_path  TEXT,
  created_seq    INTEGER NOT NULL REFERENCES events(local_seq),
  CHECK ((storage_kind = 'inline') = (inline_bytes IS NOT NULL))
) STRICT;
```

Large patches, logs, manifests, recovery bundles, and Timeshift components are content-addressed artifacts. Event payloads contain `ArtifactRef { sha256, byte_length, media_type }`; absolute machine paths and secrets MUST NOT enter the journal.

### EM-D004 — Typed identities are deliberately non-interchangeable

All domain identities are UUIDv7 newtypes stored as 16-byte BLOBs. External text uses a type prefix plus canonical Crockford Base32; parsers reject a valid UUID presented under the wrong prefix.

| Type | External prefix | Creation and semantics |
|---|---|---|
| `RepoId` | `repo_` | Created once at safe-space initialization; survives relocation |
| `EventId` | `evt_` | Created before append; names one immutable fact |
| `OperationId` | `op_` | Created by JJK or supplied by the caller; names one idempotent command execution across retries |
| `ActorId` | `actor_` | Stable local identity record; actor kind is separate |
| `StateId` | `st_` | Created by `StateCaptured`; remains stable through annotation/archive/recovery |
| `AttemptId` | `at_` | A semantic line of exploration; not a Git branch name |
| `BranchId` | `br_` | JJK identity for a branch binding; refname may change |
| `WorktreeId` | `ws_` | JJK identity for a worktree binding; path may change |
| `CompositionId` | `cmp_` | One semantic composition hyperedge |
| `CandidateId` | `cand_` | One external or composition candidate |
| `PromotionId` | `prm_` | One canonical promotion or rollback lineage |
| `NavigationId` | `nav_` | One navigation visit/activation identity |
| `ProvenanceId` | `prov_` | One provenance record linking immutable sources |
| `ValidationId` | `ver_` | One immutable validation run |
| `ArchiveId` | `arc_` | One archive/recovery lifecycle |
| `DeltaId` | `dlt_` | One exact atomic delta identity |
| `BackupId`, `TimeshiftId` | `bak_`, `tsh_` | Stable recovery workflow identities |
| `GitObjectId` | algorithm-qualified hexadecimal | Immutable Git fact; never parsed as a JJK ID |
| `JjChangeId` / `JjCommitId` | validated adapter-owned text | Optional JJ facts |
| label, alias, branch refname, path | validated domain value | Mutable human/external names; never identity |

UUIDv7 timestamp bits aid indexes but MUST NOT define event order. `local_seq` defines order within one journal; causal links define dependency; logical state edges define product topology.

### EM-D005 — Event taxonomy

All payloads include only IDs and immutable facts needed to replay. Mutable “current” values are reducers’ outputs.

| Family | Event and payload essentials | Reducer effect |
|---|---|---|
| Repository | `SafeSpaceInitialized { repo_id, root_token, initial_capabilities }` | Creates repository projection |
| Operation | `OperationPrepared { command, request_hash, precondition_fingerprint, plan, recovery_artifact, expected_effects }` | Opens a repairable operation in `prepared` |
| Operation | `ApplyStarted { effect_ids }`, `EffectObserved { effect_id, receipt, post_effect_fingerprint }` | Advances through `applying` and records each stable `EffectId = OperationId + ordinal` |
| Operation | `ConflictPaused { conflict, resolution_options }` | Enters `awaiting_resolution`; no guessed resolution |
| Operation | `VerificationStarted { expected_fingerprint }`, `OperationCommitted { result, verified_fingerprint }` | Advances `verifying → committed` |
| Operation | `AbortStarted { reason }`, `OperationAborted { reason, restored_fingerprint }` | Advances `aborting → aborted` after verified restoration |
| Operation | `RepairRequired { detected_effects, mismatch }`, `RepairResumed { strategy }` | Enters/leaves durable `repair_required` |
| Reconciliation | `GitCommitObserved { oid, parent_oids, tree_oid, author, committer, source }` | Adds immutable observed Git fact |
| Reconciliation | `GitRefObserved { branch_id, refname, target_oid, observation }`, `GitRefRemoved { ... }` | Updates current ref observation |
| Reconciliation | `JjOperationObserved { jj_operation_id, change_links, head_ids }`, `WorkspaceObserved { worktree_id, head, index_tree, dirty_digest }` | Updates optional JJ/workspace observations |
| State | `StateCaptured { state_id, kind, git_oid, jj_ids?, logical_parent_state_id?, attempt_id, label, message?, stats, capture_fingerprint }` | Creates one semantic state and its sole logical-parent relation |
| State | `StateAnnotated { state_id, annotation_id, kind, value, replaces? }` | Adds/replaces label, tag, star, note, handoff, trust metadata |
| State | `StateActivated { navigation_id, state_id, worktree_id, prior_state_id?, mode }` | Updates navigation/worktree view |
| Attempts | `AttemptForked { attempt_id, from_state_id, branch_binding?, worktree_binding?, objective }` | Creates sibling future root |
| Attempts | `BranchBound { branch_id, attempt_id, refname, target_state_id }`, `WorktreeBound { worktree_id, attempt_id, relative_locator }` | Maintains semantic/external bindings |
| Composition | `DeltaApplied { composition_id, delta_id, source_state_id, source_parent_state_id, target_base_state_id, patch_id, resolution_artifact?, result_state_id }` | Adds composition input/result provenance; never adds another logical parent |
| Composition | `CompositionAttempted { composition_id, intent, source_state_ids, candidate_attempt_ids, strategy, instructions_artifact }` | Creates a hyperedge recording plural semantic synthesis |
| Composition | `CompositionResolved { composition_id, candidate_state_ids, selected_state_id?, decision_evidence }` | Records comparison without erasing alternatives |
| Validation | `ValidationRecorded { validation_id, subject, suite, outcome, evidence, environment_fingerprint, expires_at? }` | Adds immutable validation evidence |
| Canonical | `CanonicalPromoted { promotion_id, canonical_branch_id, source_state_id, previous_state_id?, policy, validation_ids, resulting_ref_oid }` | Advances canonical projection after policy proof |
| Canonical | `CanonicalRolledBack { promotion_id, rollback_of, restored_state_id, resulting_ref_oid, reason }` | Reverses by a new fact, never event deletion |
| Lifecycle | `StateArchived { state_id, archive_id, prior_location, hidden_ref?, reason }`, `StateRecovered { state_id, archive_id, restored_location }` | Toggles visibility and restores exact graph location |
| Backup | `BackupCreated { backup_id, through_seq, through_event_hash, manifest_artifact, sqlite_artifact, git_reachability_manifest, byte_length }` | Catalogs a verified consistent backup |
| Backup | `RestorePrepared { backup_id, pre_restore_backup_id, preview, mapping }`, `RestoreApplied { backup_id, source_generation, imported_through_hash, result_fingerprint }`, `RestoreRepaired { backup_id, repair }` | Runs restore through the standard durable operation protocol |
| Ecosystem | `ExternalCandidateDiscovered { candidate_id, forge, immutable_remote_identity, observed_tip, metadata }` | Adds candidate facts without trusting them |
| Timeshift | `TimeshiftCaptured { timeshift_id, state_id?, attempt_id?, component_manifest, excluded_classes }` | Catalogs componentized situation capture |
| Timeshift | `TimeshiftRestored { timeshift_id, requested_components, restored_components, skipped_components, result_fingerprint }` | Records honest partial/full adapter result |
| Maintenance | `MigrationStarted { from, to, pre_migration_backup_id }`, `MigrationCompleted { from, to, tool_version }`, `MigrationFailed { from, to, error_artifact }`, `JournalRepairDeclared { old_generation, new_generation, recovered_through_hash, declared_gap? }` | Audits storage evolution/forensic recovery |

Kinds such as `save`, `step`, `nice`, `git`, `new`, `stash`, `cherry`, and `auto` are values in `StateCaptured.kind`. `star` is an annotation, not a second snapshot. A Git commit may back several semantic states; therefore `(repo_id, git_oid)` is intentionally not unique in `states`.

## Payload examples

Diagnostic JSON below is lossless in meaning but is encoded as canonical CBOR on disk.

### State capture on a sibling attempt

```json
{
  "event_type": "StateCaptured",
  "event_schema_version": 1,
  "event_id": "01958f84-bbd0-7c61-8e98-45ca38e29452",
  "operation_id": "01958f84-bad1-79f4-a1a1-4cb862314d90",
  "payload": {
    "state_id": "01958f84-bbcf-7ee6-bb34-8470c0c86336",
    "kind": "step",
    "git_oid": { "algorithm": "sha1", "hex": "874bc1…" },
    "logical_parent_state_id": "01958f7f-3d3d-70eb-a1c6-8a1ad1d86b02",
    "attempt_id": "01958f83-9194-73f1-bafb-1c81f1fb4c43",
    "label": "fast purple",
    "message": "Enable fast mode without changing the color model",
    "stats": { "changed_files": 1, "insertions": 2, "deletions": 1 },
    "capture_fingerprint": { "head": "874bc1…", "index_tree": "874bc1…", "dirty_digest": null }
  }
}
```

### Exact atomic pick

```json
{
  "event_type": "DeltaApplied",
  "event_schema_version": 1,
  "payload": {
    "composition_id": "01958f8a-b3b8-78db-95bc-6caf39aba41e",
    "source_state_id": "01958f84-bbcf-7ee6-bb34-8470c0c86336",
    "source_parent_state_id": "01958f7f-3d3d-70eb-a1c6-8a1ad1d86b02",
    "target_base_state_id": "01958f89-d737-7749-8910-a71e1deba15e",
    "patch_id": { "sha256": "ef71…", "byte_length": 812, "media_type": "application/vnd.jjk.patch" },
    "resolution_artifact": null,
    "result_state_id": "01958f8a-bd5e-7fba-aebb-5d9ac40bbbe0"
  },
  "provenance": {
    "algorithm": "git-diff-parent-to-state-v1",
    "source_git_parent": "7a29bf…",
    "source_git_state": "874bc1…",
    "target_git_base": "bc9921…"
  }
}
```

This event is invalid unless the same operation also contains the corresponding `StateCaptured` for `result_state_id`, and verification proves that the applied delta is the source parent→source state delta—not the full source history.

### Evidence-gated canonical promotion

```json
{
  "event_type": "CanonicalPromoted",
  "event_schema_version": 1,
  "payload": {
    "promotion_id": "01958f90-4b7c-7564-8b20-c1fa058941a2",
    "canonical_branch_id": "01958f70-f86b-7d29-b573-0591fc083f42",
    "source_state_id": "01958f8a-bd5e-7fba-aebb-5d9ac40bbbe0",
    "previous_state_id": "01958f81-19a7-7479-b327-3fa74910d6e7",
    "policy": { "name": "production", "version": 3 },
    "validation_ids": ["01958f8f-c6af-7bbc-b5c0-b048471097b2"],
    "resulting_ref_oid": { "algorithm": "sha1", "hex": "9cd822…" }
  }
}
```

### Component-honest Timeshift restore

```json
{
  "event_type": "TimeshiftRestored",
  "event_schema_version": 1,
  "payload": {
    "timeshift_id": "01958f99-5c65-7088-ae31-8cf85f0215f8",
    "requested_components": ["repository", "worktree", "relative-cwd", "terminal-layout", "agents"],
    "restored_components": ["repository", "worktree", "relative-cwd"],
    "skipped_components": [
      { "component": "terminal-layout", "reason": "adapter-unavailable" },
      { "component": "agents", "reason": "descriptors-recorded-processes-not-recreated" }
    ],
    "result_fingerprint": { "head": "9cd822…", "worktree_id": "01958f72…" }
  }
}
```

## Causality, ordering, and idempotency

### Ordering rules

1. `local_seq` is the only total event order within a journal generation.
2. `(operation_id, operation_ordinal)` orders facts inside one operation and MUST be contiguous from zero at commit.
3. `event_causes` captures cross-operation causal dependency; state graph edges are not inferred from adjacent sequence numbers.
4. `recorded_at_utc`, Git author/committer dates, and remote timestamps MUST NOT determine order. Clocks lie.
5. A multi-device/remote event is imported as a locally sequenced observation with immutable origin provenance. JJK v0.1 does not merge independent journal sequence spaces.
6. One repo has one active writer lock. Readers use SQLite read transactions and may continue during WAL writes.

### Command idempotency

`OperationId` is the public idempotency key for JJK-native and Git-enhanced mutations. The durable `OperationPrepared.request_hash` is SHA-256 over canonical command name, typed arguments, resolved targets, repo ID, and relevant preconditions.

- Retrying the same `OperationId` with the same request hash returns the recorded terminal result or resumes repair.
- Reusing it with a different hash fails `IdempotencyConflict` before any mutation.
- Transparent Git passthrough does not invent semantic events unless reconciliation observes a new fact. It MUST preserve argv bytes, cwd, stdio, env, signals, and exit code.
- Fact-level `dedup_key` prevents repeated external observation, for example `git-commit:<repo-id>:sha1:<oid>`.
- `StateCaptured` has no Git-OID dedup key because two intentional semantic states may point at the same commit. Its operation ID supplies retry idempotency.
- Reducers are pure and deterministic. Reapplying an event to a fresh projection has the same result; applying a sequence twice to an existing projection is prevented by each projection’s `last_event_seq` guard.

## Materialized projections

### EM-D006 — Journal append and projection advance share one SQLite transaction

```sql
CREATE TABLE projection_meta (
  projection_name       TEXT PRIMARY KEY,
  reducer_version       INTEGER NOT NULL,
  projected_through_seq INTEGER NOT NULL,
  projected_through_hash BLOB NOT NULL CHECK (length(projected_through_hash) = 32),
  projection_digest     BLOB NOT NULL CHECK (length(projection_digest) = 32)
) STRICT;

CREATE TABLE operations (
  operation_id        BLOB PRIMARY KEY,
  request_hash        BLOB NOT NULL,
  command_kind        TEXT NOT NULL,
  status              TEXT NOT NULL CHECK (status IN
                      ('prepared','applying','awaiting_resolution','verifying',
                       'committed','aborting','aborted','repair_required')),
  prepared_seq        INTEGER NOT NULL,
  terminal_seq        INTEGER,
  precondition_fingerprint BLOB NOT NULL,
  expected_effects    BLOB NOT NULL,
  recovery_artifact_hash BLOB,
  result              BLOB,
  last_event_seq      INTEGER NOT NULL
) STRICT;

CREATE TABLE operation_effects (
  operation_id        BLOB NOT NULL,
  effect_ordinal      INTEGER NOT NULL CHECK (effect_ordinal >= 0),
  effect_kind         TEXT NOT NULL,
  expected            BLOB NOT NULL,
  observed_receipt    BLOB,
  status              TEXT NOT NULL CHECK (status IN ('planned','observed','diverged','reversed')),
  last_event_seq      INTEGER NOT NULL,
  PRIMARY KEY (operation_id, effect_ordinal)
) STRICT;

CREATE TABLE states (
  state_id            BLOB PRIMARY KEY,
  created_seq         INTEGER NOT NULL,
  kind                TEXT NOT NULL,
  git_algorithm       TEXT NOT NULL,
  git_oid             TEXT NOT NULL,
  jj_change_id        TEXT,
  jj_commit_id        TEXT,
  attempt_id          BLOB NOT NULL,
  label               TEXT NOT NULL,
  message             TEXT,
  archived            INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
  last_event_seq      INTEGER NOT NULL
) STRICT;

CREATE TABLE state_logical_parents (
  child_state_id      BLOB PRIMARY KEY,
  parent_state_id     BLOB NOT NULL,
  created_seq         INTEGER NOT NULL
) STRICT;

CREATE TABLE state_provenance_edges (
  source_state_id     BLOB NOT NULL,
  result_state_id     BLOB NOT NULL,
  relation            TEXT NOT NULL CHECK (relation IN ('derived-from','composed-from','promotion-source')),
  provenance_id       BLOB NOT NULL,
  created_seq         INTEGER NOT NULL,
  PRIMARY KEY (source_state_id, result_state_id, relation, provenance_id)
) STRICT;

CREATE TABLE composition_inputs (
  composition_id      BLOB NOT NULL,
  source_state_id     BLOB NOT NULL,
  source_parent_id    BLOB,
  input_ordinal       INTEGER NOT NULL,
  PRIMARY KEY (composition_id, input_ordinal)
) STRICT;

CREATE TABLE composition_candidates (
  composition_id      BLOB NOT NULL,
  candidate_id        BLOB NOT NULL,
  attempt_id          BLOB NOT NULL,
  result_state_id     BLOB,
  disposition         TEXT NOT NULL CHECK (disposition IN ('pending','selected','rejected','incomparable')),
  created_seq         INTEGER NOT NULL,
  PRIMARY KEY (composition_id, candidate_id)
) STRICT;

CREATE TABLE attempts (
  attempt_id          BLOB PRIMARY KEY,
  root_state_id       BLOB NOT NULL,
  objective           TEXT NOT NULL,
  current_tip_state_id BLOB,
  archived            INTEGER NOT NULL DEFAULT 0,
  last_event_seq      INTEGER NOT NULL
) STRICT;

CREATE TABLE branch_bindings (
  branch_id           BLOB PRIMARY KEY,
  attempt_id          BLOB,
  refname             BLOB NOT NULL,       -- validated Git ref bytes; diagnostic rendering escapes
  observed_git_oid    TEXT,
  target_state_id     BLOB,
  canonical_role      TEXT,
  last_event_seq      INTEGER NOT NULL
) STRICT;

CREATE TABLE worktree_current (
  worktree_id         BLOB PRIMARY KEY,
  attempt_id          BLOB,
  active_state_id     BLOB,
  relative_locator    BLOB NOT NULL,
  head_oid            TEXT,
  index_tree_oid      TEXT,
  dirty_digest        BLOB,
  last_event_seq      INTEGER NOT NULL
) STRICT;

CREATE TABLE annotations_current (
  subject_kind        TEXT NOT NULL,
  subject_id          BLOB NOT NULL,
  annotation_kind     TEXT NOT NULL,
  annotation_id       BLOB NOT NULL,
  value               BLOB NOT NULL,
  last_event_seq      INTEGER NOT NULL,
  PRIMARY KEY (subject_kind, subject_id, annotation_kind, annotation_id)
) STRICT;

CREATE TABLE validations (
  validation_id       BLOB PRIMARY KEY,
  subject_kind        TEXT NOT NULL,
  subject_id          BLOB NOT NULL,
  suite               TEXT NOT NULL,
  outcome             TEXT NOT NULL CHECK (outcome IN ('pass','fail','error','skipped')),
  environment_fingerprint BLOB NOT NULL,
  evidence_manifest   BLOB NOT NULL,
  expires_at_utc      TEXT,
  recorded_seq        INTEGER NOT NULL
) STRICT;

CREATE TABLE compositions (
  composition_id      BLOB PRIMARY KEY,
  intent              TEXT NOT NULL,
  status              TEXT NOT NULL CHECK (status IN ('attempting','resolved','abandoned')),
  selected_state_id   BLOB,
  last_event_seq      INTEGER NOT NULL
) STRICT;

CREATE TABLE archives (
  archive_id          BLOB PRIMARY KEY,
  state_id            BLOB NOT NULL,
  prior_location      BLOB NOT NULL,
  hidden_ref          BLOB,
  archived_seq        INTEGER NOT NULL,
  recovered_seq       INTEGER
) STRICT;

CREATE TABLE backups (
  backup_id           BLOB PRIMARY KEY,
  through_seq         INTEGER NOT NULL,
  through_hash        BLOB NOT NULL,
  manifest_hash       BLOB NOT NULL,
  byte_length         INTEGER NOT NULL,
  verified            INTEGER NOT NULL CHECK (verified IN (0,1)),
  created_seq         INTEGER NOT NULL
) STRICT;

CREATE TABLE timeshifts (
  timeshift_id        BLOB PRIMARY KEY,
  state_id            BLOB,
  component_manifest_hash BLOB NOT NULL,
  excluded_classes    BLOB NOT NULL,
  captured_seq        INTEGER NOT NULL,
  last_restore_seq    INTEGER
) STRICT;
```

Additional FTS/search and graph-layout tables are disposable projections with their own watermarks. They cannot be used to authorize mutations.

Each append uses a short `BEGIN IMMEDIATE` transaction: validate the current journal head; insert the new events and cause links; run every reducer; advance every `projection_meta` watermark/hash, including no-op advancement for unaffected projections; check invariants; commit. If any reducer fails, neither those events nor projection changes become visible.

Operation lifecycle events before verification reduce only into operation/effect/reconciliation projections. **Committed domain projections (`states`, logical parents, attempts, compositions, archives, promotions, and current navigation) receive no unverified domain mutation.** After external verification succeeds, the writer atomically appends the domain facts plus `OperationCommitted` and reduces them into committed domain projections in the same SQLite transaction. This avoids both a long database transaction over external mutation and a graph that exposes unverified intent as truth.

Before a read that claims current truth:

```text
required_head = SELECT local_seq,event_hash FROM events ORDER BY local_seq DESC LIMIT 1
for every projection used:
    require projected_through_seq == required_head.local_seq
    require projected_through_hash == required_head.event_hash
otherwise rebuild or return ProjectionStale; never serve it as current
```

Thus there is no journal/projection dual truth: events are authority; projections are atomically synchronized conveniences whose authority to answer is mechanically gated by the head watermark. Prepared/applied/verifying operations are facts about work in flight, not committed semantic state.

## Cross-layer operation protocol

### EM-D007 — Durable preparation bridges SQLite and external substrates

The operation reducer enforces this transition table; every other transition is corruption:

| From | Event | To |
|---|---|---|
| absent | `OperationPrepared` | `prepared` |
| `prepared` | `ApplyStarted` | `applying` |
| `applying` | `EffectObserved` | `applying` |
| `applying` | `ConflictPaused` | `awaiting_resolution` |
| `awaiting_resolution` | `RepairResumed` | `applying` |
| `prepared` / `applying` / `awaiting_resolution` / `repair_required` | `AbortStarted` | `aborting` |
| `applying` | `VerificationStarted` | `verifying` |
| `verifying` | domain facts + `OperationCommitted` in one transaction | `committed` |
| `aborting` | `OperationAborted` | `aborted` |
| any nonterminal | `RepairRequired` | `repair_required` |
| `repair_required` | `RepairResumed` | `applying` or `verifying`, as the recorded strategy declares |

`committed` and `aborted` are terminal. `EffectObserved` is idempotent only when its canonical receipt bytes match the previously observed receipt for that effect; a mismatch is `EffectReceiptConflict`.

1. **Discover:** locate the safe space and adapters; read SQLite/Git/JJ/filesystem capabilities.
2. **Lock:** acquire the per-repo writer lock. No SQLite transaction remains open across external mutation.
3. **Reconcile:** in a short SQLite transaction, compare live Git/JJ/workspace fingerprints with the last observations; append idempotent observation events and projections or stop on ambiguity.
4. **Resolve:** turn labels/queries into typed IDs with explicit confidence. Automation MUST NOT pick an ambiguous fuzzy result.
5. **Plan:** compute exact effects as stable `EffectId = OperationId + ordinal`, expected ref/object/file outcomes, and rollback/forward-repair actions.
6. **Durable prepare:** in a short committed SQLite transaction append `OperationPrepared` and its projection **before external mutation**. It contains preconditions, desired end state, deterministic effect identities, and a content-addressed recovery artifact for non-reconstructible index/worktree bytes.
7. **Mutate:** append `ApplyStarted`, close that SQLite transaction, then apply Git/JJ/filesystem effects using compare-and-swap preconditions. After each reinspection, append `EffectObserved` in a new short SQLite transaction. A conflict appends `ConflictPaused` and enters `awaiting_resolution`.
8. **Verify:** after every expected effect has an observed receipt, append `VerificationStarted`, close the transaction, and inspect actual refs, objects, index, worktree, the intended graph transition, and requested invariant.
9. **Commit domain facts:** on an exact match, atomically append the state/composition/archive/promotion/etc. facts **together with `OperationCommitted`** and reduce committed domain projections. On mismatch append `RepairRequired`; no unverified domain fact appears in a committed graph projection.
10. **Commit/repair:** release the lock only after a terminal status or durable `repair_required`. Repair later begins with `RepairResumed`; abort uses `AbortStarted → OperationAborted` and must verify the restored fingerprint.

A crash after an external effect but before `EffectObserved`, or after `VerificationStarted` but before the atomic domain commit, is resolved from `OperationPrepared`: inspect every expected effect, classify it `not-applied`, `applied-exactly`, or `diverged`, then safely continue verification/commit, safely abort from the recovery artifact, or stop for explicit conflict resolution. The implementation MUST NOT manufacture a success event merely because some effect exists.

## Schema evolution

### Event schemas

- `(event_type, event_schema_version)` is immutable once released.
- Payload evolution is additive only when old readers can ignore the new optional field without changing semantics. Otherwise register version `N+1`.
- Pure, deterministic upcasters transform old payload bytes to the reducer’s current in-memory type. They MUST NOT query Git, the clock, environment variables, or projections.
- The original event bytes and hash remain unchanged. Upcast output is never written back as if it were the historical event.
- Unknown event type/version or required feature causes `ReaderTooOld` at the first unsupported sequence. A writer MUST NOT append beyond an event it cannot reduce. Diagnostic/raw export may remain available read-only.
- Golden fixtures cover every released payload version and the complete upcast chain.

### SQLite storage schemas

- `storage_schema_version` changes only through a registered migration operation.
- Before migration, create and verify a consistent SQLite backup plus reachable Git refs/objects manifest.
- Append `MigrationStarted`, then run the storage migration transaction. It may rebuild projections freely but MUST NOT rewrite `events.payload`, IDs, causal links, or hashes.
- On success append `MigrationCompleted`; on failure preserve the failed file, restore into a new file, and append `MigrationFailed` to the surviving journal when safe.
- Reducer versions are independent of storage and payload versions. A reducer-version mismatch invalidates its projection and triggers rebuild.

## Replay algorithm

Pure replay never inspects or mutates Git/JJ/files. Reconciliation is a separate operation after replay.

```text
replay(database, target_seq = journal_head):
  1. open read-only; verify application_id, repo_id, storage version, SQLite integrity_check
  2. locate newest usable snapshot with through_seq <= target_seq
  3. verify snapshot reducer versions, manifest hash, through_event_hash, and projection digest
  4. verify the event hash chain from the last sealed anchor (or genesis) through target_seq
  5. validate envelope, typed payload schema, UUID types, operation ordinal, and causal parents
  6. begin one write transaction for projections
  7. clear all derived tables; restore snapshot only if step 3 passed, otherwise start empty
  8. for each event after snapshot through target_seq in local_seq order:
       upcast payload in memory
       enforce event preconditions against reducer state
       apply every required reducer
       set each touched row.last_event_seq = event.local_seq
  9. run global invariants; compute deterministic projection digests
 10. update projection_meta to target_seq/hash and commit
 11. reopen a read transaction and confirm every required watermark equals the requested target
```

A snapshot is only an accelerator. If it is absent, stale, corrupt, or built by an incompatible reducer, replay starts from genesis. `jjk query --at <event|seq>` uses the newest valid prior snapshot plus replay into a temporary/in-memory projection; it does not roll the live repository backward.

## Snapshots, compaction, and retention

```sql
CREATE TABLE journal_snapshots (
  snapshot_id          BLOB PRIMARY KEY,
  through_seq          INTEGER NOT NULL,
  through_event_hash   BLOB NOT NULL CHECK (length(through_event_hash) = 32),
  reducer_manifest     BLOB NOT NULL,
  projection_blob      BLOB NOT NULL,
  projection_digest    BLOB NOT NULL CHECK (length(projection_digest) = 32),
  created_at_utc       TEXT NOT NULL,
  UNIQUE (through_seq, through_event_hash, projection_digest)
) STRICT;
```

For v0.1, **compaction MUST NOT delete events**. Expected metadata size is small compared with repository objects, and deletion would spend safety for premature optimization. JJK may:

- checkpoint/truncate WAL after readers release it;
- replace old projection snapshots, retaining at least the latest two known-good snapshots;
- rebuild or vacuum disposable projection/index tables;
- externalize large artifacts while retaining their hashes and verified manifests.

A future bounded-store mode may seal old journal ranges into immutable, checksummed segment files. It is acceptable only when the logical authority is explicitly defined as `verified sealed segments + live SQLite tail`, two independent restore drills reproduce the same terminal hash/projection digest, the segment exists in a verified backup, and the operation retains a manifest in the live journal. Routine “squash events into a snapshot” is prohibited because snapshots are derived, not truth.

## Query contract

Every query declares a consistency and temporal scope:

```rust
enum Consistency { Current, AtSeq(u64), AtEvent(EventId) }
struct GraphQuery {
    consistency: Consistency,
    roots: Vec<StateId>,
    attempts: Vec<AttemptId>,
    include_archived: bool,
    edge_kinds: Set<StateEdgeKind>,
    depth: Option<u32>,
}
```

Required query surfaces:

- `current(worktree_id)` → active state, attempt, branch binding, exact Git/workspace observation, and whether live substrate has unreconciled drift.
- `state(state_id)` → capture fact, current annotations, logical parent, Git/JJ identities, archive state, evidence, outgoing/incoming provenance edges.
- `graph(query)` → state nodes and typed edges; response includes `projected_through_seq`, `journal_head_seq`, and `is_complete`.
- `attempt(attempt_id)` → root/tip, branch/worktree bindings, ownership/objective, validations, candidate/composition relationships.
- `composition(composition_id)` → sources, exact source-parent deltas, candidates, conflicts/resolutions, validations, selection.
- `canonical(role)` → current promoted state, prior state, promotion policy, required validation IDs, rollback lineage, observed ref OID.
- `story(filters)` → curated annotations/states without changing topology.
- `operations(status = prepared|applying|awaiting_resolution|verifying|committed|aborting|aborted|repair_required)` → operation history, recovery queue, and deterministic next action.
- `events(after_seq, type, operation_id, actor_id)` → raw audit stream.
- `as_of(seq|event)` → temporary replay result, never a mutation of live projections.

Example logical-ancestry traversal (composition provenance is queried separately and never creates extra logical parents):

```sql
WITH RECURSIVE walk(state_id, depth) AS (
  VALUES (:root_state_id, 0)
  UNION ALL
  SELECT p.child_state_id, walk.depth + 1
  FROM state_logical_parents p
  JOIN walk ON p.parent_state_id = walk.state_id
  WHERE walk.depth < :max_depth
)
SELECT s.*, walk.depth
FROM walk JOIN states s USING (state_id)
WHERE (:include_archived = 1 OR s.archived = 0);
```

Queries MUST state when archived nodes, unsupported adapters, missing artifacts, or unreconciled external changes make a result filtered or incomplete.

## Invariants

| ID | Invariant |
|---|---|
| EM-I001 | Event rows, cause links, and original payload bytes are immutable. Correction is a new event. |
| EM-I002 | The event hash chain begins at a fixed genesis hash and verifies through the claimed head. |
| EM-I003 | `local_seq` is gap-tolerant but strictly increasing; no semantic rule depends on contiguity because SQLite may consume row IDs on rollback. |
| EM-I004 | Each operation has exactly one `OperationPrepared` and at most one terminal `OperationCommitted` or `OperationAborted`; `repair_required` is durable but nonterminal. Its lifecycle follows the declared status transition table. |
| EM-I005 | An operation cannot commit until every expected effect has an observed receipt and the substrate fingerprint matches its plan or an explicitly recorded resolution. Domain facts and `OperationCommitted` append atomically; no unverified domain mutation enters committed projections. |
| EM-I006 | Every current projection used in an answer has the journal head sequence/hash watermark and expected reducer version. |
| EM-I007 | Every `StateId` is created once. Annotation, archive, recovery, validation, and promotion never replace it. |
| EM-I008 | Every non-root state’s logical parent exists; attempts may fork from any visible or archived state without deleting the prior future. |
| EM-I009 | Composition records source state, source logical parent, target base, patch identity, conflict resolution, and result. |
| EM-I010 | A validation is immutable evidence about one typed subject in one environment; later reruns create new validations. |
| EM-I011 | Promotion references an existing source, previous canonical state, satisfied policy/version, validation IDs, and verified resulting Git ref OID. |
| EM-I012 | Archive only changes visibility/bindings; recovery restores recorded graph location or stops on a conflicting live binding. |
| EM-I013 | Backup claims include journal sequence/hash, manifest hash, byte length, and Git ref/object reachability manifest; unverified exports are not cataloged as valid backups. |
| EM-I014 | Timeshift names every requested, restored, skipped, and secret-excluded component. Partial restore is never reported as full. |
| EM-I015 | Git/JJ identities, JJK identities, labels, refnames, and paths are never silently coerced into one another. |
| EM-I016 | Reconciliation is idempotent: an unchanged external substrate appends no duplicate fact. |
| EM-I017 | Unknown event semantics block writes and current projections rather than being skipped. |
| EM-I018 | Removing JJK leaves standard Git objects, refs, branches, and worktrees valid and intelligible. |

## Failure modes and handling

| Failure | Detection | Required handling |
|---|---|---|
| Process dies before durable prepare | No prepared operation; preconditions prove no JJK-authorized mutation | Reconcile external facts normally; do not infer command success |
| Process dies after prepare, before external mutation | Prepared operation plus unchanged precondition fingerprint | Resume or append `OperationAborted`; no semantic state was created |
| External mutation succeeds, event finalization does not | Nonterminal prepare plus expected effect observed live | Record/recover the receipt, verify exact effect, then atomically append domain facts+commit; otherwise abort or mark `repair_required` |
| Some external effects land, others do not | Per-effect receipt/fingerprint mismatch | Restore from recovery artifact or continue only deterministic missing effects; never publish partial semantic result |
| Concurrent Git user moves a ref | Compare-and-swap/precondition mismatch | Append `ConflictPaused` or `RepairRequired`, reconcile the new ref observation, and re-plan; never guess |
| Projection row/index corruption | Watermark, digest, invariant, or SQLite integrity failure limited to derived tables | Drop derived tables and replay; do not edit journal |
| WAL/main DB backup copied inconsistently | Backup integrity/hash/head mismatch | Reject backup; source remains untouched; recreate through backup API |
| Event payload/hash/chain corruption | Hash-chain or schema validation failure | Immediately reopen read-only, quarantine a byte-for-byte diagnostic copy, and stop writes. Recover into a new DB/generation from the newest verified backup/segment; never skip the row |
| Missing journal range with no verified copy | Broken chain and unavailable backup | Preserve evidence, declare exact gap through explicit forensic `JournalRepairDeclared`, start a new generation, and report semantic facts in the gap as honestly unrecoverable; Git observations may be re-imported but intent cannot be fabricated |
| Missing Git object backing a state | `git cat-file`/adapter verification fails | Keep state/event, mark substrate availability broken, attempt configured fetch/restore, and refuse activation/composition until repaired |
| Missing content-addressed artifact | Artifact hash/path check fails | Keep referencing event, mark evidence/material unavailable, restore from backup if possible; never silently drop provenance |
| Unsupported/new event version | Registry lookup fails | Read-only raw diagnostics; return `ReaderTooOld`; upgrade before mutation/replay past that sequence |
| Unsupported filesystem locking/WAL | startup capability probe and lock test | Use rollback journal + exclusive writer if proven safe, otherwise refuse mutation with remediation |
| Disk full during append | SQLite transaction error | Roll back event+projection transaction; prepared external operation remains repairable |
| Timeshift adapter unavailable | capability discovery | Preview and record skipped component; restore supported components only after explicit plan acceptance |

Corruption handling prioritizes preservation over availability. Repair occurs in a **new** database file and journal generation; the damaged bytes are never “fixed in place.” After recovery, JJK compares the recovered terminal hash, projections, Git reachability, refs, worktree/index fingerprints, and outstanding operations before atomically selecting the new file.

## Acceptance checks

Each check is a behavioral contract, not a source-text assertion.

| ID | Scenario | Required proof |
|---|---|---|
| EM-A001 Branches | Capture green; derive purple and orange as sibling attempts; extend both; replay from genesis. | Logical parents and attempt tips match before/after replay; stable `BranchId`s map to expected standard Git refs; neither future is lost. |
| EM-A002 External branch reconciliation | Create/move/delete branches with transparent Git passthrough and rerun reconciliation repeatedly. | Exactly one fact per distinct observation, no duplicate states, correct latest ref projection, exact passthrough argv/cwd/stdio/env/signals/exit code. |
| EM-A003 Atomic composition | From orange, pick only the parent→fast-purple delta. | Result is orange+fast and not purple; `DeltaApplied` records source, source parent, target base, patch hash, result state, and replay preserves all provenance edges. |
| EM-A004 Plural composition | Request “best of A and B” and produce two candidate attempts. | Both candidates remain isolated and queryable; selection/validation is a later event and does not erase the alternative. |
| EM-A005 Archive/recover | Archive a tip and a non-tip, query with/without archived nodes, then recover. | Default graph hides it; audit graph retains it; recovery restores recorded parent/attempt/location and refuses conflicting binding rather than guessing. |
| EM-A006 Validation | Record pass, fail, error, expired pass, and a second run in a different environment. | Every result is immutable and independently queryable; policy uses only eligible nonexpired evidence and never overwrites an older result. |
| EM-A007 Promotion | Promote a validated candidate to a canonical branch, crash at every protocol boundary, then roll back. | Ref and canonical projection never disagree silently; promotion cites policy/evidence/previous tip; rollback is a new event and restores the verified standard Git ref. |
| EM-A008 Backup | Create backup under WAL load, mutate further, verify and restore into a fresh target. | Backup integrity passes, declared byte length/hash/head match, Git objects/refs are reachable, and restored replay digest equals source at `through_seq`. Raw main-file-only copies are rejected. |
| EM-A009 Restore safety | Load an older backup while newer work exists. | Automatic pre-restore backup is verified first; restore records generation/mapping; undo/forward recovery can restore the exact pre-load control state without deleting either journal. |
| EM-A010 Timeshift | Capture repository, attempt, worktree, relative cwd, env allowlist, terminal layout, and agent descriptors; restore with two adapters unavailable. | Preview names exclusions; secrets never enter artifacts; supported components restore; unsupported components are explicitly skipped; result is reported partial. |
| EM-A011 Replay | Drop every projection and rebuild from genesis, then rebuild from each retained snapshot. | Projection digests and all public query results are byte-equivalent at the same event head. |
| EM-A012 Idempotency | Retry every mutation before prepare, after prepare, after each external effect, and after commit using the same operation ID. | One semantic outcome, no duplicate state/event fact, stable result; changed arguments with the same ID fail before mutation. |
| EM-A013 Ordering | Import commits with skewed author/committer clocks and identical timestamps; record concurrent causal operations. | Sequence and causal graph are deterministic and independent of wall-clock ordering. |
| EM-A014 Schema evolution | Open every released fixture; upcast and replay; inject an unknown future event. | Old fixtures produce current projections without rewriting events; future event stops at its sequence with `ReaderTooOld` and no partial write. |
| EM-A015 Projection drift | Corrupt/delete projection rows while preserving events. | Current query refuses stale/digest-mismatched data, rebuilds, and returns the same result; journal hashes remain unchanged. |
| EM-A016 Journal corruption | Flip a payload byte, remove a causal parent, truncate an artifact, and damage a WAL copy in separate throwaway fixtures. | Each is detected; source opens read-only; no row is skipped; recovery uses a verified copy/new generation or declares the exact unrecoverable gap. |
| EM-A017 Cross-layer fault injection | Kill the process between every step of `discover → ... → commit/repair` for state capture, return, pick, archive, and promotion. | Re-entry deterministically commits, rolls back, or stops repair-required; no false success and no unowned external mutation remain. |
| EM-A018 Git-only removability | Remove `.jjk` from a completed fixture and use only Git to clone, inspect branches, diff, build, and continue. | Repository remains valid and understandable; JJK metadata was enrichment, never hostage infrastructure. |

## Explicit non-goals

1. The event journal is not a replacement for Git’s object database, reflog, refs, index, or transport.
2. JJK v0.1 does not promise atomic distributed commits across SQLite, Git, JJ, filesystems, shells, terminals, and remote forges. It promises durable prepare, verification, and deterministic repair.
3. JJK v0.1 does not merge independent writable journal histories. Remote metadata is imported as provenance-preserving observations; multi-writer replication requires a later protocol.
4. Routine event deletion, event squashing, and history rewriting are not v0.1 compaction strategies.
5. Projection tables, FTS indexes, graph layouts, and snapshots are not alternate authorities and are never edited as a user-facing mutation API.
6. Timestamps do not establish causality or state ancestry.
7. A validation event does not imply trust beyond its recorded suite, environment, evidence, and expiry.
8. Timeshift does not claim to recreate arbitrary processes, secrets, or unsupported terminal/editor state. Capability gaps are data.
9. Labels, paths, branch names, Git OIDs, JJ IDs, and JJK IDs are not interchangeable shortcuts.
10. Transparent Git passthrough is not wrapped into a different command model; it remains byte- and behavior-preserving, followed only by idempotent observation/reconciliation.

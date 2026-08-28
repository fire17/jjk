# Migrations, Backups, and Portable State

**Status:** decision-grade architecture for JJK v0.1 rewrite  
**Scope:** legacy import, metadata schema evolution, backup/load, freeze/export, metadata synchronization, privacy, integrity, removal, and release rollback  
**Authority:** `VISION.md`, `vision_overhaul.md`, and the current v1 `RepoData`/snapshot/freeze implementation

## 1. Context

The current implementation keeps the meaning layer in `.jjk/repo.json` (`version: 1`), undo/redo snapshots in `.jjk/history.json`, named whole-store snapshots in `.jjk/backups/*.json`, freeze pairs in `.jjk/freezes/<id>.{bundle,json}`, and saved Git objects under `refs/jjk/states/*`. The JSON store is human-readable but has no atomic multi-file transaction, checksum, forward-compatible envelope, migration receipt, or safe concurrent writer model. Current backup load mutates branches and metadata directly; current freezes describe one state but do not authenticate the manifest or bundle.

The rewrite uses a SQLite WAL event journal plus materialized projections. SQLite is retained after challenge rather than adopted by fashion:

| Candidate | Strength | Disqualifying cost for JJK v0.1 |
|---|---|---|
| JSON files | Inspectable, dependency-free | No safe concurrent mutation, weak partial-write recovery, expensive whole-store rewrites, migration ambiguity |
| Append-only files + custom indexes | Simple journal | JJK would have to invent transaction, index, compaction, locking, and corruption-recovery machinery |
| LMDB/RocksDB | Strong embedded storage | Poorer portable inspection and migration tooling; extra native dependency; transactions do not model relational integrity as directly |
| SQLite WAL | Atomic transactions, constraints, online backup API, mature integrity checks, one Rust-process dependency | WAL/network-filesystem caveats and single-writer serialization |

**Decision MB-001:** SQLite WAL is canonical for local journal and projections. JJK must detect filesystems on which WAL locking is unsafe and either use SQLite rollback-journal mode under the same repository lock or refuse mutation with a diagnostic. It must never silently copy a live `.db`, `-wal`, and `-shm` trio as a backup.

**Decision MB-002:** Git remains the durable object substrate. A metadata backup without its required Git objects is incomplete; a Git bundle without the meaning layer is not a JJK backup.

**Decision MB-003:** backup/load, freeze/import, metadata sync, and migration all use the normal cross-layer mutation protocol:

`discover → lock → reconcile → resolve → plan → durable prepare → mutate Git/JJ/files → append events+projections → verify → commit/repair`

No recovery command receives a privileged shortcut around that protocol.

## 2. Vocabulary and command classes

| Operation | Class | Meaning |
|---|---|---|
| `jjk migrate` | JJK-native | Import or advance the local metadata schema without changing project content |
| `jjk backup create/list/verify` | JJK-native | Capture and validate the complete local JJK control plane |
| `jjk backup load --preview/--into` | JJK-native | Plan or restore a backup, defaulting to a new target |
| `jjk freeze create/inspect/import` | JJK-native | Create or consume a portable selected-state/attempt bundle |
| `jjk metadata sync/push/pull` | JJK-native | Exchange explicitly shareable meaning-layer facts |
| `jjk export` | JJK-native | Produce a documented, non-mutating external representation |
| `jjk remove` | JJK-native | Detach JJK after reachability and recovery checks |
| `jjk git …` / unknown command passthrough | transparent Git passthrough | Preserve argv bytes, cwd, stdio, env, signals, and exit code exactly |

A backup is a disaster-recovery artifact. A freeze is a portable handoff/archive artifact. A metadata sync pack is a mergeable collaboration artifact. They are separate formats and MUST NOT be accepted interchangeably.

## 3. Storage layout

The canonical repository-wide control root is `<git-common-dir>/jjk/`, shared by every linked worktree. Per-worktree `.jjk/` is legacy input only; v0.1 must not create independent databases in worktree roots.

```text
<git-common-dir>/jjk/
├── state.sqlite3               # journal + projections; canonical metadata
├── state.sqlite3-wal           # transient; never copied directly
├── state.sqlite3-shm           # transient; never copied directly
├── lock                        # repository operation lock; transient, excluded from artifacts
├── recovery/                   # durable operation plans, compensation data, staged restores
├── migrations/
│   ├── legacy-v1/              # immutable source copies and import receipt
│   └── receipts/               # one receipt per applied schema step
├── backups/                    # default local destination
├── freezes/                    # default local destination
├── sync/                       # fetched packs, cursors, quarantine
└── quarantine/                 # corrupt/untrusted imports and removal staging
```

`state.sqlite3` contains `PRAGMA user_version`, but schema identity is not represented by that integer alone.

```rust
struct SchemaIdentity {
    format: &'static str,          // "jjk-store"
    major: u16,                    // incompatible reader boundary
    minor: u16,                    // additive/compatible change
    migration_set: [u8; 32],       // SHA-256 of ordered embedded migration descriptors
}

struct MigrationDescriptor {
    id: &'static str,              // e.g. "core/0003-add-provenance-class"
    from: SchemaIdentityRange,
    to: SchemaIdentity,
    forward_sql_sha256: [u8; 32],
    verifier_id: &'static str,
    compatibility: Compatibility,
}

enum Compatibility { Expand, Contract, DataRewrite }
```

Every descriptor is compiled into the binary. There is no runtime execution of SQL supplied by a backup or sync peer.

## 4. Global invariants

- **MB-I001 — one repository identity:** one imported legacy safe space maps to one `RepositoryId`; path, label, branch, and lane names are mutable facts, never identity.
- **MB-I002 — typed stable IDs:** new identities are UUIDv7 stored as 16-byte values and rendered with type prefixes (`st_`, `at_`, `br_`, `ws_`, `evt_`, `op_`, `prov_`, `ver_`, `arc_`). Legacy eight-character IDs survive as aliases/provenance, not as new primary keys.
- **MB-I003 — replayable truth:** accepted events are immutable. Projection rows are disposable and reproducible from the journal plus declared snapshot/compaction checkpoints.
- **MB-I004 — idempotent transforms:** each schema step and each legacy entity has a uniqueness key. Repeating import, migration, load, freeze import, or sync produces no duplicate semantic fact or Git ref movement.
- **MB-I005 — no mutation before durable prepare:** the operation record and compensation plan are committed before Git refs, workspaces, or external files change.
- **MB-I006 — committed backup boundary:** backup captures one committed database boundary. Nonterminal operations and their recovery artifacts are retained; transient lock ownership is not.
- **MB-I007 — source preservation:** migration never edits or deletes `.jjk/repo.json`, `history.json`, legacy backups, legacy freezes, or source Git refs. It first copies them byte-for-byte to an immutable migration capsule.
- **MB-I008 — restore is not overwrite by default:** load restores into a new directory/repository identity unless the operator explicitly selects the current safe space. Even then it creates and verifies an automatic pre-load backup first.
- **MB-I009 — exact Git proof:** every state that claims a Git object must resolve to that algorithm-tagged OID after import/restore. Missing objects quarantine the artifact; no placeholder state is silently made current.
- **MB-I010 — privacy follows data:** transport and backup policy is based on field classification, not filename. Unknown future fields default to local-sensitive and are not synced.
- **MB-I011 — forward preservation:** an older compatible binary may ignore an unknown event for display, but must preserve it byte-for-byte and must not rebuild/write projections it cannot understand.
- **MB-I012 — no dual truth:** after cutover, `<git-common-dir>/jjk/state.sqlite3` is canonical. Legacy JSON is read-only evidence. There is no permanent JSON/SQLite dual-write path.

## 5. Canonical migration records

```rust
type OperationId = TypedUuid<Operation>;       // op_
type EventId = TypedUuid<Event>;               // evt_
type RepositoryId = TypedUuid<Repository>;
type StateId = TypedUuid<State>;               // st_
type AttemptId = TypedUuid<Attempt>;           // at_
type BranchId = TypedUuid<Branch>;              // br_
type WorkspaceId = TypedUuid<Workspace>;       // ws_

struct LegacySourceId(String); // "repo-v1:<safeSpaceId>:<createdAt>"

struct LegacyIdMap {
    source_id: LegacySourceId,
    entity_kind: LegacyEntityKind,
    legacy_key: String,
    target_id: [u8; 16],           // allocated UUIDv7 once
    source_sha256: [u8; 32],
    imported_by: OperationId,
}
struct JournalHead {
    through_seq: u64,
    through_event_hash: [u8; 32],
}

struct EventEnvelope {
    event_id: EventId,
    repo_id: RepositoryId,
    local_seq: u64,
    event_type: String,
    event_schema_version: u16,
    envelope_version: u16,
    operation_id: OperationId,
    operation_ordinal: u32,
    actor: ActorRef,
    recorded_at_utc: String,
    observed_at_utc: Option<String>,
    repository_fingerprint: [u8; 32],
    payload_cbor: Vec<u8>,          // canonical CBOR
    provenance: Vec<ProvenanceRef>,
    evidence: Vec<EvidenceRef>,
    dedup_key: String,
    prev_event_hash: [u8; 32],
    event_hash: [u8; 32],
}


struct MigrationReceipt {
    migration_id: String,
    source_schema: SchemaIdentity,
    target_schema: SchemaIdentity,
    started_at_utc: String,
    committed_at_utc: String,
    binary_version: String,
    input_sha256: [u8; 32],
    output_journal_head: JournalHead,
    row_counts: BTreeMap<String, u64>,
    verification_sha256: [u8; 32],
}
```

Database constraints:

- `UNIQUE(source_id, entity_kind, legacy_key)` on `legacy_id_map`;
- `UNIQUE(migration_id)` on receipts;
- `UNIQUE(origin_replica_id, origin_event_id)` on imported events;
- every projection foreign key is immediate unless a documented graph cycle requires a deferred constraint;
- event payloads carry an explicit `payload_version` and canonical-CBOR bytes; JSON is an export/view, not the signed representation.

## 6. Legacy `.jjk/repo.json` import

### 6.1 Discovery and source identity

The importer recognizes only a parsed top-level object with `version == 1`. It computes SHA-256 for every discovered legacy file and records file mode, byte length, and relative path. `LegacySourceId` is derived from `safeSpaceId` and `createdAt`; if that pair is already associated with different repository-root evidence, migration stops with `LEGACY_IDENTITY_COLLISION` and requires explicit selection. It never guesses based on the current folder name.

Before import, JJK reconciles current Git refs/OIDs read-only and records the observed ref snapshot. It does not run ordinary external-commit reconciliation into the legacy JSON.

### 6.2 Complete `RepoData` mapping

| Legacy source | Canonical destination | Rule |
|---|---|---|
| `version` | import provenance + `MigrationCompleted { source: "legacy-v1" }` | Must equal `1`; never copied as current schema version |
| `safeSpaceId` | repository legacy alias | Preserved exactly; new `RepositoryId` allocated once |
| `createdAt` | repository `created_at` | Parse RFC3339 preserving original text in provenance; invalid timestamp blocks import |
| `updatedAt` | repository last-observed legacy timestamp | Not trusted as journal order; preserve original |
| `settings.watchDebounceMs` | local setting `watch.debounce_ms` | Integer range 10..600000; out-of-range quarantines rather than clamps |
| `settings.autoStatePrefix` | local setting `capture.auto_prefix` | Preserve UTF-8 exactly |
| `settings.showWorkspaceSnapshotsInGit` | local setting `git.show_workspace_snapshots` | Missing means `false`, with `defaulted=true` provenance |
| `states[]` | `states` projection + state-created/imported events | Preserve array order as `legacy_ordinal`; identity through `LegacyIdMap` |
| `lanes{}` | `attempts` plus branch links | Each lane becomes one attempt; map key and `name` both preserved; mismatch is a warning, not silently normalized |
| `branchLaneMap{branch: lane}` | branch↔attempt association | Create explicit `BranchId` and `AttemptId`; missing lane target blocks cutover |
| `allowMainBranchSave` | discarded transient compatibility flag | Preserve in provenance capsule; do not make it durable behavior |
| `returnContext` | workspace navigation/return intent | See field mapping below; imported only if referenced state exists |
| `currentStateHistory` | navigation visits + cursor | Preserve valid sequence, duplicates, and cursor; dangling IDs reported and omitted only under explicit `--repair=dangling-navigation` |
| `timeshifts[]` | situation captures | Import with local-sensitive classification; environment never becomes syncable by default |
| `freezes[]` | artifact catalog entries | Link only after pair and checksum validation; unverified legacy pair is cataloged as quarantined evidence |

### 6.3 Complete `StateRecord` mapping

| Legacy field | Canonical mapping |
|---|---|
| `id` | legacy alias; mapped once to `StateId` |
| `kind` | typed state intent; only `new|git|save|stash|cherry|step|nice|star|auto`; unknown kind blocks import rather than coercing |
| `label` | mutable label fact, exact UTF-8 |
| `description` | state description, project-private by default |
| `createdAt` | event effective time plus original-text provenance; journal order remains import order |
| `branch` | explicit `BranchId`; branch name is an alias |
| `lane` | explicit `AttemptId`; never inferred from branch even if names match |
| `continuationBranch` | optional branch association/continuation intent |
| `commit` | algorithm-tagged `GitOid` and state snapshot edge |
| `parentCommit` | Git parent fact, not logical state ancestry |
| `parentStateId` | logical parent edge after second-pass ID resolution |
| `tags[]` | normalized tag relation while preserving original tag spelling/order in provenance |
| `stats.changedFiles` | observed legacy statistic with unit `files` and `unverified` evidence status |
| `stats.insertedLines` | observed legacy statistic with unit `lines`; absence stays unknown, not zero |
| `stats.deletedLines` | observed legacy statistic with unit `lines`; absence stays unknown, not zero |
| `metadata.gitCommit` | Git OID corroboration; it must equal `commit`, or import stops with `STATE_OID_CONFLICT` |
| `metadata.message` | state message, project-private by default |
| `metadata.base` | composition target/base state alias; resolve to provenance edge |
| `metadata.cherry` | composition source state alias; resolve to provenance edge |
| `metadata.stashFromBranch` | stash origin branch association |
| `metadata.stashFromStateId` | stash origin state edge |
| `metadata.deletedAt` | archive event effective time |
| `metadata.deletedBranch` | legacy archive branch label |
| `metadata.deletedLocation.branch` | archive recovery context branch |
| `metadata.deletedLocation.lane` | archive recovery context attempt |
| `metadata.deletedLocation.continuationBranch` | archive recovery continuation branch |
| `metadata.deletedLocation.parentStateId` | archive recovery logical parent |
| `metadata.deletedLocation.wasLaneCurrent` | archive recovery cursor fact |
| `metadata.priorContexts[]` | ordered prior-context provenance records |
| `priorContexts[].branch` | prior branch association |
| `priorContexts[].lane` | prior attempt association |
| `priorContexts[].continuationBranch` | prior continuation association |
| `priorContexts[].updatedAt` | prior-context effective time |

If `metadata` is absent, `metadata.gitCommit` is synthesized from `commit` with `defaulted=true`, matching the current normalizer without rewriting source evidence. `star` remains accepted as a legacy state kind, but curation is imported as a tag/event on the same state; no second canonical snapshot is created merely for curation.

### 6.4 Lanes, branches, return context, and navigation

| Legacy field | Canonical mapping |
|---|---|
| `lanes[key].name` | attempt label |
| `.branch` | attempt’s active `BranchId` |
| `.baseRef` | unresolved Git ref expression plus resolved OID at import time, when available |
| `.createdAt` / `.updatedAt` | attempt effective timestamps |
| `.currentStateId` | attempt tip/cursor; must target an imported state or be null |
| `returnContext.stateId` | pending-return target state |
| `.sourceBranch` | pending-return source branch |
| `.sourceLane` | pending-return source attempt |
| `currentStateHistory.entries[]` | ordered `navigation_visits`, one row per occurrence |
| `currentStateHistory.index` | workspace `navigation_cursor`; `-1` only for empty history |

Branch aliases are validated with Git’s ref-format rules but are not rewritten. An invalid historical branch name remains displayable provenance; it cannot be materialized as a Git ref without explicit repair.

### 6.5 Timeshifts

Every `TimeshiftRecord` field is imported: `id` as legacy alias to a new situation-capture ID; `label`; `createdAt`; explicit branch and attempt; nullable `stateId`; `relativeCwd`; and every key/value in `env`. `relativeCwd` must be relative, normalized, and unable to escape the repository. Unsafe paths block that capture only and quarantine its raw record. Environment keys are never executed or injected during import/restore. Known current keys (`SHELL`, `TERM`, `COLORTERM`) and unknown keys receive the same local-sensitive treatment.

### 6.6 Undo/redo `history.json`

Legacy `SnapshotHistory` maps completely:

- top-level `version` must be `1`;
- `index` becomes the undo cursor;
- every entry’s `id`, `createdAt`, and `reason` become an imported control snapshot record;
- each embedded `repo` is parsed through the same complete mapping rules, but stored as a content-addressed historical legacy image rather than replayed as new current facts;
- `git.currentBranch`, `git.headCommit`, and every `git.branches{name: oid}` become that snapshot’s exact ref/control image.

Identical embedded repositories are deduplicated by SHA-256, not dropped. Array position is preserved. A malformed entry does not shift the cursor: migration stops unless the operator chooses a repair plan that records the original index and the omission explicitly. Undo/redo history is bounded after import according to configured retention, but the immutable legacy migration capsule retains the full source.

### 6.7 Legacy backups

Each `.jjk/backups/*.json` is parsed as a `WorkspaceSnapshot` and mapped completely: `id`, `createdAt`, `reason`, embedded full `RepoData`, current branch, HEAD OID, and the complete local branch map. It is cataloged as `legacy-unverified`, never described as a verified v0.1 backup. The importer:

1. hashes and preserves the original bytes;
2. verifies every OID syntax and available object;
3. converts the snapshot into the new backup format only when all required objects are available;
4. records `converted_from_sha256` and never overwrites the legacy file;
5. leaves incomplete conversions quarantined with an exact missing-OID list.

### 6.8 Legacy freezes

Every `FreezeRecord` field in `repo.json` is retained: `id`, `stateId`, `createdAt`, `bundlePath`, and `manifestPath`. The referenced legacy manifest’s `id`, full `state`, `createdAt`, and `generatedAt` are parsed; the `.bundle` is checked using Git bundle verification. Path resolution is confined beneath the safe-space root and rejects symlinks escaping it. A valid legacy freeze is re-packed into the new format because the legacy pair has no authenticated cross-file checksum.

### 6.9 Migration execution and cutover

1. **Discover:** inventory legacy files, Git repository format/hash algorithm, refs, worktrees, JJ presence, and current binary/schema.
2. **Lock:** acquire the repository operation lock; readers may continue on legacy data, no writer may start.
3. **Reconcile read-only:** capture exact refs, HEAD/index/worktree status and outstanding operation evidence.
4. **Resolve:** parse all legacy structures, construct `LegacyIdMap`, resolve second-pass edges, and produce a deterministic plan.
5. **Plan:** print counts, warnings, repairs, bytes, required objects, privacy findings, and cutover/rollback paths. `--check` stops here.
6. **Durable prepare:** create `<git-common-dir>/jjk/migrations/legacy-v1/<operation-id>/`, copy source bytes, fsync files and directory, create pre-migration full backup, commit operation status `prepared` in the staged database, and append `MigrationStarted` in the operation event sequence.
7. **Build:** populate a new database at a recovery-staging path in one transaction; do not mutate Git or source files.
8. **Verify:** run all checks in section 14, replay projections, compare row/edge/ref counts, and write a signed-by-checksum receipt.
9. **Activate:** embed schema identity, migration receipt, and an activation nonce inside the staged DB; fsync it; then atomically rename that single self-identifying DB to `<git-common-dir>/jjk/state.sqlite3`. Any human-readable format marker is regenerated from the active DB and is never an activation authority. No legacy file is renamed or deleted.
10. **Commit:** append `MigrationCompleted` (or `MigrationFailed` before abort/repair), mark the operation `committed`, fsync, unlock, and render the recovery command.

Crash behavior is determined only by durable operation status: `prepared` discards/reuses staging; `applying|verifying` resumes verification; `repair_required` forbids ordinary mutation; `committed` makes retry a no-op. Re-running migration over unchanged input returns the original receipt. Changed legacy input after cutover is reported as drift and is never merged implicitly.

## 7. Versioned schema migrations

**MB-004:** migrations are a linear, checksum-pinned sequence. Branching migration histories are forbidden in released binaries.

- **Expand step:** add nullable table/column/index/event decoder; old supported binary still operates.
- **Data rewrite:** populate new representation in bounded transactions with a persistent cursor and deterministic verifier.
- **Contract step:** remove old representation only after the compatibility window has expired and a verified backup exists.

Each step transitions an `operations` row through canonical statuses:

`prepared → applying → verifying → committed`

Failure transitions are:

`applying|verifying → aborting → aborted`, or `repair_required` when an external effect cannot be compensated automatically. `awaiting_resolution` is used only for an explicit ambiguity requiring a human selection; it is not a generic error bucket.

A migration transaction records affected schema objects, journal head before/after, backup ID, progress cursor, verifier output, and compensation procedure. SQL transaction rollback is sufficient only when no external effect occurred. Git/JJ/filesystem effects always require the durable operation protocol.

Migrations are monotonic. JJK does not destructively down-migrate a live database. Application rollback is provided by the compatibility protocol in section 15.

## 8. Backup format and creation

### 8.1 Artifact

A backup is one immutable directory or deterministic archive (`.jjkbak`, tar+zstd when available; uncompressed tar remains required for universal recovery):

```text
backup.jjkbak/
├── manifest.json
├── manifest.sha256
├── metadata/state.sqlite3
├── git/objects.bundle
├── git/refs.json
├── workspaces/control.json
├── workspaces/<ws-id>/index.bundle
├── workspaces/<ws-id>/tracked.patch
├── workspaces/<ws-id>/untracked.tar
├── recovery/operations.tar
└── legacy/                     # optional preserved migration capsule
```

The SQLite file is produced with SQLite’s online backup API from one committed read snapshot. JJK's repository lock excludes JJK writers but cannot exclude native Git/JJ/IDE writers. Backup therefore hashes every captured mutable payload as it is copied, then re-observes and re-hashes the corresponding live ref set, index bytes, tracked patch basis, untracked content, recovery artifact, and workspace control fact before accepting the manifest. Endpoint fingerprints are only a cheap drift gate; matching endpoints never prove consistency. Any payload/post-observation mismatch—including an A→B→A race—rejects the attempt. Where a filesystem cannot provide stable re-observation, backup requires operator-established quiescence or a proven filesystem snapshot and records that primitive. `-wal` and `-shm` are absent from the artifact.

`git/objects.bundle` includes all OIDs reachable from captured ordinary refs, `refs/jjk/*`, operation recovery anchors, and states in the database. `git/refs.json` records symbolic HEAD, peeled refs, object format, remotes without credentials, and per-worktree HEAD/branch. Index and dirty files are separate because neither Git bundle nor metadata DB contains them.

### 8.2 Manifest

```rust
struct JournalHeadManifest {
    through_seq: u64,
    through_event_hash: String,     // lowercase hex of JournalHead hash bytes
}

struct BackupManifestV1 {
    format: String,                 // "jjk-backup"
    format_version: u16,            // 1
    backup_id: String,              // bkp_<uuidv7>
    repository_id: String,
    created_at_utc: String,
    created_by_version: String,
    schema: SchemaIdentity,
    journal_head: JournalHeadManifest,
    operation_boundary: String,
    git_object_format: String,      // "sha1" or "sha256"
    privacy: PrivacyManifest,
    artifacts: Vec<ArtifactDigest>,
    required_oids: Vec<String>,
    refs_sha256: String,
    restore_capabilities: Vec<String>,
    source_backup_id: Option<String>,
}

struct ArtifactDigest {
    path: String,                   // normalized relative path
    media_type: String,
    size_bytes: u64,
    sha256: String,
    required: bool,
}
```

`manifest.json` is canonical JSON: UTF-8, sorted object keys, no insignificant whitespace, integer byte counts, RFC3339 UTC timestamps. `manifest.sha256` authenticates those exact bytes. SHA-256 is chosen for ubiquitous disaster-tool availability; Git OIDs remain their native algorithm and are not treated as integrity digests for non-Git files. Optional signatures/encryption wrap, but never replace, checksums.

### 8.3 Create algorithm

1. Discover and reject an unresolved prior repair.
2. Lock and reconcile all Git/JJ/workspace facts.
3. Resolve privacy policy and destination; default destination permissions are owner-only.
4. Plan artifact list and estimated space; require free space ≥ `estimated_bytes * 1.2 + 64 MiB`.
5. Commit `BackupCreate` operation as `prepared`.
6. While holding the JJK lock, capture a pre-fingerprint, SQLite through the online backup API, refs, operation recovery artifacts, indexes, patches, and untracked content with per-payload hashes; then re-observe and re-hash every mutable source and capture a post-fingerprint. Reject any mismatch, ABA/torn-read evidence, or unsupported stable-read primitive.
7. Build Git bundle from an explicit temporary ref namespace; delete temporary refs only after the bundle verifies.
8. Hash every artifact, write canonical manifest last, fsync all files and parent directory.
9. Verify from the artifact, not the source: checksums, SQLite checks, projection replay sample/full policy, Git bundle, OID closure, safe paths, and clean extraction.
10. Atomically rename staging to final destination; append `BackupCreated`; mark operation committed.

A command is successful only after step 10. Output includes path, bytes, backup ID, journal head, privacy/encryption state, and an exact verify/restore command.

## 9. Load and restore

`jjk backup load` means restore, not metadata merge.
Restore event vocabulary is fixed: the durable plan appends `RestorePrepared`; successful cross-layer verification appends `RestoreApplied`; recovery that completes or compensates an interrupted restore appends `RestoreRepaired`. `BackupCreated` is appended only after destination-artifact verification. All four use the canonical `EventEnvelope`; names are not aliases for operation statuses.

### 9.1 Preview

`jjk backup load X --preview` performs no mutation and reports:

- format/schema compatibility and required minimum JJK version;
- checksum, signature, encryption, SQLite, Git bundle, and OID-closure results;
- repository identity match/mismatch;
- refs to create/move/delete;
- workspaces and dirty/index content to restore;
- local data that would be displaced;
- required bytes and path conflicts;
- outstanding operations that recovery will resume;
- exact mode: `--into <new-dir>`, `--current`, or metadata-only diagnostic extraction.

### 9.2 Restore into a new target (default)

1. Verify archive before extraction; reject absolute, `..`, device, hard-link, and escaping symlink entries.
2. Create a new empty target sibling with owner-only permissions.
3. Acquire a target-local lock and write a durable restore plan.
4. Extract to staging; verify all digests again.
5. Initialize Git object database, import bundle, and create refs under a temporary namespace.
6. Install SQLite DB, run migrations supported by the current binary, and recover nonterminal operations before accepting mutations.
7. Materialize requested branches/worktrees, index, tracked patch, and untracked files. Existing paths are impossible because target began empty.
8. Verify control state, then atomically publish the target directory/name where the platform permits.
9. Append `RestoreApplied` in the restored journal with source backup ID and destination identity; preserve original repository identity unless `--fork-identity` is explicit.

### 9.3 Restore current safe space

`--current` requires an explicit operator choice. JJK first creates and fully verifies an automatic backup named `pre-restore-<operation-id>`. Dirty work is included; it is never silently stashed or discarded. Restore then follows the ordinary durable effect protocol: stage every replacement, record per-resource preconditions and postconditions, apply refs/index/worktree/database effects in a declared order, observe after each effect, and finish in `committed` or `repair_required`. Ref updates use compare-and-swap against the values captured in the plan. JJK MUST NOT describe this heterogeneous transition as atomic.

A failure before the first external effect leaves the source untouched. A failure after any effect may leave a partially applied but fully journaled state; JJK preserves source, staged data, the verified pre-restore backup, and all observed external changes, then prints one exact resume or restore-from-backup command. It never promises rollback when a resource no longer matches the recorded postcondition. `undo` of a committed restore is implemented as load of the named pre-restore backup through the same protocol, not by reversing ad hoc SQL.

## 10. Freeze bundles

A freeze is the smallest self-describing portable subgraph that can reconstruct the selected state or attempt without the source repository.

```text
<name>.jjkfreeze/
├── manifest.json
├── manifest.sha256
├── metadata/events.cbor
├── metadata/view.json
├── git/objects.bundle
└── evidence/                    # explicitly selected verification evidence
```

A `FreezeManifestV1` includes: format/version, freeze ID, origin repository/replica, creation time/binary, root state(s), selected attempt(s), all included state/edge/event IDs, boundary parents excluded from the bundle, required OIDs, refs offered under `refs/jjk/imports/<freeze-id>/*`, artifact digests, privacy transformations, and required capabilities.

Creation computes closure over:

- selected state and its logical ancestry required to explain it, or a selected attempt and all included tips;
- composition inputs and provenance edges referenced by included states;
- verification evidence explicitly selected and safe to share;
- Git commits/trees/blobs required to materialize every included state.

It excludes navigation history, local workspace paths, environment variables, lock/recovery state, credentials, remotes containing credentials, unrelated branches, and local-only notes by default. `--include-local-sensitive` is explicit and requires encryption.

Import verifies first, places Git refs under quarantine/import namespace, deduplicates events by origin replica/event ID and states by globally typed ID, detects same-ID/different-content as corruption, previews name collisions, and then commits one standard operation. Import never moves a canonical branch or checks out a workspace unless separately requested.

## 11. Metadata synchronization

**MB-005:** metadata sync exchanges immutable, shareable event packs; it does not copy `state.sqlite3` and does not elect one mutable `repo.json` as winner.

Each replica owns one append-only remote stream under `refs/jjk/metadata/<repository-id>/<replica-id>`. A stream ref advances by compare-and-swap to a Git commit containing a canonical pack manifest and content-addressed event segments. Different replicas never force-update one another’s stream. Pull is union + validation + deterministic projection rebuild; duplicates are harmless through `(origin_replica_id, origin_event_id)` uniqueness.

Sync protocol:

1. discover remote capabilities and fetch stream refs without changing worktree;
2. lock metadata writer and reconcile local Git facts;
3. download packs to `.jjk/sync/quarantine`;
4. verify pack checksums/signatures, repository identity, schema compatibility, event IDs, causal parents, and privacy classes;
5. resolve semantic conflicts into explicit facts (for example, two labels or two candidate tips), never last-write-wins over history;
6. durable-prepare import plan and cursor updates;
7. import events in one DB transaction, rebuild affected projections, and verify;
8. commit cursor and emit sync result; only then advance this replica’s outgoing stream.

Git remains fully usable without metadata refs. Push/pull of metadata is opt-in per remote. An unsupported future event is preserved in quarantine or opaque compatible storage according to the compatibility declaration; it is never partially projected as a known older event.

## 12. Export and removal

### 12.1 Export

Exports are pure reads against a committed snapshot:

- `jjk export state|attempt --format json|cbor|freeze`;
- `jjk export graph --format json|dot`;
- `jjk export legacy-v1` only while the current state is exactly representable by legacy v1.

Every export includes `format`, `format_version`, schema identity, repository identity, journal boundary, generated timestamp, generating binary, privacy/redaction declaration, and SHA-256 manifest. A v1 export refuses—not approximates—attempts with multiple logical parents, unsupported event kinds, evidence/provenance unavailable in v1, SHA-256 Git repositories unsupported by consumer assumptions, or other nonrepresentable semantics. It emits the exact blocking IDs.

### 12.2 Removal

`jjk remove` is staged and reversible:

1. preview every JJK path, hook/integration entry, ref, worktree, and object whose reachability depends on JJK;
2. create and verify a full backup;
3. for each state object not reachable from ordinary refs, require one policy: create archival ordinary refs, export a freeze, or explicitly permit loss;
4. disable JJK hooks/shell integration using ownership markers; never edit unrelated user lines;
5. move `.jjk` atomically to a timestamped sibling quarantine, not immediate deletion;
6. remove only owned refs/integration after verifying chosen reachability policy;
7. prove ordinary Git status, branch tips, worktrees, remotes, and project files are unchanged;
8. print quarantine expiry and restore command.

Permanent purge is a separate explicit command after retention. JJ data is removed only if JJK created it and no non-JJK JJ usage is detected. JJK never deletes `.git` or project content.

## 13. Privacy and security

### 13.1 Data classes

| Class | Examples | Backup | Freeze/sync |
|---|---|---|---|
| `public` | state IDs, share-approved labels, Git OIDs | included | included |
| `project-private` | descriptions, messages, provenance, validation output | included | included only by policy |
| `local-sensitive` | absolute/relative cwd, navigation, worktree paths, environment, local remote URLs | included with warning | excluded by default |
| `secret` | credential-bearing URL, token-like environment value, private key material | excluded unless encrypted explicit override | always excluded from ordinary sync; explicit encrypted freeze only |

Unknown fields default to `local-sensitive`. Classification is attached to typed fields and events, then enforced by serializer allowlists. Regex secret scanning is a defense-in-depth warning, never the primary boundary.

Backups default to mode `0600` files/`0700` directories. Encryption uses an age-compatible recipient envelope when requested; JJK never invents key storage. A secret override requires `--include-secrets --encrypt-to <recipient>` and the manifest lists inclusion without revealing values. Temporary plaintext is created only in a private staging directory and removed after atomic completion; crash recovery reports its exact path.

Untrusted backup/freeze metadata is inert data: no shell expansion, environment injection, hook installation, command execution, or checkout filter execution during verification. Archive extraction is size/count bounded to prevent decompression bombs. Manifest artifact paths use a tagged lossless native-path encoding: UTF-8 relative text when round-trippable, otherwise platform-tagged bytes on Unix or UTF-16 code units on Windows. Decoded paths must be relative, component-safe, root-contained, and unique after the target platform's normalization/case-folding before any extraction.

## 14. Integrity checks

A backup, freeze, migration, or sync pack passes only when all applicable checks pass:

1. manifest format/version and canonical encoding;
2. manifest SHA-256 and every artifact SHA-256/byte length;
3. signature/encryption recipient policy, when declared;
4. SQLite `quick_check` for routine verification and `integrity_check` for create/load/migration;
5. SQLite `foreign_key_check` returns zero rows;
6. migration set checksum and receipt chain match the binary-supported schema;
7. event ID uniqueness, payload decode, causal references, per-replica sequence continuity, and journal-head closure;
8. deterministic projection rebuild yields the recorded projection digest/counts;
9. typed-ID prefixes match entity tables; UUID bytes are valid;
10. all Git OIDs match declared algorithm, exist, and have expected object type;
11. `git bundle verify` succeeds and required-OID closure is complete;
12. captured refs point to declared OIDs and symbolic refs resolve without cycles;
13. state snapshot OID agrees with imported `metadata.gitCommit`/`commit` evidence;
14. logical parents, attempt tips, navigation cursors, archive recovery links, freeze roots, and composition edges resolve;
15. workspace paths remain beneath their declared roots and archive paths cannot escape;
16. privacy allowlist and secret scanner produce no policy violation;
17. staged extraction/materialization succeeds on a throwaway target before current-space activation.

`verify` reports `PASS`, `WARN`, `FAIL`, or `UNSUPPORTED` per check. Any required `FAIL` or `UNSUPPORTED` blocks activation. `--force` cannot bypass checksum, path traversal, database integrity, identity collision, or missing required Git object failures.

## 15. Rollback between releases

**MB-006:** application rollback is compatibility-based, not destructive schema downgrade.

- Releases support reading/writing the current schema and the immediately previous release schema.
- Release N first ships only expand migrations. N−1 binaries remain valid against the expanded schema and preserve unknown columns/events.
- Data rewrites are resumable and retain the old representation for the compatibility window.
- Contract migrations ship only after N−1 is outside the supported rollback window and a verified pre-contract backup exists.
- A binary reads `SchemaIdentity` before any mutation. Too-new major/schema means read-only diagnostics and an exact upgrade command, never an attempted write.
- Feature gates prevent emitting semantics that the declared rollback binary cannot safely preserve while a rollout is still reversible.

Release procedure:

1. run `jjk migrate --check --target <N>` and verify the N−1 compatibility matrix;
2. create and restore-drill a full pre-upgrade backup;
3. apply expand/data migration under durable operation;
4. run N against the real surface and record migration receipt;
5. to roll application code back, stop writers, install N−1, run `jjk doctor --compat`, and resume only if it declares schema write-compatible;
6. if not compatible, do not down-migrate in place: run N in repair/read-only mode or restore the pre-upgrade backup into a new target and explicitly reconcile post-upgrade work through freezes/standard Git;
7. never overwrite the current repository merely to make an old binary start.

Every release fixture contains golden stores from all supported predecessors. CI opens, migrates, mutates, rolls application binary back, mutates with the supported previous binary, rolls forward, and compares journal/projection/Git truth. Contract release tests prove the older binary refuses safely rather than corrupting unknown data.

## 16. Failure modes and containment

| Failure | Detection | Required response |
|---|---|---|
| Crash copying legacy files | missing durable-prepare inventory/checksum | Resume copy to staging; source untouched |
| Crash building staged DB | operation `prepared|applying` | Delete/rebuild or resume bounded step; never activate partial DB |
| Crash during/after DB activation rename | active DB embedded activation nonce/receipt disagrees with durable migration evidence | Treat the single renamed DB as the only selector; verify it, finish the receipt, or restore the prior DB from the prepared recovery copy; regenerate any display marker |
| Legacy ID collision | same source/entity/key, different digest | `awaiting_resolution`; show both records; never merge automatically |
| `commit` vs `metadata.gitCommit` mismatch | import verifier | Block state import and cutover |
| Missing legacy state ref but object exists | ref/OID reconciliation | Recreate only in staged plan after explicit proof |
| Missing Git object | object closure check | Quarantine artifact; fetch only through explicit user-selected remote |
| Live DB copied naively | absent online-backup provenance or SQLite failure | Reject artifact as unsupported, even if it opens |
| Disk full during backup | free-space gate/write failure | Remove staging only after logging; retain source and operation evidence |
| Branch moves during restore | compare-and-swap mismatch | Enter `repair_required`; preserve both ref snapshots |
| Malicious archive path/link | extraction validator | Reject before extraction outside private staging |
| New event reaches old reader | compatibility declaration | Preserve opaque/read-only or quarantine; never reserialize lossy |
| Metadata peers disagree | multiple valid signed facts | Project explicit conflict/candidates; never wall-clock last-write-wins |
| Backup contains secret | serializer class violation/scanner | Fail before final rename; require explicit encrypted override where allowed |
| Restore partially materializes worktree | post-restore verifier | Keep staging and pre-restore backup; compensate from durable file inventory |
| Corrupt current DB | SQLite/journal verification | Stop mutations, open read-only, restore into new target from last verified backup |
| Removal would orphan objects | ordinary-ref reachability check | Require archive refs/freeze or explicit loss authorization |
| Network filesystem violates locking | startup filesystem/lock probe | rollback-journal under repo lock or refuse mutation; no silent WAL use |

## 17. Exact disaster drills

Each drill runs on a disposable clone/temp target, never the live safe space. A release is not backup-capable until all drills pass.

### DR-01 — total local metadata loss

1. Create a fixture with two attempts, a logical fork, atomic pick provenance, archived state, navigation history, timeshift, dirty tracked/untracked/index content, and one nonterminal prepared operation.
2. Run `jjk backup create --output <temp>/dr01.jjkbak`.
3. Run `jjk backup verify <temp>/dr01.jjkbak --full`; require all checks PASS.
4. Move the fixture’s `.jjk` and JJK refs out of the disposable clone to simulate loss.
5. Run `jjk backup load <temp>/dr01.jjkbak --into <temp>/restored`.
6. Run `jjk doctor --full` in restored target; require zero repair items.
7. Compare canonical graph export, refs digest, journal head, attempt tips, archive contexts, navigation cursor, index tree, tracked bytes, and untracked bytes to the manifest.
8. Resume/recover the prepared operation and prove ordinary mutation becomes available only afterward.

**Pass:** every declared byte/fact matches; original damaged fixture was never overwritten.

### DR-02 — interrupted migration at every boundary

For each failpoint after lock, source copy, operation prepare, each import batch, projection build, DB fsync, DB rename, receipt append, and commit:

1. clone the same legacy v1 fixture;
2. inject process termination at that failpoint;
3. rerun `jjk migrate`;
4. require either one committed receipt or one actionable `repair_required`, never two imports;
5. compare all entity/edge counts and legacy ID mappings with an uninterrupted golden migration;
6. hash original `repo.json`, `history.json`, backup, freeze, and refs before/after.

**Pass:** source hashes unchanged, final graph/export identical, no duplicate event/state/ref.

### DR-03 — corrupt/truncated/tampered backup

Create independent cases: flip one DB byte, truncate Git bundle, change manifest without updating checksum, change artifact and update only its listed digest but not manifest checksum, add `../escape`, escaping symlink, duplicate case-folded path, and decompression bomb metadata.

For every case run preview and load.

**Pass:** preview identifies the failed invariant; load performs zero target/source mutation; nothing is written outside private quarantine.

### DR-04 — current-space restore failure and undo

1. Create backup A, advance to state B with branch/ref/index/untracked changes.
2. Invoke load A `--current` and inject failure after each external-effect boundary.
3. Require a verified `pre-restore-*` backup and either exact B preservation or `repair_required` with both ref images.
4. Complete restore A, then invoke restore of the pre-restore backup.
5. Compare byte-for-byte workspace/index/untracked content and exact refs to B.

**Pass:** no silent loss; round trip A→B is exact.

### DR-05 — release rollback

1. Open an N−1 golden store with N and apply expand migration.
2. Mutate using N only within rollback-safe feature gates.
3. start N−1, require `doctor --compat` write-compatible, mutate, then reopen with N.
4. compare journal/event preservation and projection rebuild.
5. Repeat with an N-only incompatible event; N−1 must refuse mutation read-only.
6. Restore pre-upgrade backup into a new target and import post-upgrade Git work via freeze; never overwrite the upgraded target.

**Pass:** compatible rollback preserves all facts; incompatible rollback refuses safely with a complete recovery path.

### DR-06 — metadata sync convergence

1. Clone three replicas from the same boundary.
2. Create concurrent label changes, attempt tips, archive/recover, and verification events; include one duplicate pack and one same-ID/different-payload attack.
3. Exchange packs in all six order permutations, including interrupted pulls.
4. Rebuild projections from journals.

**Pass:** honest replicas converge to identical journal-set/projection digests independent of order; the attack is quarantined; no canonical branch moves.

### DR-07 — privacy and encrypted recovery

1. Seed descriptions, token-like strings, credential URL, cwd, environment, and a normal public label.
2. Create default backup, default freeze, default sync pack, and encrypted secret-inclusive backup.
3. Inspect extracted artifacts with a byte scanner.
4. Restore encrypted backup with correct identity and attempt with wrong/no identity.

**Pass:** default freeze/sync omit local/secret data; default backup follows declared local policy and warns; encrypted artifact restores exactly only for intended recipient; manifests disclose classifications without values.

### DR-08 — removal and return

1. Create states reachable only from `refs/jjk/*` plus ordinary branches/worktrees.
2. Preview removal and confirm it blocks orphaning.
3. select freeze/archive-ref preservation, execute removal, and prove plain Git build/status/branch/worktree operation.
4. restore from quarantine/backup into a new target and compare graph/ref digests.

**Pass:** no project or ordinary Git change, all selected JJK-only history recoverable.

## 18. Acceptance checks

| ID | Check |
|---|---|
| MB-A001 | A golden importer fixture exercises every `RepoData`, `StateRecord`, metadata, lane, return, navigation, timeshift, freeze, history snapshot, and legacy backup field listed above. |
| MB-A002 | Importing the same bytes 100 times yields one receipt, stable ID mappings, identical journal/projection digests, and no ref movement after the first commit. |
| MB-A003 | Kill-point tests cover every migration/backup/restore external-effect boundary and prove resume or repair without source loss. |
| MB-A004 | Original legacy file and ref hashes remain unchanged after successful migration and every failed migration. |
| MB-A005 | Full backup manifest lists a SHA-256 and byte length for every artifact; corruption of any byte blocks load. |
| MB-A006 | Backup restore reproduces metadata graph, Git refs/OIDs, active branch/detached HEAD, index, tracked dirty content, untracked content, and recoverable operation state. |
| MB-A007 | Freeze import into an unrelated clone materializes every selected state and provenance edge, but does not move canonical branches or leak excluded local fields. |
| MB-A008 | Metadata sync is order-independent, idempotent, privacy-filtered, and same-ID/different-content fail-closed. |
| MB-A009 | Supported N↔N−1 application rollback matrix passes; unsupported rollback is read-only and preserves unknown facts. |
| MB-A010 | Removal cannot orphan a Git object without explicit policy and remains recoverable through quarantine/verified backup. |
| MB-A011 | DR-01 through DR-08 run in automation, and a scheduled human-readable restore drill records artifact ID, operator/tool version, duration, and proof digest. |
| MB-A012 | No operation claims success until artifact-from-destination verification and cross-layer transaction commit both complete. |

## 19. Explicit non-goals

- Replacing Git object transport with the metadata database.
- Treating a Git remote as the sole backup.
- Permanently dual-writing legacy JSON and SQLite.
- Automatically uploading backups, freezes, metadata, environment, or workspace content.
- Syncing terminal environment/timeshift secrets by default.
- Making an old binary understand incompatible new semantics through lossy conversion.
- In-place destructive down-migration.
- Restoring over live state by default or “testing” a backup by deleting its source.
- Executing hooks, filters, commands, or environment captured in an artifact.
- Garbage-collecting legacy migration evidence, backup history, quarantine, or JJK-only Git objects without a separate retention policy and explicit reachability proof.
- Solving remote replication consensus; v0.1 provides immutable per-replica streams and deterministic convergence, not centralized leader election.

# Cross-Resource Transactions

**Status:** decision-grade architecture for JJK v0.1  
**Scope:** SQLite, Git objects/refs/index/worktrees, optional Jujutsu, and filesystem artifacts  
**Normative terms:** MUST, MUST NOT, SHOULD, MAY follow RFC 2119.

## Context

A JJK mutation spans independently durable systems. SQLite owns the append-only JJK event journal and derived projections. Git owns objects, refs, `HEAD`, indexes, worktrees, and interoperability. JJ may own change identities and an operation log. The filesystem owns recovery artifacts and user bytes. None can atomically commit the others; JJK MUST NOT claim distributed ACID.

JJK uses a **durably prepared, fingerprinted saga**. It records exact intent before external effects, applies idempotent effects under compare-and-swap (CAS) preconditions, records observations, independently verifies all authorities, then publishes semantic facts. Recovery inspects reality and deterministically completes forward, compensates only when doing so cannot overwrite another actor, or stops in a durable repair/conflict state.

Every mutation follows:

`discover → lock → reconcile → resolve → plan → durable prepare → mutate Git/JJ/files → append events+projections → verify → commit/repair`

Git remains the universal substrate and MUST remain valid without JJK. JJ is optional. The JJK event journal is semantic authority; projections are rebuildable views. The old mutable `.jjk/repo.json` and JSON snapshots do not provide adequate crash or concurrent-writer boundaries.

## Decisions

### TX-D001 — Prepared saga, not cross-system transaction theater

Each JJK-native or Git-enhanced mutation has one typed `OperationId` and immutable `PreparedPlan`: request hash, resource footprint, ordered effects, preconditions, expected postconditions, recovery artifacts, conflict policy, and verification policy. No SQLite transaction remains open while Git, JJ, a hook, editor, pager, credential helper, or filesystem mutation runs.

The semantic linearization point is the SQLite transaction that appends verified domain events plus `OperationCommitted` and advances all affected committed projections. External effects can become visible before that point. Readers may serve the last committed graph plus typed `transitioning` operation state, but MUST NOT invent a half-old/half-new current state.

A Git-valid result is never reversed merely because SQLite completion is uncertain. JJK proves external reality, then completes semantic history forward.

### TX-D002 — One control root per Git common directory

```text
<git-common-dir>/jjk/
├── state.sqlite3
├── locks/
│   ├── lifecycle.lock
│   ├── repository.lock
│   └── worktrees/<worktree-id>.lock
├── recovery/<operation-id>/
│   ├── manifest.cbor
│   ├── receipts/<effect-ordinal>.cbor
│   ├── indexes/
│   └── blobs/<sha256>
└── worktrees/<worktree-id>/
```

All linked worktrees share `state.sqlite3`. Runtime `-wal`/`-shm` files are never copied as a backup; use SQLite online backup or `VACUUM INTO`. WAL plus `synchronous=FULL` is allowed only where locking/shared-memory/durability probes pass. Otherwise use proven rollback-journal exclusive mode or refuse mutation.

Recovery files use temp-file → file `fsync` → atomic rename → parent-directory `fsync`; directories/files are private. Manifests use relative platform-byte-preserving paths and hashes, not ambient absolute paths.

### TX-D003 — Command classes are explicit

| Class | Contract |
|---|---|
| **JJK-native** | Full protocol, prepare, effects, verification, semantic events: capture/save, return, fork, pick, promotion, archive/recover, undo/redo, restore. |
| **Git-enhanced** | Explicit safety/semantic wrapper that may lock, prepare, invoke Git, reconcile, and emit JJK facts. It is not transparent. |
| **Transparent Git passthrough** | Preserves raw argv bytes/order, cwd, stdin/stdout/stderr/TTY, environment, signals, and exit code. It does not rewrite output, require/read JJK metadata, take a long JJK lock, or claim a transaction. Later reconciliation observes Git facts. |

Transparent passthrough MUST work if SQLite is missing, locked, too new, or corrupt. Immediate post-observation is Git-enhanced behavior, never hidden passthrough behavior.

### TX-D004 — Isolation prevents overwriting external work

Arbitrary editors and native Git do not honor JJK locks, and portable filesystems lack a universal atomic conditional file replacement. Therefore:

- capture builds through an operation-private `GIT_INDEX_FILE` and does not write the user's index or worktree;
- return, pick, composition, replay, and conflict-producing operations materialize in a JJK-owned linked worktree by default;
- shell integration returns the destination path; a child never claims it changed its parent's cwd;
- user-owned worktree replacement requires an explicit cooperative lease plus a fresh exact fingerprint; v0.1 refuses unleased in-place replacement;
- destinations must be absent and atomically reserved; existing directories are never cleared/reused;
- abort never deletes a worktree containing human edits.

Concurrent editor writes during capture are detected by pre/post hashes. A torn candidate is never committed as verified; user bytes are never overwritten.

### TX-D005 — Git reachability survives DB uncertainty

Before moving a ref away from a commit, create CAS-guarded retention refs:

```text
refs/jjk/recovery/<operation-uuid>/pre/<ordinal>
refs/jjk/recovery/<operation-uuid>/post/<ordinal>
```

`pre` retains displaced targets. New result objects receive a `post` ref before public refs advance. Recovery refs are retention/evidence, not state identity. Nonterminal refs have no TTL. Terminal pruning is a separate idempotent operation and uses CAS after proving another declared root or retention-policy eligibility.

If SQLite is unreadable or completion indeterminate, retain valid Git objects/refs/index/worktrees. Direct Git remains usable and can recover from these refs.

### TX-D006 — Unverified semantic mutations stay out of committed views

After external mutation, `EffectObserved` and substrate evidence update operation/audit projections. Domain events such as `StateCaptured`, `DeltaApplied`, `StateActivated`, and `CanonicalPromoted` append only after verification, in the same SQLite transaction as `OperationCommitted` and committed graph projection updates.

A projection may authorize planning only when its watermark/hash equals the journal head used by the planner. Reducer failure exposes neither domain events nor projection changes.

### TX-D007 — Evidence decides recovery

Each planned effect is classified:

```rust
enum EffectClassification {
    NotApplied,
    AppliedExactly { receipt: Option<EffectReceipt> },
    AppliedThenAdvanced { proof: AdvancementProof },
    ConflictPaused { conflict: ConflictSnapshot },
    Diverged { expected_pre: Hash256, expected_post: Hash256, actual: Hash256 },
    Uninspectable { authority: AuthorityKind, error: TypedError },
}
```

`NotApplied`, `AppliedExactly`, `AppliedThenAdvanced`, and exact declared conflict support automatic progress. `Diverged` and `Uninspectable` become `repair_required` without mutating the disputed resource. Similar-looking state is not evidence.

### TX-D008 — Optional JJ cannot weaken Git survivability

Prepare records whether JJ is absent, advisory, or required and captures JJ operation/workspace/change/commit IDs and relevant bookmarks. Advisory JJ failure never rolls back a valid Git result; record degradation and reconcile JJ before the next JJ-dependent command. A missing JJ-only capability fails before prepare.

`jj op restore` is compensation only when the current JJ operation is the exact operation-created descendant, no external descendant exists, and related Git/workspace compensation is independently safe. Otherwise retain Git and reconcile forward or stop for repair.

## Invariants

| ID | Invariant |
|---|---|
| `TX-I001` | One `OperationId` maps to one request hash. Different reuse fails before mutation. |
| `TX-I002` | `OperationPrepared` and its fsynced recovery manifest precede every external effect. |
| `TX-I003` | No SQLite transaction/mutex spans an adapter or arbitrary child invocation. |
| `TX-I004` | Every effect has stable `(operation_id, ordinal)`, exact footprint, pre/post fingerprint, and recovery rule. |
| `TX-I005` | Every ref update is CAS; related refs use a native Git ref transaction when supported. Unexpected ref movement is never forced back. |
| `TX-I006` | Compensation never deletes Git objects and changes only proven JJK-owned refs/files by CAS. |
| `TX-I007` | Index/worktree/files are restored only when live state still equals the operation's exact post-state. Later external bytes win preservation. |
| `TX-I008` | File replacement occurs in JJK-owned worktrees unless an explicit cooperative lease exists. |
| `TX-I009` | Git stays valid/directly usable when SQLite completion is indeterminate; ordinary Git never depends on JJK availability. |
| `TX-I010` | Journal append and affected projections share one SQLite transaction; committed graph views contain only committed operations. |
| `TX-I011` | Fingerprints are versioned observations with provenance, not substitute authorities. |
| `TX-I012` | Recovery is idempotent: same terminal status or same unresolved conflict, never duplicate domain events. |
| `TX-I013` | Locks follow one global order, release in reverse, and are not held across human input. |
| `TX-I014` | Conflict continuation consumes only reserved worktree/planned paths; unrelated edits remain dirty and excluded. |
| `TX-I015` | `committed` requires independent verification of every postcondition; exit code zero is not proof. |
| `TX-I016` | Suspended conflicts reserve their footprint while disjoint operations proceed; overlap names the owner/continuation. |
| `TX-I017` | Clocks do not order recovery. Journal sequence/hash, OIDs, reflog order, JJ ancestry, ordinals, and receipts do. |
| `TX-I018` | Nonterminal recovery artifacts have no TTL; cleanup is a separate CAS-guarded operation. |

## Data and API shapes

UUIDv7 IDs are typed 16-byte newtypes. Public forms use prefixes (`op_`, `evt_`, `st_`, `at_`, `br_`, `ws_`). Effects are operation-scoped: `op_<uuid>#0003`.

### Operation state machine

```rust
struct OperationRecord {
    operation_id: OperationId,
    repo_id: RepoId,
    request_hash: Hash256,
    command_class: CommandClass,
    command_kind: CommandKind,
    actor: ActorRef,
    status: OperationStatus,
    plan: PreparedPlan,
    recovery_manifest: RecoveryManifestRef,
    prepared_event_id: EventId,
    terminal_event_id: Option<EventId>,
}

enum OperationStatus {
    Prepared,
    Applying,
    AwaitingResolution,
    Verifying,
    Committed,
    Aborting,
    Aborted,
    RepairRequired,
}
```

Exact storage spellings:

```text
prepared | applying | awaiting_resolution | verifying |
committed | aborting | aborted | repair_required
```

```text
(no row) ─OperationPrepared─> prepared
prepared ─ApplyStarted─> applying
prepared ─AbortStarted─> aborting
applying ─ConflictPaused─> awaiting_resolution
applying ─VerificationStarted─> verifying
applying ─AbortStarted─> aborting
applying ─RepairRequired─> repair_required
awaiting_resolution ─RepairResumed(forward)─> applying
awaiting_resolution ─AbortStarted─> aborting
verifying ─OperationCommitted─> committed
verifying ─RepairRequired─> repair_required
aborting ─OperationAborted─> aborted
aborting ─RepairRequired─> repair_required
repair_required ─RepairResumed(forward|verify|rollback)─> applying|verifying|aborting
```

`committed` and `aborted` are terminal. Reversal/reconciliation is a new operation. Lifecycle transitions are event-backed and atomically projected; direct SQL status edits are forbidden.

### Prepared plan

```rust
struct PreparedPlan {
    plan_version: u16,
    capability_snapshot: CapabilitySnapshot,
    pre: CrossLayerFingerprint,
    resolved_targets: Vec<TypedTarget>,
    resource_footprint: BTreeSet<ResourceKey>,
    effects: Vec<EffectSpec>,
    semantic_postconditions: Vec<Postcondition>,
    verification_spec: VerificationSpec,
    conflict_policy: ConflictPolicy,
    recovery_policy: RecoveryPolicy,
}

struct EffectSpec {
    ordinal: u32,
    resource: ResourceKey,
    kind: EffectKind,
    depends_on: Vec<u32>,
    precondition: ResourceFingerprint,
    expected_postcondition: ResourceFingerprint,
    payload_hash: Hash256,
    exposure: Exposure,                 // Internal | RecoveryRef | PublicRef | NewWorktree
    compensation: CompensationSpec,     // None | CasOnly(...)
}

enum ResourceKey {
    GitObject(GitObjectId), GitRef(GitRefNameBytes), GitHead(WorktreeId),
    GitIndex(WorktreeId), WorktreePath(WorktreeId, PlatformPathBytes),
    JjStore(RepoId), JjWorkspace(WorktreeId), InternalPath(PortableRelativePath),
    Journal(RepoId),
}
```

Effect ordinals are contiguous/immutable after prepare. Recovery cannot silently amend a plan. New intent creates a new operation.

### Cross-layer fingerprints

```rust
struct CrossLayerFingerprint {
    journal: JournalFingerprint,
    git: GitFingerprint,
    jj: JjFingerprint,                  // Disabled | Enabled(...)
    worktrees: BTreeMap<WorktreeId, WorktreeFingerprint>,
    internal_files: BTreeMap<PortableRelativePath, FileFingerprint>,
    digest: Hash256,
}

struct JournalFingerprint {
    journal_generation: u64,
    storage_schema_version: u32,
    committed_seq: u64,
    committed_event_hash: Hash256,
    projection_watermarks: BTreeMap<ProjectionName, (u64, Hash256)>,
}

struct GitFingerprint {
    common_dir_token: FileIdentity,
    object_format: GitObjectFormat,     // SHA-1 or SHA-256
    head: HeadFingerprint,              // symbolic ref+OID | detached | unborn
    relevant_refs: BTreeMap<GitRefNameBytes, Option<GitObjectId>>,
    refs_digest: Hash256,
}

struct WorktreeFingerprint {
    worktree_id: WorktreeId,
    git_dir_token: FileIdentity,
    root_token: FileIdentity,
    head: HeadFingerprint,
    index: IndexFingerprint,
    workspace: WorkspaceFingerprint,
    sparse_checkout_digest: Option<Hash256>,
    in_progress_git_state: Option<GitInProgressState>,
}

struct IndexFingerprint {
    raw_bytes_sha256: Hash256,
    byte_length: u64,
    file_identity: FileIdentity,
    staged_entries_digest: Hash256,     // stage/mode/OID/raw-path tuples
    write_tree_oid: Option<GitObjectId>,
}

struct WorkspaceFingerprint {
    porcelain_v2_z_sha256: Hash256,
    affected_paths: BTreeMap<PlatformPathBytes, FileFingerprint>,
    protected_dirty_paths: BTreeMap<PlatformPathBytes, FileFingerprint>,
    untracked_manifest_sha256: Hash256,
    ignored_manifest_sha256: Option<Hash256>,
}

struct JjEnabledFingerprint {
    store_token: FileIdentity,
    operation_id: JjOperationId,
    workspace_id: JjWorkspaceId,
    working_copy_commit_id: JjCommitId,
    change_id: JjChangeId,
    relevant_bookmarks_digest: Hash256,
}
```

Fingerprints are canonical/versioned, not status prose. Ref/path bytes are preserved and escaped only for display. Hash every path an operation may alter and each dirty/untracked path it promises to protect. Unrestorable special files, ACLs, xattrs, or metadata make destructive operations refuse before prepare. Broad fingerprints trigger reconciliation; narrow effect fingerprints alone authorize that effect.

### Receipt and adapter seam

```rust
struct EffectReceipt {
    effect: EffectKey,
    adapter: AdapterIdentity,
    started_from: ResourceFingerprint,
    command: SanitizedCommandEvidence,
    termination: AdapterTermination,
    observed_post: ResourceFingerprint,
    durable_evidence: Vec<EvidenceRef>,
    receipt_hash: Hash256,
}

enum RecoveryAction {
    CompleteForward,
    VerifyThenCommit,
    CompensateByCas,
    AwaitResolution,
    StopForRepair,
}

trait TransactionalAdapter {
    fn discover(&self, ctx: &DiscoveryContext) -> Result<CapabilitySnapshot>;
    fn observe(&self, scope: &ResourceFootprint) -> Result<ResourceFingerprint>;
    fn prepare(&self, effect: &EffectSpec) -> Result<PreparedAdapterStep>;
    fn apply(&self, prepared: &PreparedAdapterStep) -> Result<ApplyReceipt>;
    fn verify(&self, effect: &EffectSpec, receipt: Option<&ApplyReceipt>) -> VerificationReport;
    fn repair(&self, effect: &EffectSpec, observed: &ResourceFingerprint) -> RepairOutcome;
}
```

`apply` is idempotent by `EffectKey`; retry observes first and never equates invocation with effect.

### Conflict continuation

```rust
struct ConflictSnapshot {
    operation_id: OperationId,
    worktree_id: WorktreeId,
    reserved_resources: BTreeSet<ResourceKey>,
    base_head: HeadFingerprint,
    conflict_index: IndexFingerprint,
    unmerged_entries: Vec<UnmergedEntry>,
    planned_paths: BTreeSet<PlatformPathBytes>,
    workspace_digest_at_pause: Hash256,
    recovery_manifest: RecoveryManifestRef,
    continuation_nonce_hash: Hash256,
}
```

A nonce prevents automation from accepting unreviewed human bytes. Machine-readable pause output carries it; only its hash is stored. Interactive continuation retrieves it from JJK-owned worktree administration, never user files.

## Lock ordering and concurrency

Acquire in this order; release in reverse:

1. **Lifecycle gate** (`lifecycle.lock`): normal commands shared; migration, restore, generation replacement, destructive repair exclusive.
2. **Repository writer** (`repository.lock`): one JJK-native/Git-enhanced coordinator per Git common directory.
3. **Worktree leases**, sorted by raw `WorktreeId` bytes.
4. **Short SQLite write transaction**: `BEGIN IMMEDIATE`, append/reduce/check/commit, release before adapters.
5. **Native Git/JJ locks**, only within one adapter call while no SQLite transaction is open.

SQLite and native locks are never nested. Descriptors are close-on-exec. A mutating JJK call recursively invoked by a Git-enhanced hook fails immediately with `ReentrantMutation { operation_id }`; it never deadlocks. Read-only queries can return last committed projections plus progress.

OS locks, not PID files, are authoritative. Owner/PID/process-start metadata is diagnostic. Untrustworthy advisory-lock filesystems refuse shared-store mutation.

Native tools ignore JJK locks, so every effect repeats its narrow precondition immediately before mutation and every ref change uses native CAS. Locks prevent cooperative collisions; fingerprints/CAS prevent clobbering non-cooperative writers.

`awaiting_resolution` releases OS locks but keeps a durable logical lease over its footprint. Disjoint operations proceed; overlapping plans fail before prepare naming the owner. Crashed nonterminal operations repair before new overlapping mutations. WAL readers remain available.

## Exact operation sequence

```mermaid
sequenceDiagram
    participant C as Caller
    participant O as Coordinator
    participant D as SQLite
    participant A as Git/JJ/files
    participant R as Recovery artifacts
    C->>O: typed request + OperationId
    O->>A: discover/fingerprint
    O->>O: acquire ordered locks
    O->>D: reconcile observations (short tx)
    O->>O: resolve and plan
    O->>R: write + fsync bundle
    O->>D: OperationPrepared (commit)
    O->>D: ApplyStarted (commit)
    loop stable effect ordinals
        O->>A: recheck precondition
        O->>A: CAS/apply and observe post
        O->>R: persist receipt
        O->>D: EffectObserved (commit)
    end
    O->>D: VerificationStarted (commit)
    O->>A: independently verify authorities
    alt verified
        O->>D: domain events + OperationCommitted + projections (one tx)
    else declared conflict
        O->>D: ConflictPaused / awaiting_resolution
    else divergence
        O->>D: RepairRequired
    end
    O-->>C: exact result/evidence/continuation; unlock
```

| Step | Required action | Boundary |
|---:|---|---|
| 1 Discover | Resolve common-dir/worktree identity, object format, generation, adapters, filesystem/lock/durability. | Unsupported capability refuses without mutation. |
| 2 Lock | Acquire global order; rediscover identity. | Changed identity restarts. |
| 3 Reconcile | Compare authorities with committed observations; append idempotent facts; inspect nonterminal ops. | Ambiguity/repair blocks new intent. |
| 4 Resolve | Convert query to typed IDs with explicit confidence. | Automation never guesses. |
| 5 Plan | Freeze request hash, footprint, effects, fingerprints, recovery/verification. | Required approval precedes prepare. |
| 6 Durable prepare | Fsync artifacts; atomically append/project `OperationPrepared`. | Indeterminate commit is proved on a fresh connection before effects. |
| 7 Mutate | `ApplyStarted`; for each effect observe-pre, CAS/apply, observe-post, persist receipt, `EffectObserved`. | Mismatch stops before disputed resource mutation. |
| 8 Append | Operational receipts/observations use short SQLite transactions. | No committed graph publication yet. |
| 9 Verify | `VerificationStarted`; inspect refs, objects, index/worktree, JJ, files, journal/graph. | Exit status alone insufficient. |
| 10 Commit/repair | Domain events + `OperationCommitted` + projections atomically, or conflict/abort/repair status. | Unlock only after durable status; cleanup separate. |

## Crash windows and deterministic recovery

| Window | Possible durable state | Deterministic recovery |
|---|---|---|
| `CW-00` before lock | No operation/effect; independent external changes possible. | Discover/reconcile; retry. |
| `CW-01` after lock, before reconcile commit | Kernel releases lock; no new intent. | Reacquire/reconcile; ignore stale diagnostic metadata. |
| `CW-02` reconcile DB transaction | All or none. | Check watermark/hash; rerun idempotently. |
| `CW-03` after reconcile, before prepare | Observation facts only. | Keep facts; replan. |
| `CW-04` recovery bundle write | Temp/unreferenced blobs, no prepare/effect. | Delete only ownership-nonce temp; reuse verified blobs or later GC. |
| `CW-05` prepare commit indeterminate | Prepare may have committed despite error. | Discard connection; fresh-open by operation ID. Absent forbids effects; present resumes. |
| `CW-06` prepared before apply | Plan/bundle durable; effects pre. | Complete forward, or explicit cancellation aborts without touching user resources. |
| `CW-07` applying before first effect | External state pre. | Classify not-applied; continue. |
| `CW-08` inside Git ref transaction | All-pre or all-post; receipt may be absent. | Inspect exact refs. Pre retry; post record; mixed preserve/repair, never force. |
| `CW-09` object created before retention | Object may be unreferenced. | Recompute/verify OID, create post recovery ref, continue; unproven object left to Git GC. |
| `CW-10` retained before public CAS | Result retained; public ref pre. | CAS publish if still pre; else preserve both/repair. |
| `CW-11` public CAS before DB receipt | Git exact post; DB lags. | Append receipt and complete forward; never roll Git back to stale DB. |
| `CW-12` index replacement | Complete old/new index; stale lock possible. | Prove ownership death; old retry, new record, other preserve/repair. Never delete live/unknown lock. |
| `CW-13` worktree creation | Destination absent/complete/partial; registration partial. | Verify ownership nonce+registration. Complete continue; owned untouched partial recreate; unknown/user content preserve and repair/new path. |
| `CW-14` conflict before pause event | Reserved worktree may hold exact unmerged state. | Exact declared fingerprint appends pause; otherwise preserve/repair. |
| `CW-15` after pause | Human edits possible; locks released. | No automation. Explicit continue/abort preserves unrelated bytes. |
| `CW-16` effects complete, receipts partial | Live post/recovery refs; DB prefix. | Classify every effect; exact/proven advanced appends missing receipts then verifies; divergence repairs. |
| `CW-17` receipt DB transaction | All or none. | Deduplicate by operation+ordinal+receipt hash; re-observe if absent. |
| `CW-18` before/during verify | Effects/receipts durable; no semantic commit. | Reverify from authorities. |
| `CW-19` verified in memory | Success not durable. | Reverify; retry terminal transaction. |
| `CW-20` terminal DB transaction | Prior state or entire event/projection/commit set. | Fresh-open status/hash. Committed returns result; otherwise reverify/retry same terminal tx. |
| `CW-21` committed before unlock/output | Terminal; caller may miss success. | Same operation ID returns recorded result without reapply. |
| `CW-22` cleanup | Extra temp/recovery roots. | Separate idempotent CAS maintenance; committed result unchanged. |
| `CW-23` DB unreadable after Git mutation | Git/recovery/index/worktrees valid; semantics unknown. | Stop JJK mutation, retain Git, expose direct-Git recovery, restore DB into new generation, reconcile. Never reset Git to projection. |
| `CW-24` JJ fails after Git success | Git valid; JJ partial. | Advisory: keep Git/reconcile JJ forward. Required JJ semantics: repair; no Git rollback for parity alone. |
| `CW-25` power loss after reported success | Authority may reopen pre/post. | Initial durability probe/fsync contract applies; classify live pre/post. Unsupported filesystem was refused. |

Every external effect is bounded by a durable precondition and independently observable postcondition. Fault injection targets every boundary, including `SIGKILL`, `ENOSPC`, fsync failure, and commit-indeterminate errors.

## Complete-forward versus rollback

Compensation is permitted only when every resource still equals the exact operation-written post-state.

| Evidence | Rule |
|---|---|
| No external effect | Complete forward; explicit cancellation may abort. |
| Internal temp/blob/immutable object only | Complete or abandon internal data; never delete Git objects as rollback. |
| Result retained, public refs pre | CAS complete forward. |
| Public ref exact post, DB incomplete | Journal/verify forward; rollback to stale DB forbidden. |
| Ref later advanced with proof through expected post | Record operation achieved; reconcile advancement as subsequent fact. |
| Ref differs with ambiguous ordering | Stop for repair; no mutation. |
| Index/worktree exact operation post and exact pre-image retained | Explicit abort MAY CAS-compensate if no dependent effect/human change. |
| Index/worktree differs from operation post | Never restore over it; preserve bytes/artifacts/refs and repair. |
| Conflict has human edits | Nonce-bearing continue; abort retains worktree. |
| JJ exact operation post, no descendant | MAY compensate only with independently safe Git compensation. |
| JJ unknown/descendant op | Reconcile forward or repair; no restore. |
| DB commit indeterminate | Fresh-open/prove; preserve Git meanwhile. |
| Recovery artifact missing/corrupt | No destructive compensation; preserve and repair. |

Compensation runs reverse dependency order and rechecks each post fingerprint immediately before CAS. One failure stops rollback and appends `RepairRequired`; dependent compensation does not continue. `OperationAborted` retains prepare/receipts/audit.

## Conflict continuation

1. Prepare declares conflict-capable effect and reserves a JJK-owned worktree.
2. Exact conflict captures unmerged stages, paths, `HEAD`, Git in-progress markers, worktree digest, and unrelated protected paths.
3. `ConflictPaused` sets `awaiting_resolution`, returns operation/path/paths/source/base/nonce and commands, then releases OS locks.
4. Disjoint work proceeds; overlapping footprint is refused.
5. JJK never background-resets, checks out, or auto-stages the conflict worktree.
6. `jjk continue op_…` reacquires locks; verifies identity, nonce, base/ref preconditions, and no unmerged entries. Planned-path bytes are resolution input; unrelated edits remain dirty/excluded.
7. An operation-private/owned index forms and retains the result before resuming `applying → verifying` under the same ID.
8. External base/source movement preserves resolution bytes and enters repair; never silently rebase/overwrite.
9. Abort removes only unchanged operation-owned administration. Dirty worktree remains recoverable; deletion is separate.

Death during continuation uses the same crash matrix; stable identities prevent duplicate result states.

## Surface rules

### SQLite

One API writes lifecycle, receipts, domain events, and projections. Operation/request hash and operation+ordinal enforce idempotency. A connection reporting commit failure is discarded and outcome proved through a fresh connection. Stale projections rebuild/fail and never authorize mutation. Migration/restore takes exclusive lifecycle lock and repairs nonterminal operations before mutation.

### Git objects/refs

Create/verify objects before refs; retain result before public publication. Single refs use expected old OID/absence; related refs use native ref transaction and verify. Symbolic, detached, and unborn `HEAD` are distinct. Automatic recovery contains no force/unconditional update/reset. Reflogs support ordering but do not replace retention refs.

### Git index

Capture uses an operation-private `GIT_INDEX_FILE`. Real-index replacement acquires the standard lock, rechecks raw digest, writes/fsyncs a complete index, and atomically renames; mismatch preserves native index. Unmerged stages are fingerprinted exactly. Remove a stale index lock only after proving dead owner and JJK-operation ownership.

### Worktrees/files

Destructive materialization uses JJK-owned worktrees and never clears existing destinations. Capture pre/post hashes detect concurrent edits without overwriting. Ignored files are untouched unless explicitly included and protected. Raw path semantics are retained. Unrestorable special files/metadata cause pre-mutation refusal. Internal publication uses atomic rename plus directory fsync.

### JJ

Record pre/post operation IDs and verify ancestry. Import/export failure cannot be ignored. JJ success does not prove Git export; Git is independently observed. Git remains usable during JJ degradation. JJ locks never span SQLite writes or human resolution.

## Failure modes

| ID | Failure and response |
|---|---|
| `TX-F001` | External ref moves: CAS fails; replan if no effect, otherwise preserve/repair; never force. |
| `TX-F002` | Editor writes during capture: preserve; torn capture cannot verify; retry fresh. |
| `TX-F003` | External edit during replacement: isolation avoids it; lease violation stops before replacement. |
| `TX-F004` | SQLite busy: bounded wait/report owner; never bypass/create second store. |
| `TX-F005` | Disk full/commit indeterminate: fresh-open proves DB; preserve external state. |
| `TX-F006` | Corrupt DB/WAL: stop JJK mutation; transparent Git remains; recover new generation/reconcile. |
| `TX-F007` | Recovery hash mismatch: no compensation; preserve/forensic repair. |
| `TX-F008` | Object succeeds/ref fails: retain result; retry CAS or abort without object deletion. |
| `TX-F009` | Mixed multi-ref result: preserve/repair; never normalize by force. |
| `TX-F010` | Index lock: remove only proven operation-owned stale lock. |
| `TX-F011` | Worktree destination user data: never delete/reuse; allocate new path. |
| `TX-F012` | Undeclared conflict: preserve isolated worktree/repair. |
| `TX-F013` | Resolution plus unrelated edits: commit planned paths only. |
| `TX-F014` | External JJ advance: no restore; forward reconcile/repair. |
| `TX-F015` | Recursive mutating hook: fail fast with parent operation ID. |
| `TX-F016` | Lock holder death: OS releases; next writer repairs first. |
| `TX-F017` | Stale projection: rebuild/typed error; never plan from stale data. |
| `TX-F018` | Retry: same hash resumes/returns; changed hash `IdempotencyConflict`. |
| `TX-F019` | Signal: pre-effect abort; later compensate only if CAS-safe, else repair. |
| `TX-F020` | Unsupported filesystem: proven fallback or refusal, never silent WAL. |
| `TX-F021` | Git valid/DB unavailable: retain Git/recovery refs; never reset to projection. |
| `TX-F022` | Reader too old: read-only diagnostics/export and transparent Git only. |

## Acceptance checks

| ID | Release-blocking check |
|---|---|
| `TX-VAL-001` | Kill/error every mutator at every `CW-*`; restart reaches verified pre, intended post, or declared preserved conflict/repair—never unexplained mixture. |
| `TX-VAL-002` | Commit-indeterminate DB faults at prepare/receipt/terminal are resolved by fresh connection without duplicate effects/events. |
| `TX-VAL-003` | Kill after public ref CAS before DB receipt; recovery journals forward, never moves ref back; Git works throughout. |
| `TX-VAL-004` | Corrupt DB after Git success; `git status/log/branch/fsck` and recovery refs remain usable; JJK preserves results/refuses mutation. |
| `TX-VAL-005` | Race native `git update-ref` against every ref effect; one CAS wins, loser never overwrites. |
| `TX-VAL-006` | 32 processes ×100 rounds across shared/sibling worktrees: no lost refs, leaked `BUSY`, stale live lock, duplicate ID/event, starvation, or projection mismatch. |
| `TX-VAL-007` | Continuously edit during capture; JJK writes no user file and commits no torn snapshot. |
| `TX-VAL-008` | Return/pick from dirty worktree materializes elsewhere; original HEAD/index/tracked/untracked/ignored bytes are identical. |
| `TX-VAL-009` | Crash every worktree-creation substep; pre-existing content is never deleted; owned partials recover deterministically. |
| `TX-VAL-010` | Conflict, crash around pause, resolve plus unrelated edits, continue: resolution commits once, unrelated edits remain dirty, source/base unchanged. |
| `TX-VAL-011` | Abort after conflict edits; no byte disappears and retained worktree is addressable. |
| `TX-VAL-012` | Advance ref/JJ after expected post; proven advancement is preserved, ambiguous ordering stops without mutation. |
| `TX-VAL-013` | Replay/rebuild after crashes; digests match and unverified semantic state never appears in committed views. |
| `TX-VAL-014` | Retry same ID at every phase/lost output: exactly-once effects/results; changed request always conflicts. |
| `TX-VAL-015` | Differential transparent passthrough preserves non-UTF-8/raw argv, cwd, env, stdio/PTY, helpers, signals, exit, and repo bytes without readable DB. |
| `TX-VAL-016` | Fingerprints distinguish SHA-1/256, unborn/detached/symbolic HEAD, all index/dirty states, sparse/linked/submodule/bare-readonly repos, raw paths, symlinks, modes, case collisions. |
| `TX-VAL-017` | WAL/fallback/full-disk/permissions/fsync/stale-lock/network-FS cases mutate only under a proven durability contract. |
| `TX-VAL-018` | Git-only, broken-advisory-JJ, and healthy-JJ produce equal Git-representable semantic outcomes with honest capability differences. |
| `TX-VAL-019` | External changes outside footprint survive/reconcile; inside-footprint changes stop JJK before overwrite. |
| `TX-VAL-020` | Crash cleanup deletes only exact JJK-owned terminal artifacts by CAS; nonterminal and semantic roots remain. |
| `TX-VAL-021` | Recursive mutating hook fails fast; read-only hook sees committed view plus progress. |
| `TX-VAL-022` | Suspended conflict allows disjoint captures, refuses overlap naming owner, and remains continuable. |

## Explicit non-goals

1. Distributed ACID across SQLite, Git, JJ, and files.
2. JJ as mandatory coordinator.
3. Making arbitrary editors/native Git honor JJK locks.
4. Silent in-place checkout/reset of user-owned worktrees.
5. Rolling Git back to stale/unreadable metadata.
6. Deleting Git objects as rollback.
7. Holding locks across prompts, editors, conflicts, network, pagers, or helpers.
8. Treating process success as cross-layer verification.
9. Hiding Git-enhanced behavior inside transparent passthrough.
10. Folding unrelated dirty work into capture, conflict resolution, pick, or promotion.
11. Remote/multi-device journal consensus in v0.1.
12. Transactional restore of arbitrary processes, terminals, editors, or secrets.
13. Treating recovery artifacts as semantic state identity.
14. Best-effort mutation on an unproven filesystem; safe refusal is valid.

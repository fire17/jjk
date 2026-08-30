# JJK v0.1 Rewrite — Operational Wargame and Pseudo-Oracle

> **Status:** pre-implementation architecture gate  
> **Date:** 2026-08-28  
> **Scope:** the v0.1 indestructible core, from initialization through repository detachment  
> **Authority:** `/Users/magic/Creations/JJK/VISION.md` and `/Users/magic/wholesomegarden/Codex/jjk_v1/vision_overhaul.md`  
> **Audience:** implementers, reviewers, migration authors, operators, and future agents arriving without prior conversation context

`MUST`, `MUST NOT`, `SHOULD`, and `MAY` are normative. Proposed command names in this document are part of the v0.1 architecture contract; they are not claims about the legacy implementation.

---

## 1. Context capsule

JJK is a semantic state and collaboration layer above Git and, when available, Jujutsu. Git remains the universal object, ref, history, and transport substrate. JJ is an optional local accelerator and recovery adapter. JJK owns intent, stable state identity, attempt topology, provenance, evidence, exact composition, navigation, promotion, and recovery.

The product promise governing every branch of this wargame is:

> A user or agent can try, branch, reject, combine, return, migrate, synchronize, and remove JJK without losing good work or making the underlying Git repository dishonest.

### 1.1 Command classes

Every public command is classified before implementation in the versioned command registry. JJK-native and Git-enhanced operations record their class durably; transparent passthrough deliberately writes no JJK record.

| ID | Class | Meaning | Examples | Mutation ownership |
|---|---|---|---|---|
| `CC-JN` | **JJK-native** | The semantic transition exists because JJK exists. | `jjk save`, `jjk return`, `jjk fork`, `jjk pick`, `jjk promote`, `jjk sync` | JJK owns the complete cross-layer transaction. |
| `CC-GE` | **Git-enhanced** | A Git operation is deliberately wrapped with JJK planning, isolation, provenance, and recovery. | planned submission refresh, protected branch promotion, external candidate import | JJK owns the plan and recovery; Git remains the mutation engine. |
| `CC-GP` | **transparent Git passthrough** | Run any top-level argv not claimed by the versioned JJK-native/enhanced registry exactly as Git, with `jjk git -- <native Git argv…>` as the explicit collision-proof form. Reconcile only on the next JJK command. | `jjk rebase …`; `jjk remote …`; `jjk git -- <native Git argv…>` | Git owns the mutation and result; JJK does not observe, lock, or write metadata in the passthrough process. |

A `CC-GP` implementation MUST accept native OS strings rather than UTF-8 strings. It MUST preserve argv bytes (Unix) or native wide strings (Windows), exact cwd, inherited environment, inherited stdin/stdout/stderr, terminal/TTY behavior, signals, and Git's exit result. If Git exits due to a signal, the wrapper MUST terminate by the same signal where the platform permits; it MUST NOT convert that result into a fabricated success or a JJK-specific exit code. JJK MUST NOT log full environment values because they may contain secrets.

### 1.2 Cross-layer mutation protocol

`TX-01` is the only mutation protocol:

```text
discover
→ lock
→ reconcile
→ resolve
→ plan
→ durable prepare
→ mutate Git/JJ/files
→ append events+projections
→ verify
→ commit/repair
```

No mutating JJK-native or Git-enhanced command may skip a phase. A phase may be a proven no-op, but the operation record must say why. `commit` means the intended transition is observed across all affected layers and the operation is marked committed. `repair` means the command did not prove that transition; recovery material remains pinned and the operation is resumable or abortable.

Transparent Git passthrough is the deliberate boundary case. It MUST NOT hold the JJK repository lock while the child Git process runs, because Git may block for credentials, open an editor, invoke hooks, or recursively invoke JJK. Its protocol is:

```text
short discover + pre-observation
→ release all JJK locks
→ exec/spawn Git transparently
→ preserve Git's result
→ best-effort short post-observation
→ next JJK-native command performs authoritative TX-01 reconciliation
```

The post-observation MUST NOT change the Git exit result or inject output into inherited stdio. If it cannot complete, it records a local `ReconcilePending` marker without user-visible output; the next JJK-native operation repairs it before resolving targets.

### 1.3 Architecture under test

The wargame assumes this v0.1 shape:

1. One Rust binary with a reusable library core.
2. A repository-wide metadata root resolved from `git rev-parse --git-common-dir`, never by assuming `.git` is a directory.
3. A local SQLite database containing the authoritative logical event journal, durable operation records, and rebuildable materialized projections.
4. SQLite WAL mode only after a runtime capability probe proves a local coherent filesystem and WAL support. SQLite's own WAL contract permits concurrent readers but one writer, requires same-host shared memory, and does not support network filesystems. WAL therefore improves local concurrency but does not solve distributed locking or cross-layer Git/SQLite atomicity.
5. An outer repository mutation lock serializing JJK cross-layer writers. SQLite locking alone is insufficient because Git refs, worktrees, files, and optional JJ state are outside the database.
6. Git objects and namespaced refs preserving snapshots and attempt reachability. Ordinary branches remain standard Git branches.
7. Optional JJ adapter state recorded as capability evidence, never silently assumed.
8. Optional remote metadata synchronization by immutable event segments over namespaced Git refs, never by sharing or copying a live SQLite WAL database.

### 1.4 SQLite WAL challenge and resolution

`DEC-STORAGE-01` challenges the default instead of treating “SQLite WAL” as universally safe.

- **Evidence:** WAL permits readers alongside a writer but still has only one writer. It relies on shared memory and is unsupported across network filesystems. Checkpoint starvation and WAL growth are possible under long readers. A SQLite transaction cannot atomically commit Git refs, filesystem bytes, JJ operations, and database rows.
- **Decision:** use WAL as the default local database mode only after a two-connection/two-process capability probe in the actual metadata directory. Require `journal_mode` to report `wal`, use `synchronous=FULL` for operation-boundary durability, set a bounded busy timeout, bound read transactions, and expose WAL/checkpoint health in `jjk doctor`.
- **Unsafe filesystem behavior:** if locality or lock coherence cannot be proven, v0.1 JJK mutations fail closed with `JJK-E-STORAGE-UNSAFE`. Git remains usable, including transparent passthrough. Do not silently fall back to a mode with weaker, untested guarantees. A rollback-journal fallback may be added only after the same crash and multi-process conformance suite passes on that filesystem class.
- **Remote behavior:** synchronize immutable logical event segments, not `state.sqlite3`, `-wal`, or `-shm` files.
- **Revisit trigger:** a daemon/server architecture becomes the sole writer, or a tested storage backend provides equivalent local durability plus remote coordination.

---

## 2. Decision records

### `ADR-WG-001` — Git truth and JJK meaning remain separate

- **Context:** Git may change outside JJK; JJK may create semantic states without needing a new ordinary branch commit.
- **Decision:** Git OIDs/refs/index/worktree are observed facts. JJK events are semantic facts. Reconciliation links them; it never overwrites either to make a mismatch disappear.
- **Rejected:** treat JJK metadata as the only truth; encode all semantics solely in Git commit messages.
- **Consequences:** every state carries explicit Git identities and observation evidence. External rewrites create new observations and possible orphan status rather than retroactively changing old events.
- **Revisit only if:** Git ceases to be the universal compatibility substrate, which is outside v0.1.

### `ADR-WG-002` — Durable prepare plus repairable saga, not pretend atomicity

- **Context:** SQLite cannot participate in one atomic transaction with Git refs, worktree bytes, and JJ.
- **Decision:** persist an operation plan and recovery boundary before mutation, apply idempotent steps, then atomically append JJK events and update projections in one SQLite transaction. Verify all layers before committing the operation. Any interrupted boundary becomes `RepairRequired`.
- **Rejected:** append the event first and hope Git succeeds; mutate Git first without a recovery record; claim two-phase commit without a real transaction manager.
- **Consequences:** fault injection is mandatory at every TX-01 boundary. Recovery storage remains pinned until commit or verified abort.
- **Revisit only if:** all mutation participants support a real shared transaction protocol.

### `ADR-WG-003` — Logical append-only events; projections are disposable

- **Context:** current JSON state rewrites whole files and mixes authority with cached views.
- **Decision:** event payloads are immutable. Corrections append superseding events. State/attempt/tag/navigation/story views are materialized projections with a recorded `last_event_seq` and deterministic rebuild.
- **Rejected:** mutable state rows as history; projection-only storage; filesystem log plus unrelated SQLite cache.
- **Consequences:** migration must preserve legacy evidence and produce deterministic events. A projection checksum mismatch triggers rebuild, not event mutation.
- **Revisit only if:** event replay cannot meet measured startup/read budgets after snapshotting and compaction are exhausted.

### `ADR-WG-004` — Stable identities are opaque and never inferred from labels

- **Context:** labels, branches, worktree paths, and Git OIDs can change or collide.
- **Decision:** repository, operation, event, state, attempt, conflict, promotion, migration, sync-segment, actor, and worktree IDs are separate typed UUIDv7 values. Short prefixes are display conveniences only and must resolve uniquely.
- **Rejected:** eight-character random IDs as authoritative; branch names as attempt IDs; Git OIDs as JJK state IDs.
- **Consequences:** target resolution can fail as ambiguous without corrupting the graph.
- **Revisit only if:** an equally collision-resistant typed identity scheme improves offline merge behavior.

### `ADR-WG-005` — Historical return preserves both old and future topology

- **Context:** returning to the past must restore exact bytes without deleting later work or eagerly manufacturing branches.
- **Decision:** returning to a non-tip state activates that exact snapshot in a detached/JJK-managed historical context. The prior future remains reachable. A sibling attempt/branch is created only when the next state-producing mutation diverges.
- **Rejected:** hard reset; force-moving the source branch; creating a branch on every navigation action.
- **Consequences:** navigation context and pending divergence are explicit projection state.
- **Revisit only if:** user research proves eager branches are clearer without increasing cleanup debt.

### `ADR-WG-006` — Concurrent agents get distinct attempts and worktrees

- **Context:** a shared SQLite journal does not make shared worktree bytes safe.
- **Decision:** each concurrent writer receives one attempt ID, one Git branch/ref, one registered worktree ID, and one filesystem worktree. Repository journal writes serialize briefly; source edits do not.
- **Rejected:** multiple agents in one worktree; one branch checked out in multiple mutable worktrees; an advisory owner field as the only collision guard.
- **Consequences:** worktree registration and lease/owner data are operational evidence, not correctness primitives.
- **Revisit only if:** a future virtual working-copy engine proves equivalent isolation.

### `ADR-WG-007` — Exact pick is parent-to-state content effect

- **Context:** “take only fast mode” must not import purple or earlier unrelated changes.
- **Decision:** the pick source is `(source_parent_tree, source_state_tree)`, using the state's recorded logical parent. Apply that exact content delta to the target base in an isolated operation worktree. Record source, parent, both tree OIDs, patch identity, target base, conflict decisions, and result tree.
- **Rejected:** cherry-pick the entire source branch; diff from merge-base; infer parent from current Git adjacency; AI-generated patch on the deterministic path.
- **Consequences:** source states without a valid, reachable logical parent are not pickable until provenance is repaired.
- **Revisit only if:** a future explicit multi-parent delta type is added; it must not weaken single-parent semantics.

### `ADR-WG-008` — Conflicts are durable states, never half-applied user worktrees

- **Context:** exact picks, refreshes, and promotions may conflict.
- **Decision:** risky composition occurs in an isolated operation worktree/temp index. A conflict creates a typed `ConflictRecord` and pauses the operation. The target ref/worktree remains at its pre-operation fingerprint until `continue` verifies a resolved result and performs a CAS publish.
- **Rejected:** leave conflict markers in the user's active target; auto-select ours/theirs; mark a conflicted operation successful.
- **Consequences:** abort is cheap and deterministic; resolution provenance is complete.
- **Revisit only if:** an operation is explicitly requested to edit the current worktree interactively and its preview says so.

### `ADR-WG-009` — Promotion is evidence-gated compare-and-swap

- **Context:** canonical branches represent chosen truth and may move externally while validation runs.
- **Decision:** promotion records source state, policy version, evidence IDs and content hashes, expected canonical tip, previous tip, result OID, approver, and rollback ref. Publish uses Git ref compare-and-swap only after re-verifying policy and target occupancy.
- **Rejected:** force update; validation by unstructured “tests passed” text; updating a checked-out dirty canonical worktree behind its back.
- **Consequences:** stale promotions fail safely and can be replanned against the new tip.
- **Revisit only if:** repository policy explicitly selects another atomic forge-side mechanism with equivalent rollback evidence.

### `ADR-WG-010` — Remote sync exchanges immutable segments, not a database

- **Context:** WAL is local-only; remotes can be untrusted, concurrently updated, unavailable, or used by clients with different schemas.
- **Decision:** package bounded, hash-chained event segments and referenced manifests as Git objects; advertise device heads under namespaced `refs/jjk/sync/<repo-id>/<device-id>`. Fetch to quarantine, validate schema/hash/repository identity, then import idempotently. Unknown event types remain preserved but inactive.
- **Rejected:** rsync/copy SQLite files; put mutable metadata on the user branch; last-writer-wins labels; execute remote-provided commands/hooks.
- **Consequences:** metadata transport is optional and backward-compatible. Same event ID with a different payload is an integrity incident.
- **Revisit only if:** a dedicated authenticated sync service provides stronger semantics while retaining offline export/import.

### `ADR-WG-011` — Migration is copy, verify, switch; never in-place reinterpretation

- **Context:** legacy `.jjk/repo.json` and future schema versions may contain valuable, imperfect state.
- **Decision:** read legacy input without modifying it, create a timestamped immutable backup, transform into a new staged database, replay and verify projections, write a migration receipt, then atomically switch the active metadata pointer. Keep the backup until an explicit retention action.
- **Rejected:** overwrite `repo.json`; mutate the live DB page-by-page without rollback; discard unknown legacy fields.
- **Consequences:** downgrade opens read-only or uses the preserved pre-migration store. Lossy mappings are surfaced before apply.
- **Revisit only if:** a future migration is provably metadata-only and still uses the same staged verification contract.

### `ADR-WG-012` — Removal has global uninstall and per-repository detach

- **Context:** uninstalling a binary and removing repository metadata are different operations. JJK-only refs may be the sole reachability anchors for valuable commits.
- **Decision:** global uninstall removes only installed binaries/completions/marker-owned shell blocks. `jjk repo detach` is a separately previewed repository operation that exports a manifest, converts JJK-only reachable attempt leaves into ordinary archive branches unless already ordinarily reachable, then removes local JJK metadata and namespaced local refs. Remote metadata deletion is a separate explicit operation. Destructive purge requires a repository-ID confirmation token.
- **Rejected:** package uninstall scans and deletes repository metadata; `rm -rf .jjk`; delete refs before reachability proof.
- **Consequences:** Git remains valid and understandable without JJK, and valuable history remains reachable.
- **Revisit only if:** a repository owner explicitly chooses a stronger archival policy.

---

## 3. Typed data and API shapes

These shapes are semantic contracts, not a final Rust module layout. All IDs are distinct newtypes; APIs MUST NOT accept a generic string where a typed ID is known.

```rust
struct RepoId(UuidV7);
struct OperationId(UuidV7);
struct EventId(UuidV7);
struct StateId(UuidV7);
struct AttemptId(UuidV7);
struct WorktreeId(UuidV7);
struct ConflictId(UuidV7);
struct PromotionId(UuidV7);
struct MigrationId(UuidV7);
struct SyncSegmentId(UuidV7);
struct ActorId(UuidV7);

enum CommandClass {
    JjkNative,
    GitEnhanced,
    TransparentGitPassthrough,
}

enum OperationPhase {
    Prepared,
    Mutating { step: u16 },
    EventsAppended { through_seq: u64 },
    Verified,
    Committed,
    RepairRequired { failed_phase: TxPhase, reason: ErrorCode },
    Aborted { verified_rollback: bool },
}

struct OperationRecord {
    schema_version: u16,
    operation_id: OperationId,
    repo_id: RepoId,
    class: CommandClass,
    kind: OperationKind,
    actor_id: ActorId,
    phase: OperationPhase,
    created_at_utc: Timestamp,
    updated_at_utc: Timestamp,
    expected_before: CrossLayerFingerprint,
    intended_after: IntendedTransition,
    plan_digest: Sha256,
    recovery_bundle: RecoveryBundleRef,
    lock_fence: u64,
    error: Option<StructuredError>,
}
```

`lock_fence` is monotonically allocated while holding the repository lock. Any delayed process whose fence is older than the active fence MUST fail before publishing refs or events.

```rust
struct EventEnvelope<T> {
    schema_version: u16,
    event_id: EventId,
    repo_id: RepoId,
    operation_id: OperationId,
    sequence: u64,
    event_type: EventType,
    actor_id: ActorId,
    occurred_at_utc: Timestamp,
    causal_event_ids: Vec<EventId>,
    repository_fingerprint: RepositoryFingerprint,
    payload_hash: Sha256,
    payload: T,
    provenance: Provenance,
}

struct ProjectionCheckpoint {
    projection_name: ProjectionName,
    projection_version: u16,
    last_event_seq: u64,
    row_count: u64,
    content_digest: Sha256,
    rebuilt_at_utc: Timestamp,
}
```

No API exposes `UPDATE event` or `DELETE event`. A correction is another event referencing the superseded event.

```rust
struct CrossLayerFingerprint {
    git: GitFingerprint,
    workspace: WorkspaceFingerprint,
    jj: Option<JjFingerprint>,
    journal_last_seq: u64,
    projections: Vec<ProjectionCheckpoint>,
}

struct GitFingerprint {
    object_format: GitObjectFormat,       // Sha1 | Sha256
    head_oid: Option<GitOid>,
    symbolic_head: Option<GitRefName>,
    observed_refs_digest: Sha256,
    git_common_dir_identity: FileIdentity,
}

struct WorkspaceFingerprint {
    worktree_id: WorktreeId,
    index_digest: Option<Sha256>,
    status_v2_digest: Sha256,
    at_risk_paths_digest: Sha256,
}
```

The recovery bundle stores only bytes that the planned mutation can overwrite or make unreachable. It MUST include the exact pre-operation refs, index bytes when relevant, at-risk untracked files, worktree registration, and the plan. It MUST exclude secrets and ignored content unless that content is explicitly in the at-risk path set and the user authorizes capture.

```rust
struct StateCaptured {
    state_id: StateId,
    attempt_id: AttemptId,
    kind: StateKind,
    label: Label,
    message: Option<Message>,
    logical_parent: Option<StateId>,
    snapshot_commit_oid: GitOid,
    snapshot_tree_oid: GitOid,
    workspace_base_oid: Option<GitOid>,
    excluded_paths: ExclusionSummary,
    evidence_ids: Vec<EvidenceId>,
}

struct ExactDelta {
    source_state: StateId,
    source_parent: StateId,
    parent_tree_oid: GitOid,
    state_tree_oid: GitOid,
    canonical_patch_id: Sha256,
    target_base_oid: GitOid,
}

struct ConflictRecord {
    conflict_id: ConflictId,
    operation_id: OperationId,
    kind: ConflictKind,
    base_oid: GitOid,
    ours_oid: GitOid,
    theirs_oid: GitOid,
    paths: Vec<ConflictPath>,
    sandbox_worktree: WorktreeId,
    pre_target_fingerprint: CrossLayerFingerprint,
    status: ConflictStatus,               // Open | Resolved | Aborted
    resolution_tree_oid: Option<GitOid>,
    resolution_evidence: Vec<EvidenceId>,
}

struct PromotionRecord {
    promotion_id: PromotionId,
    source_state: StateId,
    canonical_ref: GitRefName,
    expected_previous_oid: GitOid,
    result_oid: GitOid,
    policy_version: PolicyVersion,
    evidence: Vec<EvidenceBinding>,
    approver: ActorId,
    rollback_ref: GitRefName,
    status: PromotionStatus,
}
```

Evidence bindings include the validator identity, command/adapter version, target tree/content hash, started/finished timestamps, exit status, and artifact digest. Evidence for a different tree is stale even if its label is identical.

```rust
struct SyncSegmentManifest {
    schema_version: u16,
    segment_id: SyncSegmentId,
    repo_id: RepoId,
    device_id: DeviceId,
    parent_segments: Vec<SyncSegmentId>,
    first_event_seq: u64,
    event_count: u32,
    events_digest: Sha256,
    referenced_git_oids: Vec<GitOid>,
    created_at_utc: Timestamp,
}

struct MigrationReceipt {
    migration_id: MigrationId,
    source_format: SchemaDescriptor,
    target_format: SchemaDescriptor,
    source_digest: Sha256,
    backup_path: PathBuf,
    staged_db_digest: Sha256,
    imported_counts: BTreeMap<RecordKind, u64>,
    warnings: Vec<MigrationWarning>,
    projection_digests: Vec<ProjectionCheckpoint>,
    switched_at_utc: Option<Timestamp>,
}

struct DetachPlan {
    repo_id: RepoId,
    metadata_root: PathBuf,
    ordinary_reachable_oids: BTreeSet<GitOid>,
    jjk_only_leaf_oids: BTreeSet<GitOid>,
    archive_ref_creations: Vec<RefCas>,
    local_ref_deletions: Vec<RefCas>,
    export_manifest: PathBuf,
    shell_blocks: Vec<OwnedShellBlock>,
    remote_actions: Vec<NeverImplicit>,
}
```

### 3.1 Core service APIs

```rust
trait OperationEngine {
    fn plan(&self, request: TypedRequest) -> Result<OperationPlan, PlanError>;
    fn execute(&self, plan: OperationPlan) -> Result<CommittedOperation, RepairTicket>;
    fn continue_operation(&self, id: OperationId) -> Result<CommittedOperation, RepairTicket>;
    fn abort_operation(&self, id: OperationId) -> Result<VerifiedAbort, RepairTicket>;
    fn inspect_operation(&self, id: OperationId) -> OperationInspection;
}

trait Reconciler {
    fn observe(&self) -> Result<ObservedWorld, ObserveError>;
    fn diff(&self, journal: JournalHead, world: ObservedWorld) -> ReconcilePlan;
    fn apply(&self, plan: ReconcilePlan) -> Result<ReconcileReceipt, RepairTicket>;
}

trait GitPassthrough {
    fn run_native(
        &self,
        argv: Vec<NativeOsString>,
        cwd: NativePath,
        inherited_env: InheritedEnvironment,
        stdio: InheritedStdio,
    ) -> NeverNormalizeProcessResult;
}
```

`GitPassthrough::run_native` MUST NOT route through a shell, join argv into a string, parse Git flags, replace stdin, colorize output, buffer a pager, redact child output, or translate exit status.

---

## 4. Invariants and their active threats

A threat without a tripwire and containment move is not accepted into this plan.

| ID | Invariant | Threat | Tripwire | Pre-approved containment |
|---|---|---|---|---|
| `INV-001` | No user work is lost. | Dirty tracked/index/untracked bytes are overwritten during return/pick/migration. | Before/after workspace fingerprints differ outside the plan; at-risk path scan finds collision. | Stop before publish; pin recovery bundle; restore exact index and at-risk paths or keep operation sandboxed. |
| `INV-002` | Git remains valid and usable without JJK. | JJK-only storage becomes required to resolve ordinary branches or commits. | Git-only conformance clone cannot log, checkout, diff, commit, fetch, push. | Block release; materialize standard Git refs/objects; remove the dependency. |
| `INV-003` | External Git truth is never silently rewritten. | Reconciliation “fixes” an unexpected ref move. | Observed ref does not match the last recorded OID and has no committed JJK operation. | Append external observation; classify fast-forward/rewrite/delete; require explicit repair for ambiguity. |
| `INV-004` | One semantic mutation has one operation ID and one terminal result. | Retry duplicates a state, event, ref, or promotion. | Same idempotency key maps to different plan digest or more than one committed terminal event. | Quarantine conflicting retry; return prior result only when plan digest matches exactly. |
| `INV-005` | Events are immutable and projections are reproducible. | Migration or repair edits history in place. | Event table update/delete hook fires; rebuild digest differs. | Refuse write; restore journal backup; rebuild projections; append correction event. |
| `INV-006` | Short names never resolve ambiguously in automation. | Prefix/label collision chooses the wrong state. | Resolver returns multiple candidates above threshold or top scores within ambiguity margin. | Exit `JJK-E-TARGET-AMBIGUOUS`; print stable IDs and distinguishing context; require exact selection. |
| `INV-007` | Historical return preserves all futures. | Source branch is moved backward or later states become unreachable. | Ref delta includes source branch rewind; reachability proof loses a pre-operation leaf. | Abort plan; restore refs from recovery boundary; use detached historical context. |
| `INV-008` | Exact pick imports only parent→state effect. | Merge-base or branch diff drags unrelated changes. | Computed trees/patch ID do not match recorded source parent and state. | Reject source as non-pickable; repair provenance before retry. |
| `INV-009` | Concurrent writers never share mutable workspace bytes. | Two agents register the same worktree or branch. | Duplicate filesystem identity, Git worktree record, active owner, or branch checkout. | Deny second writer; allocate a new attempt/branch/worktree. |
| `INV-010` | Canonical refs move only by validated CAS promotion or external Git truth. | Stale validation or external ref move races promotion. | Current tip differs from `expected_previous_oid`; evidence tree hash differs from result tree. | Fail without ref update; refresh/revalidate/replan. |
| `INV-011` | A conflict cannot look committed. | Partial files exist while operation/event says success. | Open conflict record plus terminal `Committed`, or target fingerprint changed before resolution. | Mark repair-required; restore target; retain isolated sandbox. |
| `INV-012` | A crash is restartable from durable facts. | Process dies between Git mutation and event append. | Prepared nonterminal operation found on startup. | Run deterministic phase classifier; roll forward only if intended transition is exactly observed, otherwise roll back. |
| `INV-013` | Migration never destroys the only readable source. | New binary overwrites legacy metadata before verification. | Source digest/path changes before receipt `switched_at_utc`. | Stop; restore source from immutable backup; migration remains failed. |
| `INV-014` | Remote input is inert until validated. | Malformed or malicious segment drives refs/files/commands. | Bounds/schema/hash/repo-ID validation fails or unknown active instruction appears. | Quarantine object; no projection/ref/worktree mutation; surface segment ID. |
| `INV-015` | Removal cannot make valuable commits unreachable by default. | Detach deletes last namespaced refs. | Reachability set after planned deletions is smaller than before, excluding explicitly purged OIDs. | Create ordinary archive leaf refs first; verify; only then delete JJK refs. |
| `INV-016` | Transparent Git passthrough is behaviorally transparent. | UTF-8 conversion, shell quoting, buffered stdio, signal translation, or JJK error replaces Git result. | Differential harness differs on argv/cwd/env/TTY/output/signal/exit tuple. | Block release; bypass wrapper path until native process contract passes. |
| `INV-017` | SQLite locality assumptions are proved, not guessed. | Repo resides on NFS/SMB/cloud-synced or broken-lock filesystem. | WAL probe does not return `wal`; cross-process lock/checkpoint probe fails; filesystem identity changes. | Enter read-only JJK mode; allow Git; require explicit relocation or supported storage. |
| `INV-018` | Recovery material lives until outcome verification. | Cleanup runs after apparent Git success but before event/projection verification. | Recovery bundle reference count reaches zero before terminal verified phase. | Treat as invariant breach; stop all mutation and escalate with operation bundle. |
| `INV-019` | JJ remains optional. | Missing/stale JJ blocks Git-only operation or silently changes Git truth. | Capability snapshot changes or JJ import/export differs from Git fingerprints. | Degrade explicitly to Git-only before mutation; if already mutated, repair against Git authority. |
| `INV-020` | Metadata sync never transports secrets or machine-local restoration data. | Env values, credentials, absolute paths, or terminal history enter a segment. | Data-class scanner finds forbidden fields before segment creation/import. | Reject segment; identify offending event field; create redacted local-only event if useful. |

---

## 5. Twelve-step operational wargame

### Scenario seed

The sequential game uses one repository so failures compound realistically:

- existing Git repository `R`, with `main` and two historical commits;
- staged, unstaged, and untracked work at different moments;
- Git SHA-1 initially, with SHA-256 covered as an alternate fixture;
- JJ absent at init, then installed later to prove optionality;
- two agents working concurrently;
- a remote that another client may advance;
- a legacy `.jjk/repo.json` introduced during migration rehearsal;
- eventual JJK removal while preserving all useful Git history.

Each step has four branches:

- `N`: nominal success;
- `L`: loud failure;
- `H`: half-success that lies—the most dangerous branch;
- `I`: improbable but plausible boundary event.

Likelihood is qualitative (`common`, `probable`, `occasional`, `rare`); blast radius is bounded explicitly. Every branch has a pre-approved resolution.

### `WG-01` — Initialize an existing repository

**Command class:** `CC-JN`, JJK-native.  
**Entry:** Git works; JJK metadata is absent; existing HEAD/index/worktree/ref state is fingerprinted.  
**Plan:** resolve Git common dir and worktree identity; probe object format and filesystem/WAL capability; import reachable commits and refs as observations; stage DB; append `SafeSpaceInitialized` plus `GitCommitObserved` events; build projections; create required namespaced reachability refs by CAS; verify Git/index/worktree byte identity; commit operation.

| Branch | Likelihood / blast radius | Detection signal | Pre-approved resolution |
|---|---|---|---|
| `WG-01-N` | common / metadata only | Imported counts and graph digest match independent Git enumeration; HEAD/index/status unchanged. | Commit `OP-INIT`; show repo ID, imported counts, storage mode, and “Git unchanged.” |
| `WG-01-L` | occasional / none if contained | `JJK-E-STORAGE-UNSAFE`, permissions error, unsupported object format, or malformed existing metadata. | Leave Git untouched; remove only staged temp files owned by this operation; print exact unsupported capability. Transparent Git remains available. |
| `WG-01-H` | probable in naïve designs / repo-wide semantic corruption | Init reports success but misses non-current branches, merge parents, packed refs, SHA-256 OIDs, or linked-worktree common dir. | Gate success on independent `for-each-ref` + graph enumeration and projection digest. If mismatch, mark repair-required and discard staged DB; never call partial import initialized. |
| `WG-01-I` | rare / worktree and metadata collision | `.git` is a file, repo is bare, ref backend is reftable, unborn HEAD exists, or another process initializes concurrently. | Use Git plumbing, not path assumptions. The repository lock plus unique repo-ID CAS elects one initializer; loser reopens winner. Bare/unborn support is explicit capability behavior, not guessed. |

**Gate `AG-01`:** init is idempotent; a second invocation creates no duplicate events or refs and returns the original `RepoId`. Git-only pre/post snapshots are identical except namespaced JJK refs and metadata-root files.

### `WG-02` — Run external Git through transparent passthrough

**Command class:** `CC-GP`, transparent Git passthrough.  
**Entry:** JJK is initialized. The user runs native Git argv containing spaces/non-UTF-8 bytes, may use a pager/editor/credential helper, and may interrupt it.  
**Plan:** take a short pre-observation without holding the mutation lock; spawn Git directly with native argv/cwd/env/stdio; forward signals; preserve result; attempt silent post-observation; authoritative reconcile waits for the next JJK-native command.

| Branch | Likelihood / blast radius | Detection signal | Pre-approved resolution |
|---|---|---|---|
| `WG-02-N` | common / intended Git scope | Differential process tuple equals direct Git; next `jjk status` observes new commit/ref and appends idempotent `GitCommitObserved`. | Preserve Git result exactly. Reconcile once under TX-01 before resolving any semantic target. |
| `WG-02-L` | common / none beyond Git's own failure | Git returns nonzero, is signaled, credential prompt fails, or hook rejects. | Return the same exit/signal and untouched output. Post-observe may note actual side effects, because failed Git commands can still mutate, but JJK never relabels the result. |
| `WG-02-H` | occasional / wrong automation behavior | Git succeeds, but wrapper emits extra text, changes color/pager/TTY, normalizes argv, or returns JJK reconciliation failure instead of Git's zero. | `INV-016` blocks release. Reconciliation failure becomes internal pending state and cannot replace Git's process result. |
| `WG-02-I` | rare / deadlock or recursive process tree | A Git hook invokes `jjk`, or an editor waits while wrapper holds a repository lock. | Never hold JJK lock across child Git. Nested JJK command gets its own TX-01; post-reconcile deduplicates observations. |

**Gate `AG-02`:** differential harness covers native argv, cwd, environment visibility, binary stdin/stdout/stderr, TTY/pager, SIGINT/SIGTERM, hooks, editor, credential helper, zero/nonzero/signal termination, and Git commands with partial side effects.

### `WG-03` — Capture meaningful states without disturbing workspace truth

**Command class:** `CC-JN`, JJK-native (`save`, `step`, `nice`).  
**Entry:** repository contains staged, unstaged, and untracked work; ignored files exist; user index is meaningful.  
**Plan:** reconcile; resolve current attempt; fingerprint HEAD/index/worktree; build snapshot with a temporary index or equivalent Git plumbing; create commit/tree and namespaced reachability ref without using the user's index as scratch; append `StateCaptured`; verify captured tree plus exact preservation of HEAD/index/worktree.

| Branch | Likelihood / blast radius | Detection signal | Pre-approved resolution |
|---|---|---|---|
| `WG-03-N` | common / new state only | Snapshot tree matches declared capture policy; state parent/attempt correct; user index bytes and status digest unchanged. | Commit state and show included/excluded summary, stable state ID, parent, and return command. |
| `WG-03-L` | occasional / none if contained | Disk full, object write error, file changes during capture, permission error, or unsupported special file. | Delete only unreferenced temp artifacts after proving no ref/event publication; otherwise leave repair ticket. Ask user to retry after workspace stabilizes. |
| `WG-03-H` | probable in naïve designs / user staging intent corrupted | State appears correct but `git diff --cached` changes, ignored files leak in, symlink/mode bits differ, or a clean capture creates noisy duplicate Git history. | Verify index byte digest, captured tree, modes, and exclusion policy. Explicit semantic save on unchanged tree may append a JJK event but MUST NOT create ordinary Git commit noise. |
| `WG-03-I` | rare / inconsistent snapshot | File mutates while being hashed or an untracked path becomes tracked mid-capture. | Compare pre/post path and index fingerprints; retry the read phase a bounded number of times before durable prepare. After prepare, any drift aborts and leaves workspace untouched. |

**Gate `AG-03`:** capture matrix covers staged-only, unstaged-only, mixed, untracked, ignored, empty, symlink, executable bit, deletion, rename/content-equivalent, large file, concurrent file mutation, SHA-1, and SHA-256 repositories.

### `WG-04` — Return to a historical state without destroying its future

**Command class:** `CC-JN`, JJK-native.  
**Entry:** states `S1 → S2 → S3` exist; current tip is `S3`; user requests `S1`; dirty work may collide with `S1`.  
**Plan:** reconcile; resolve exactly or stop ambiguous; determine if at-risk bytes require a recovery capture; preview ref/worktree effect; durable prepare; activate `S1` exactly without moving the `S3` source branch; append `StateActivated`; verify tree, cleanliness policy, and reachability of `S2/S3`.

| Branch | Likelihood / blast radius | Detection signal | Pre-approved resolution |
|---|---|---|---|
| `WG-04-N` | common / active worktree only | Active tree equals `S1`; source future refs/OIDs unchanged; next navigation can return to `S3`. | Commit historical context with `pending_divergence_from=S1`; do not create a branch until state-producing mutation. |
| `WG-04-L` | probable / none if stopped | Fuzzy target ambiguous, missing Git object, dirty collision, submodule policy unavailable, or target path unsafe. | Stop before mutation. Show candidates or missing capability. Offer explicit capture/stash/clean-room return; never choose silently. |
| `WG-04-H` | occasional / future loss or fake exactness | Files look like `S1`, but source branch moved backward, untracked files from `S3` remain, index differs, or state graph claims clean exact return incorrectly. | Compare complete planned workspace fingerprint and reachability set. Any mismatch triggers restore from recovery and repair-required. |
| `WG-04-I` | rare / external race | Another process moves source refs or edits the worktree after plan but before activation. | Lock fence plus before-fingerprint check invalidates plan. Reconcile, re-resolve, and require a new preview if effects changed. |

**Gate `AG-04`:** clean and dirty return fixtures prove exact bytes/index, no duplicate capture on clean navigation, no future ref movement, and a new sibling attempt only after the first divergent state-producing mutation.

### `WG-05` — Provision parallel agents safely

**Command class:** `CC-JN`, JJK-native (`fork --worktree`).  
**Entry:** two agents request forks from the same state; one desired name collides; one process may die after Git worktree creation.  
**Plan:** allocate typed attempt/branch/worktree IDs; CAS-create separate refs; create distinct worktrees; append `AttemptForked` and ownership/handoff facts; verify Git worktree registry, filesystem identity, base tree, and non-overlap.

| Branch | Likelihood / blast radius | Detection signal | Pre-approved resolution |
|---|---|---|---|
| `WG-05-N` | common / isolated attempts | Distinct attempt IDs, refs, paths, worktree IDs, and owners; identical declared base OID. | Commit both provisions independently; each agent receives exact cwd and resume/return commands. |
| `WG-05-L` | probable / one attempted lane | Name/path exists, branch already checked out, permissions fail, or worktree add fails. | No fallback to shared cwd. Allocate a collision-free display suffix or require explicit existing-attempt adoption; clean only unregistered operation-owned empty path. |
| `WG-05-H` | occasional / concurrent data corruption | Metadata says two attempts, but both agents write the same inode tree, branch, or unregistered copied directory. | Gate on canonical filesystem identity plus `git worktree list --porcelain` equivalent. Quarantine duplicate registration and stop second writer. |
| `WG-05-I` | rare / orphan resources | Process crashes after branch/worktree creation but before events; user manually moves/deletes a worktree; Git prunes registration. | Startup repair matches durable planned IDs/ref/path. Complete registration if exact; otherwise preserve ref, classify worktree missing, and offer recreate at a new path. Never delete unrecognized files. |

**Gate `AG-05`:** stress at least 32 concurrent provisions from one base, with duplicate names and crash injection; no two committed worktree records share branch or filesystem identity, and all states remain graph-visible.

### `WG-06` — Apply an exact atomic pick

**Command class:** `CC-JN`, JJK-native.  
**Entry:** `S-fast-purple` logically descends from `S-purple`; target is orange. Only the fast-mode delta is requested.  
**Plan:** reconcile; resolve source/target; fetch/locate parent and state trees; derive exact parent→state delta and patch ID; create isolated operation worktree at target base; apply; verify paths/content/modes and prove unrelated purple delta absent; append `DeltaApplied` and resulting `StateCaptured`; CAS publish target attempt.

| Branch | Likelihood / blast radius | Detection signal | Pre-approved resolution |
|---|---|---|---|
| `WG-06-N` | common / target attempt only | Result tree equals deterministic application; fast-mode changed; purple-only content absent; provenance complete. | Commit a `cherry`/composed state with source, parent, target base, patch ID, and result OID. |
| `WG-06-L` | probable / sandbox only | Parent object missing, state has no valid logical parent, unsupported submodule delta, binary apply failure, or target ambiguity. | Stop target publication. Fetch missing object only when an explicit configured remote can satisfy the recorded OID; otherwise repair provenance. |
| `WG-06-H` | occasional / unrelated code imported | Command succeeds by cherry-picking source commit/branch or diffing merge-base, silently importing purple. | Mandatory negative oracle checks unrelated paths and exact tree algebra. If mismatch, discard sandbox publication and mark implementation defect `INV-008`. |
| `WG-06-I` | rare / representation edge | Source state has a Git merge commit, rename ambiguity, case-only rename on case-insensitive FS, symlink/submodule mode, or SHA-256 OIDs. | Logical parent remains explicit. Treat delta as tree content+mode effect; do not depend on rename heuristics. Unsupported filesystem materialization fails in sandbox with a portable conflict record. |

**Gate `AG-06`:** canonical snake fixture asserts “fast yes, purple no,” plus deletion, binary, mode, symlink, case collision, merge-source logical parent, and object-format variants. A plausible whole-commit implementation must fail the gate.

### `WG-07` — Resolve or abort a pick conflict

**Command class:** `CC-JN`, JJK-native continuation of `WG-06`.  
**Entry:** exact delta conflicts with orange on two paths. Target ref/worktree remains at pre-pick state.  
**Plan:** persist `ConflictRecord`; expose isolated sandbox and exact base/ours/theirs; user/agent edits resolution; `jjk operation continue <id>` verifies no unresolved entries, captures resolution tree/evidence, then CAS-publishes; abort deletes only operation-owned sandbox after verifying target unchanged.

| Branch | Likelihood / blast radius | Detection signal | Pre-approved resolution |
|---|---|---|---|
| `WG-07-N` | probable / isolated sandbox then target publish | Conflict paths resolved; index has no unresolved stages; validation evidence binds result tree; target CAS succeeds. | Append resolution and delta events, publish once, retain provenance, then remove sandbox after committed verification. |
| `WG-07-L` | common / sandbox only | User aborts, unresolved entries remain, validation fails, or target moves before continue. | Keep target unchanged. Abort restores nothing because target was never changed; or rebase/recreate a new conflict sandbox against the new target with a new plan. |
| `WG-07-H` | occasional / target contaminated | CLI says “conflict” but markers/index stages were written into active target, or says “success” while conflict stages remain. | `INV-011` forces repair: restore target from before-fingerprint, pin conflict sandbox, and deny commit status. |
| `WG-07-I` | rare / hostile or accidental sandbox change | Resolver edits paths outside conflict set, deletes operation metadata, or replaces sandbox directory. | Re-fingerprint complete sandbox; show out-of-scope changes and require explicit inclusion or reset within sandbox. Never publish an unreviewed widened delta. |

**Gate `AG-07`:** at every pause/abort point, target ref, index, tracked, and untracked fingerprints equal entry state. Continue is idempotent and publishes exactly once.

### `WG-08` — Promote a validated state to a canonical branch

**Command class:** `CC-JN` with Git-enhanced ref publication semantics.  
**Entry:** candidate state is validated; canonical `main` may be checked out elsewhere; another client may advance it while validation runs.  
**Plan:** resolve candidate and policy; bind evidence to result tree; inspect all worktrees containing canonical ref; preview commit/ref delta and rollback; durable prepare; create rollback ref; CAS-update canonical ref only if expected tip and occupancy policy still hold; append `CanonicalPromoted`; verify canonical tree and Git-only usability.

| Branch | Likelihood / blast radius | Detection signal | Pre-approved resolution |
|---|---|---|---|
| `WG-08-N` | common / canonical ref | Policy passes, evidence tree matches, expected tip unchanged, worktree handling safe, CAS succeeds. | Commit promotion with previous tip and rollback command; retain rollback ref per policy. |
| `WG-08-L` | probable / none | Missing/stale evidence, policy version changed, non-fast-forward disallowed, checked-out dirty canonical worktree, or CAS stale tip. | Fail before ref movement. Refresh candidate onto new canonical base and revalidate; never force as retry. |
| `WG-08-H` | occasional / canonical inconsistency | Ref moves but checked-out canonical worktree/index still shows old files; or metadata says promoted before CAS. | Never update a ref behind a dirty checked-out worktree. Publish in a controlled worktree or require explicit checkout refresh. Verify worktree/ref coherence before committed event. |
| `WG-08-I` | rare / governance compromise | Evidence artifact is altered after validation, clock ordering lies, approver identity missing, or rollback ref name collides. | Bind content digests rather than filenames/times; use typed actor and unique promotion ID; CAS-create rollback ref before canonical update. |

**Gate `AG-08`:** inject an external ref advance in every interval from policy read through event append. Outcome is either one verified CAS promotion or no canonical ref movement; rollback restores exact previous OID.

### `WG-09` — Recover from a crash at any transaction boundary

**Command class:** recovery engine for all JJK-native/Git-enhanced operations.  
**Entry:** kill/power-loss simulation occurs after each TX-01 boundary, including after Git mutation but before events and after events but before verification.  
**Plan:** startup obtains lock/fence; opens SQLite so its own recovery completes; enumerates nonterminal operations; compares durable plan, recovery bundle, actual Git/JJ/files, and journal; classifies exact phase; rolls forward only when intended outcome is fully present, otherwise rolls back to proved before-state; appends repair outcome.

| Branch | Likelihood / blast radius | Detection signal | Pre-approved resolution |
|---|---|---|---|
| `WG-09-N` | occasional over product life / one operation | Prepared record plus actual world maps uniquely to before or intended after. | Deterministically abort or finish; verify; append `OperationRepaired`; release recovery material only after terminal state. |
| `WG-09-L` | rare / operation blocked | Recovery bundle missing/corrupt, Git objects missing, SQLite integrity fails, or filesystem denies restore. | Enter mutation-disabled safe mode; keep Git commands usable; emit doctor bundle with operation ID and exact missing evidence; escalate. |
| `WG-09-H` | probable in naïve sagas / dual truths | Git result exists with no event, event exists with wrong ref, or startup ignores “prepared” because process is gone. | Nonterminal operations are mandatory startup work before target resolution. Reconciler may synthesize the missing completion only when hashes exactly match the prepared plan. |
| `WG-09-I` | rare / repeated crash or stale process | Recovery process crashes; old process resumes after lock theft; disk reports success then loses writes. | Idempotent repair steps plus fence tokens reject stale publisher. Reopen and re-verify durable state after fsync/commit; repeated failure escalates without discarding bundle. |

**Gate `AG-09`:** deterministic fault injection kills the process at every durable write/ref/index/worktree/event/projection/verification step for capture, return, fork, pick, conflict continue, promotion, migration switch, sync import, and detach. Every restart reaches exactly verified-before or verified-after, never an unclassified middle.

### `WG-10` — Migrate legacy and future metadata safely

**Command class:** `CC-JN`, JJK-native administration.  
**Entry:** legacy `.jjk/repo.json` version 1 may be large, partially inconsistent, or accompanied by `history.json`, backups, freezes, and JJK refs.  
**Plan:** inspect read-only; hash all sources; reconcile Git separately; preview mapping/warnings; immutable backup; create staged target DB; transform records to typed events while preserving unknown raw fields in a legacy envelope; rebuild projections; run graph/ref/count/reachability checks; atomically switch active pointer; retain receipt and old store.

| Branch | Likelihood / blast radius | Detection signal | Pre-approved resolution |
|---|---|---|---|
| `WG-10-N` | probable for adopters / metadata representation | Counts, topology, OIDs, tags, navigation, deletion context, and reachability match declared mapping; source digest unchanged. | Switch pointer, append `MigrationApplied`, keep backup, and show downgrade/read-only path. |
| `WG-10-L` | probable / none if contained | Invalid JSON, unknown required schema, duplicate IDs, missing commits, disk full, staged projection mismatch. | Do not switch. Leave legacy store active/readable; emit receipt with path-specific warnings and repair instructions. |
| `WG-10-H` | occasional / silent semantic loss | Migration “succeeds” but flattens lanes/attempts, changes parentage, drops unknown fields, imports current external Git state as if legacy-authored, or cannot reproduce graph. | Golden legacy fixtures and independent source→target ledger block switch. Preserve uncertain input as `LegacyRecordObserved`, never invent certainty. |
| `WG-10-I` | rare / version skew | Old binary opens new DB, two clients migrate concurrently, migration is interrupted at pointer rename, or source lives in linked worktree path. | Schema compatibility gate makes old binary read-only/refuse mutation. Repository lock elects one migration. Pointer switch is atomic and receipt-driven; metadata root comes from Git common dir. |

**Gate `AG-10`:** migrate representative real legacy data, corrupt/truncated variants, duplicate IDs, missing refs, all state kinds, deleted/recovered contexts, huge history, linked worktrees, and interrupted switches. Source files remain byte-identical.

### `WG-11` — Synchronize optional metadata through a Git remote

**Command class:** `CC-JN`/`CC-GE`, JJK sync with Git transport.  
**Entry:** two devices have offline event segments; remote may reject non-fast-forward, contain unknown future event types, or be malicious.  
**Plan:** export bounded local segment after secret/path classification; write Git objects; fetch remote JJK refs to quarantine; validate repo ID/schema/bounds/hash chain; idempotently import events; rebuild affected projections; CAS-push this device head; never mutate user branches or active worktree.

| Branch | Likelihood / blast radius | Detection signal | Pre-approved resolution |
|---|---|---|---|
| `WG-11-N` | common when enabled / metadata only | Segment hashes valid, duplicate events identical, projections deterministic, device head CAS succeeds. | Commit sync receipt showing imported/exported counts and unresolved semantic conflicts. |
| `WG-11-L` | probable / local metadata unchanged or locally imported only | Offline/auth failure, push rejection, unsupported schema, missing referenced object, quota, or remote deletion. | Keep local committed events and segment. Retry fetch/merge/push with backoff only for transient errors; never roll back local meaning due to transport failure. |
| `WG-11-H` | occasional / divergent or leaked metadata | Copying SQLite appears to work but loses WAL pages; last-writer-wins drops offline events; absolute paths/env leak; same label silently overwrites another device's annotation. | Only segment protocol is supported. Data-class scan rejects forbidden fields. Concurrent semantic edits remain explicit competing annotations/conflicts. |
| `WG-11-I` | rare / integrity or parser attack | Same event ID has different payload, hash chain cycle, decompression bomb, enormous segment, remote object claims another repo ID, unknown event requests execution. | Bound before allocation; quarantine; no event activation/ref/file command; report `JJK-E-SYNC-INTEGRITY` and require human trust decision. |

**Gate `AG-11`:** two-device offline convergence, retry/idempotency, non-fast-forward, deletion, unknown future event preservation, malicious bounds corpus, secret scanner, and Git-user branch noninterference all pass. Local DB/WAL files are never remote payloads.

### `WG-12` — Uninstall globally and detach the repository

**Command class:** global package operation plus `CC-JN` repository detach.  
**Entry:** JJK has local metadata, namespaced refs, attempt commits not otherwise reachable, shell integration, optional JJ, and remote sync refs.  
**Plan:** distinguish global uninstall from repository detach. For detach: reconcile; dry-run reachability; export metadata manifest; create ordinary archive branches for JJK-only leaf OIDs by CAS; verify all prior useful commits remain reachable; checkpoint/close DB; remove only marker-owned local integration and local JJK metadata/refs; leave user branches, worktrees, JJ, remotes, and remote metadata untouched unless separately requested.

| Branch | Likelihood / blast radius | Detection signal | Pre-approved resolution |
|---|---|---|---|
| `WG-12-N` | eventual / JJK local layer only | Git-only clone/log/checkout/diff works; all useful OIDs reachable from ordinary refs; shell file changes match owned markers; export readable. | Commit detach receipt outside removed metadata location; print archive branches and explicit separate remote cleanup command. |
| `WG-12-L` | probable / no removal | Dirty/incomplete operation, open conflict, failed reachability proof, metadata DB busy, archive ref collision, or shell block modified by user. | Refuse detach. Resolve/abort operations; choose new archive prefix; leave modified shell content and print manual boundaries rather than editing it. |
| `WG-12-H` | occasional / permanent loss or collateral config damage | Package uninstall deletes repo metadata; detach removes last refs; broad text replacement damages shell config; remote refs are silently deleted. | Separate operations by design. Default detach creates refs before deletions and proves reachability. Marker ownership is exact. Remote deletion is never implicit. |
| `WG-12-I` | rare / partial cleanup | Crash after archive refs but before metadata removal, or after metadata removal but before shell cleanup; user reinstalls later. | Order makes partial state safe: extra archive/JJK refs are harmless. Reinstall detects detach export/receipt but never auto-imports; cleanup is idempotent and marker-scoped. |

**Gate `AG-12`:** uninstall matrix covers package-only removal, detach dry-run, detach apply, crash at every step, JJK-only commits, packed/reftable refs, linked worktrees, modified shell markers, remote sync configured, and reinstall. No ordinary Git ref/config/worktree byte changes outside the approved archive creations.

---

## 6. Failure modes resolved before implementation

| ID | Failure history from the six-month premortem | Mechanism that would have permitted it | Design resolution and permanent guard |
|---|---|---|---|
| `FM-001` | Users stopped trusting JJK after one return erased staged work. | Workspace safety checked only `git status --porcelain`, not index bytes and at-risk untracked collisions. | `INV-001`, recovery bundles, index digest, at-risk path capture, and dirty return fault fixtures. |
| `FM-002` | Parallel agents produced inexplicable mixed states. | Shared worktree with only an `owner` metadata field. | Distinct attempt/ref/worktree identity enforced by `ADR-WG-006`; stress gate `AG-05`. |
| `FM-003` | “Exact pick” became ordinary cherry-pick with a friendlier label. | Source delta inferred from branch/merge-base rather than logical parent trees. | `ADR-WG-007`, negative snake oracle, and full provenance fields. |
| `FM-004` | Metadata and Git periodically disagreed after crashes; repair meant hand-editing SQLite. | No durable prepare; success event written before cross-layer verification. | `ADR-WG-002`, nonterminal startup repair, every-boundary fault injection. |
| `FM-005` | WAL databases corrupted or locked forever on network homes. | WAL selected by config/default, not proved against actual metadata filesystem. | `DEC-STORAGE-01`, runtime capability probe, fail-closed mutation mode. |
| `FM-006` | The Git wrapper broke scripts using binary paths, pagers, and signals. | CLI parsed UTF-8 strings and captured stdio. | Native OS-string API and differential passthrough contract `INV-016`. |
| `FM-007` | Promotion occasionally overwrote a teammate's newer main. | Read-then-force-update with stale validation. | Evidence content binding and Git CAS `ADR-WG-009`; race injection `AG-08`. |
| `FM-008` | Remote sync leaked machine paths and lost offline annotations. | SQLite copy/last-writer-wins sync. | Immutable bounded segments, data classes, union-by-event-ID, explicit semantic conflicts. |
| `FM-009` | Migration appeared successful but lost deleted-state recovery and lane topology. | Count-only migration test and destructive source rewrite. | Source-target ledger, graph/reachability digests, raw legacy envelopes, no source mutation. |
| `FM-010` | Removing JJK garbage-collected months of experiments. | Namespaced refs deleted without post-delete reachability proof. | Archive ordinary leaf refs before deletion and gate `AG-12`. |
| `FM-011` | JJK became a feature pile nobody could operate. | Every edge feature flattened into top-level commands, errors were vague, repair required architecture knowledge. | Command classes, tiny core UX elsewhere, stable error codes, symptom playbooks below, and weakest-reader acceptance. |
| `FM-012` | Recovery code itself caused the second incident. | “Best effort” repair guessed intent when evidence was incomplete. | Exact phase classifier; ambiguous repair enters mutation-disabled safe mode and escalates. |

---

## 7. Symptom-keyed operator playbooks

All commands here are proposed v0.1 operator surfaces. Each playbook starts from what the operator sees, not an internal subsystem name.

### `PB-001` — Symptom: `JJK-E-LOCK-BUSY` or “another JJK operation owns this safe space”

1. Run `jjk operation list --active`.
2. If an operation is making progress, do not delete the lock; wait or inspect it with `jjk operation show <operation-id>`.
3. If its process is gone, run `jjk repair --operation <operation-id> --plan`.
4. Apply only the printed deterministic repair with `jjk repair --operation <operation-id> --apply`.
5. **Stop and escalate** if the process is live but the lock fence differs, or if two operations both claim the active fence.

Expected safe result: one active writer, or one repairable nonterminal operation; no forced lock deletion.

### `PB-002` — Symptom: `JJK-E-STORAGE-UNSAFE`, `journal_mode` is not `wal`, or repeated `SQLITE_BUSY`

1. Run `jjk doctor storage --verbose`.
2. Confirm the reported metadata path and filesystem class; do not move `.sqlite`, `-wal`, or `-shm` files while any JJK process is open.
3. If on NFS/SMB/cloud-synced storage, keep JJK mutation disabled and relocate the clone/worktree to a supported local filesystem; Git remains usable.
4. If local, close long-lived JJK readers and run `jjk doctor storage --checkpoint`.
5. **Stop and escalate** on integrity-check failure, lock incoherence, or a WAL mode that changes across opens.

Expected safe result: a proved local WAL configuration or explicit read-only JJK mode; never a silent weaker fallback.

### `PB-003` — Symptom: `JJK-E-RECONCILE-DIVERGED` after normal Git use

1. Run `jjk reconcile --plan`.
2. Inspect each external ref action classified as create, fast-forward, rewrite, delete, or ambiguous.
3. Apply unambiguous observations with `jjk reconcile --apply`.
4. For rewrite/delete, preserve the old OID under a recovery ref before accepting the observation.
5. **Stop and escalate** if an expected OID is missing from the object database or two repository identities appear.

Expected safe result: external Git is recorded as truth; no ref is moved merely to match old metadata.

### `PB-004` — Symptom: `JJK-E-TARGET-AMBIGUOUS`

1. Read the aligned candidates: full `StateId`, label, attempt, parent, Git OID, time, and status.
2. Retry using the full state ID, not another fuzzy phrase.
3. In automation, never pass `--first` or an equivalent silent tie breaker; none should exist.
4. **Stop and escalate** if a full typed ID maps to more than one payload.

Expected safe result: exactly one immutable ID is selected.

### `PB-005` — Symptom: `JJK-E-WORKSPACE-AT-RISK` before return/pick

1. Run `jjk operation show <operation-id> --at-risk-paths`.
2. Choose one explicit action: `jjk save ...`, create an isolated operation worktree, or abort.
3. Replan; compare the new before-fingerprint.
4. Do not use `git reset --hard`, `git clean`, or manual stash as a hidden prerequisite.
5. **Stop and escalate** if ignored files or submodules are in the at-risk set and no capture policy is declared.

Expected safe result: all bytes that could be overwritten are durably recoverable or untouched.

### `PB-006` — Symptom: `JJK-E-PICK-PARENT-MISSING`

1. Run `jjk show <state-id> --provenance` and note `logical_parent`, parent tree OID, and source tree OID.
2. If the recorded Git object is available on an explicitly configured remote, fetch that exact OID/ref without changing branches.
3. Run `jjk reconcile --apply`, then replan the pick.
4. If provenance itself is absent/ambiguous, mark the state non-pickable; do not substitute Git first-parent or merge-base.
5. **Stop and escalate** before inventing a parent.

Expected safe result: exact tree pair recovered or pick refused.

### `PB-007` — Symptom: `JJK-E-CONFLICT-OPEN` and a sandbox path is printed

1. Confirm `jjk operation show <operation-id>` says the target fingerprint is unchanged.
2. Resolve only inside the printed sandbox.
3. Inspect with `jjk conflict show <conflict-id>`; validate the complete resulting tree.
4. Continue with `jjk operation continue <operation-id>` or abort with `jjk operation abort <operation-id>`.
5. **Stop and escalate** if conflict markers/stages appear in the target worktree or the target ref moved.

Expected safe result: one provenance-rich resolved state or exact no-op abort.

### `PB-008` — Symptom: `JJK-E-PROMOTION-STALE` or “canonical tip moved”

1. Do not force-push or retry the stale plan.
2. Run `jjk promote <state-id> --to <canonical-ref> --plan` against the new tip.
3. Refresh/recompose candidate if required and rerun validation; old evidence is invalid for a changed tree.
4. Apply only when expected tip and evidence digest match.
5. **Stop and escalate** if a checked-out canonical worktree is dirty or policy cannot determine its owner.

Expected safe result: CAS promotion against current truth or no ref movement.

### `PB-009` — Symptom: `JJK-E-OPERATION-INCOMPLETE` on startup

1. Run `jjk repair --operation <operation-id> --plan`.
2. Confirm classifier output is exactly `before`, `intended-after`, or `ambiguous`.
3. Apply deterministic abort/finish only for the first two.
4. Preserve the recovery bundle and run `jjk doctor --bundle <operation-id>` for `ambiguous`.
5. **Stop and escalate** on ambiguous state; never hand-edit the DB or delete temp refs.

Expected safe result: verified terminal operation or mutation-disabled safe mode with complete evidence.

### `PB-010` — Symptom: `JJK-E-MIGRATION-VERIFY` or migration warnings include “lossy”

1. Confirm the legacy source digest is unchanged and note the backup path from the receipt.
2. Run `jjk setup --migration=check --json` and inspect the reported migration ID, record counts, topology, reachability, and raw-field warnings.
3. Fix the transformer or source inconsistency; create a new staged migration ID.
4. Do not edit the failed staged DB into compliance.
5. **Stop and escalate** if the source changed after hashing or the backup digest differs.

Expected safe result: old metadata remains active until a clean staged migration switches atomically.

### `PB-011` — Symptom: `JJK-E-SYNC-INTEGRITY`

1. Record remote, ref, object OID, segment ID, claimed repo ID, and failing bound/hash.
2. Keep the object quarantined; do not import with a force/ignore flag.
3. Fetch a known-good device head or ask the remote owner to republish a valid segment.
4. Same event ID with different payload is an incident, not a merge conflict.
5. **Stop and escalate** before changing local events, refs, or trust configuration.

Expected safe result: local journal unchanged; hostile/invalid input retained only as bounded diagnostic evidence.

### `PB-012` — Symptom: `JJK-E-PROJECTION-MISMATCH`

1. Run `jjk doctor projections --from-events --plan`.
2. Confirm journal integrity and immutable event count first.
3. Rebuild projections into staged tables; compare checkpoint digests.
4. Atomically swap projection tables only when deterministic digests match.
5. **Stop and escalate** if repeated rebuilds from the same journal differ.

Expected safe result: events untouched; deterministic projections restored.

### `PB-013` — Symptom: disk full during mutation

1. Stop new JJK mutations; do not clean recovery bundles.
2. Free space outside repository recovery paths.
3. Run `jjk repair --operation <operation-id> --plan`.
4. Finish/abort based on exact phase, then run storage integrity and Git object/ref checks.
5. **Stop and escalate** if fsync durability or object presence is uncertain.

Expected safe result: before or intended-after state, with no assumed write success.

### `PB-014` — Symptom: worktree directory missing or “registered worktree is prunable”

1. Run `jjk worktree doctor <worktree-id>`.
2. Verify attempt ref and base/result commits remain reachable.
3. If directory is absent, recreate into a new empty path; do not reuse an occupied path.
4. If unrecognized files exist at the recorded path, leave them untouched and choose another path.
5. **Stop and escalate** if branch/ref identity conflicts with another live worktree.

Expected safe result: same attempt identity with a new verified worktree registration, or preserved ref with explicit offline status.

### `PB-015` — Symptom: detach says “JJK-only commits would become unreachable”

1. Run `jjk repo detach --dry-run --verbose`.
2. Inspect every JJK-only leaf OID and proposed ordinary archive branch.
3. Choose a collision-free archive prefix and rerun dry-run.
4. Apply detach only after post-plan reachability proves all non-purged OIDs remain reachable.
5. **Stop and escalate** if any object is missing or a ref cannot be CAS-created.

Expected safe result: archive refs exist and verify before JJK refs/metadata are removed.

### `PB-016` — Symptom: `jjk git` behaves differently from direct `git`

1. Capture the minimal differential tuple: platform, raw argv representation, cwd, relevant environment key names (not secret values), TTY state, signal, direct result, passthrough result.
2. Reproduce in a throwaway repository with the passthrough conformance harness.
3. Disable/bypass the wrapper path for the affected platform until fixed; users can run Git directly.
4. Do not “fix” by parsing/requoting the command through a shell.
5. **Stop and escalate** if the mismatch involves signals, stdin, credentials, output bytes, or exit status.

Expected safe result: byte/native-string and process behavior parity, or no claim of transparency.

---

## 8. Known/unknown sweep

### 8.1 Known knowns

| ID | Known fact | Encoded response |
|---|---|---|
| `KK-001` | Git is the universal substrate; JJ is optional. | `ADR-WG-001`, `INV-019`, Git-only gates. |
| `KK-002` | SQLite WAL supports local concurrent readers but one writer and not network filesystems. | `DEC-STORAGE-01`, `INV-017`, `PB-002`. |
| `KK-003` | Git/SQLite/files cannot share one native transaction. | Repairable saga `ADR-WG-002`. |
| `KK-004` | Worktrees share a Git common dir but have distinct worktree git dirs and bytes. | Common metadata root plus typed per-worktree records. |
| `KK-005` | Git object formats include SHA-1 and SHA-256. | Variable-length typed `GitOid`; no 40-character assumption. |
| `KK-006` | External Git commands can partially mutate even on failure. | Post-observation plus next-operation reconciliation. |

### 8.2 Known unknowns with tripwires

| ID | Unknown before implementation | Resolve-before-code experiment / tripwire | Decision if unresolved |
|---|---|---|---|
| `KU-001` | Which bundled SQLite Rust path exposes reliable backup, hooks, and WAL checkpoint behavior on all targets. | Spike finalists with process-kill, disk-full, busy-reader, and cross-process writer fixtures on macOS/Linux/Windows. | Choose boring, supported binding; do not expose backend-specific APIs. |
| `KU-002` | Reliable filesystem locality/lock-coherence detection across macOS/Linux/Windows/WSL. | Capability probe on APFS/ext4/NTFS/WSL plus NFS/SMB/cloud-sync negative fixtures. | Unknown means mutation-disabled, not optimistic WAL. |
| `KU-003` | Ref backend differences for packed refs and reftable. | Use Git plumbing across both backends; never parse ref files. | Reftable is unsupported only if conformance proves a missing necessary Git command. |
| `KU-004` | Exact semantics for submodules and sparse checkouts during return/pick. | Build explicit fixtures and decide capture/materialization policy per capability. | Fail before mutation with a capability-specific error; no partial support claim. |
| `KU-005` | Cross-platform preservation of child termination by signal. | Native process harness per OS; compare parent-observed result against direct Git. | Document platform-equivalent semantics precisely; never claim more. |
| `KU-006` | Checkpoint thresholds that preserve <50 ms orientation reads under long-running UI readers. | Benchmark 1k/100k/1m events with controlled readers and WAL growth. | Bound UI read transactions; checkpoint by measured bytes/time, not arbitrary event count. |
| `KU-007` | Whether remote event segments need signatures in addition to Git transport and hashes. | Threat model private/public/remapped remotes and actor authenticity needs. | v0.1 treats remote metadata as untrusted, preserves transport provenance, and makes no identity-authenticity claim. |

### 8.3 Unknown knowns recovered from the legacy implementation

| ID | Legacy evidence | Architectural lesson already applied |
|---|---|---|
| `UK-001` | Legacy `RepoData.version: 1` mixes states, lanes, mappings, navigation, freezes, and settings in one rewritten JSON value. | Separate immutable events from projections and operation control state. |
| `UK-002` | Legacy state records already distinguish JJK ID, Git commit, logical parent, branch/lane, tags, cherry provenance, deletion context, and prior contexts. | Migration must preserve semantic richness; a simpler table that drops these is regression. |
| `UK-003` | Legacy load performs external Git reconciliation and writes whole JSON. | Reconciliation becomes an explicit idempotent operation, never a surprising side effect of a read API. |
| `UK-004` | Legacy code includes worktree, return, exact pick, promotion, undo/redo, backup/load, and snake fixtures. | These are behavioral oracles and migration fixtures, not proof the architecture is already transactional. |
| `UK-005` | Legacy paths sometimes assume `.git` directory layout. | New design resolves Git common/worktree dirs through Git plumbing. |

### 8.4 Unknown unknown detection

The system cannot enumerate every future filesystem, Git backend, crash, forge, hook, or concurrent actor. It therefore carries these mandatory sensors:

1. before/after cross-layer fingerprints on every mutation;
2. immutable operation plans and phase records;
3. lock fencing and stale-publisher rejection;
4. independent event/projection digests and deterministic rebuild;
5. reachability proofs before ref deletion;
6. bounded parsers and quarantine for all remote/migration input;
7. capability snapshots that invalidate plans when adapters change;
8. startup scan for nonterminal operations;
9. stable structured error codes linked to playbooks;
10. append-only field log below.

**Divergence rule `ESC-DIVERGENCE`:** the moment observed reality cannot be classified by the prepared plan as exact before-state, intended after-state, or a documented conflict state, stop mutations, preserve all evidence and recovery material, append a field-log entry, and escalate. Do not improvise past a broken map.

---

## 9. Explicit escalation contract

A worker MUST stop rather than guess when any condition below holds.

| ID | Stop condition | Required escalation packet |
|---|---|---|
| `ESC-001` | Full typed ID maps to different payloads or repository IDs. | IDs, payload hashes, DB integrity result, sync provenance if any. |
| `ESC-002` | Recovery classifier is ambiguous or recovery material is missing/corrupt. | Operation ID, phase, plan digest, before/intended fingerprints, observed fingerprint, bundle manifest, attempted playbook. |
| `ESC-003` | Git object/ref required by provenance is missing and no explicit trusted source supplies the exact OID. | State/parent IDs, OIDs, ref observations, fetch attempts. |
| `ESC-004` | JJK and direct Git cannot agree on ref/head/index/worktree truth after one fresh observation. | Git version/backend/object format, raw plumbing outputs, JJK fingerprint, worktree list. |
| `ESC-005` | Storage probe or repeated projection rebuild is nondeterministic. | Filesystem identity, SQLite version/config, probe outputs, digests from each run. |
| `ESC-006` | Transparent passthrough differs in argv/cwd/env/stdio/signal/exit behavior. | Minimal differential tuple from `PB-016`, with secrets removed. |
| `ESC-007` | A canonical ref moved without a committed promotion or observed external Git operation. | Ref reflog/plumbing evidence, operation/event range, active fences, worktree occupancy. |
| `ESC-008` | Remote segment violates identity/hash/bounds or duplicates an event ID with another payload. | Quarantined object OID, segment manifest, expected/actual hashes, remote/ref; never the full secret-bearing environment. |
| `ESC-009` | Detach cannot prove post-delete reachability for every non-purged useful OID. | Dry-run plan, before/after reachability sets, proposed archive refs, collisions/missing objects. |
| `ESC-010` | A planned mutation would touch paths/refs/events outside its declared effect set. | Operation plan, unexpected delta, before/observed fingerprints, adapter versions. |

The escalation must state: **symptom; operation ID; invariant threatened; playbook consulted; actions attempted; exact observed evidence; whether any user-visible mutation occurred.** It must not contain a speculative fix disguised as fact. No worker may delete locks, recovery refs, operation sandboxes, migration backups, or quarantined sync objects merely to clear an error.

---

## 10. Acceptance gates

No v0.1 implementation is architecture-complete until all gates below have executable evidence. Passing a narrower unit test does not satisfy a real-surface gate.

| Gate | Acceptance check | Proof required |
|---|---|---|
| `AG-00` | TX-01 phase legality | State-machine/property test rejects skipped, reordered, duplicate-terminal, or stale-fence publication. |
| `AG-01` | Init import and idempotency | Existing/empty/unborn/bare/linked worktree, packed/reftable, merge graph, SHA-1/SHA-256 fixtures; direct Git before/after comparison. |
| `AG-02` | Transparent Git passthrough | Differential process harness across argv/cwd/env/stdio/TTY/pager/editor/hooks/credentials/signals/exits and partial failures. |
| `AG-03` | Capture fidelity | Tree/index/worktree matrix and concurrent mutation; negative assertion that user index and ignored policy did not change. |
| `AG-04` | Historical return | Exact clean/dirty bytes, preserved future refs/reachability, no branch until divergence, toggle/back/forward consistency. |
| `AG-05` | Parallel attempts | 32-writer provisioning stress, collisions, process death, manual worktree deletion/move, no shared filesystem identity. |
| `AG-06` | Exact pick | Snake “fast not purple” oracle plus modes/binary/deletes/symlink/case/submodule/object-format; full provenance assertion. |
| `AG-07` | Conflict containment | Target fingerprint unchanged through conflict and abort; continue publishes once; out-of-scope sandbox edits surfaced. |
| `AG-08` | Canonical promotion | Evidence content binding, checked-out dirty branch refusal, CAS race at every interval, exact rollback. |
| `AG-09` | Crash recovery | Process kill/fault at every TX-01 boundary for every v0.1 mutator; restart converges to exact before or after. |
| `AG-10` | Migration | Real legacy/corrupt/large/version-skew fixtures; byte-identical source; count/topology/reachability/raw-field ledger; interrupted switch. |
| `AG-11` | Remote sync | Offline two-device convergence, retries, push races, future types, malicious bounds, data-class leak scan, user-branch noninterference. |
| `AG-12` | Uninstall/detach | Git-only behavior, archive reachability, package-vs-repo separation, marker-scoped shell changes, crash/reinstall matrix. |
| `AG-13` | Event authority | Attempts to update/delete events fail; correction event works; projections rebuild to identical digest from journal alone. |
| `AG-14` | Storage safety | Local WAL kill/busy/checkpoint suite and NFS/SMB/cloud-sync refusal suite; bounded WAL under representative readers. |
| `AG-15` | Git-only/JJ parity | Same high-level result graph for Git-only and supported colocated JJ; adapter removal mid-plan invalidates safely. |
| `AG-16` | Performance without safety shortcuts | Warm status/current <50 ms, return/fork plan <100 ms, first graph paint <100 ms at 1k states on declared hardware fixtures; all durability checks enabled. |
| `AG-17` | Weak-reader runbook | A cold executor follows sampled playbooks to safe outcomes without architecture inference, hidden commands, or destructive improvisation. |

### 10.1 Release abort criteria

Release is blocked if any of these occur even once in the fault/conformance corpus:

1. staged, unstaged, untracked, or ignored user bytes are lost or silently changed outside policy;
2. a source future becomes unreachable after return;
3. exact pick imports an unrelated change;
4. two agents commit from the same mutable worktree identity;
5. an open conflict or nonterminal operation is reported as committed;
6. canonical ref moves without matching CAS evidence;
7. passthrough changes Git process behavior;
8. migration changes its source before switch;
9. remote input executes or mutates before validation;
10. detach removes the last reachability anchor for non-purged work;
11. repair guesses through ambiguous evidence;
12. WAL is enabled where its coherence probe did not pass.

---

## 11. Explicit non-goals for v0.1

| ID | Non-goal | Boundary reason | Safe behavior instead |
|---|---|---|---|
| `NG-001` | Distributed multi-host writes to one live JJK database. | SQLite WAL is same-host and single-writer; Git/files also lack shared transactionality. | Local DB per clone/device; immutable segment sync. |
| `NG-002` | Automatic semantic merge by an LLM on the exact-pick path. | Exact pick must be deterministic and auditable. | AI may propose separate semantic composition attempts in a later layer. |
| `NG-003` | Silent fuzzy selection for automation. | Ambiguity can select and mutate the wrong future. | Stable exact IDs or explicit candidate choice. |
| `NG-004` | Full Timeshift of secrets, arbitrary processes, terminal state, or editor state. | Adapter/privacy contracts are later phases. | v0.1 restores repository/worktree/control state and reports unsupported components honestly. |
| `NG-005` | JJK emulation of every Git flag. | Transparent passthrough must remain Git, not a second parser. | Direct native process invocation and later reconciliation. |
| `NG-006` | Automatic remote metadata publication. | Remotes may be public/untrusted and metadata may be private. | Sync is opt-in, scoped, previewable, and leak-scanned. |
| `NG-007` | Automatic resolution of canonical policy failures. | Promotion is governance, not cleanup. | Fail with stale/missing evidence and require replan/revalidation. |
| `NG-008` | Mutation on filesystem classes whose locking/durability are unknown. | Safety promise outranks convenience. | Read-only JJK plus normal Git until support is proved. |
| `NG-009` | Deleting legacy backups immediately after migration. | A migration is not proved by opening once. | Retain until explicit policy after verified operation history. |
| `NG-010` | Package uninstall deleting per-repository data. | Global tool lifecycle and project data lifecycle are distinct. | Separate `repo detach`, dry-run, export, and reachability proof. |
| `NG-011` | Preserving every machine-local absolute path in synced metadata. | Paths are nonportable and can leak user identity. | Sync relative logical identifiers; local bindings remain local. |
| `NG-012` | Treating advisory agent ownership as a security boundary. | Processes and humans can act outside JJK. | Worktree/ref isolation, lock fencing, fingerprints, reconciliation. |

---

## 12. Dead ends — do not rediscover these

| ID | Rejected approach | Why it fails | Use instead |
|---|---|---|---|
| `DE-001` | Copy or sync `state.sqlite3`, `-wal`, and `-shm`. | WAL is a live local protocol, not a portable event bundle; copying can omit committed WAL data or import host-local locks. | Hash-chained logical event segments. |
| `DE-002` | Rely on SQLite writer locking for whole-repo concurrency. | Git refs/worktrees/files/JJ mutate outside SQLite. | Outer repository lock with fencing plus short SQLite transactions. |
| `DE-003` | Hold JJK lock around transparent Git. | Editors, credentials, hooks, and recursive JJK calls can deadlock; passthrough stops being transparent. | Release lock during child; authoritative reconcile next operation. |
| `DE-004` | Serialize argv to a shell command string. | Loses native bytes, changes quoting/globbing/injection behavior, and breaks parity. | Native OS strings and direct process spawn/exec. |
| `DE-005` | Use current Git first-parent or merge-base for pick. | Imports too much or the wrong delta and violates remembered logical parent semantics. | Recorded parent tree → state tree. |
| `DE-006` | Resolve conflicts in the user's active target. | Half-applied state becomes visible and abort is destructive. | Isolated operation worktree and CAS publish. |
| `DE-007` | Mark metadata success before Git verification. | Crash creates event/ref dual truth. | Durable prepare, mutate, append, verify, then terminal commit. |
| `DE-008` | “Repair” by deleting lock/temp/ref files. | Destroys the only evidence/reachability needed to recover. | Phase classifier and pinned recovery bundle. |
| `DE-009` | Force promotion after stale-tip failure. | Overwrites external truth and invalidates evidence. | Refresh, revalidate, new CAS plan. |
| `DE-010` | In-place JSON→SQLite conversion. | Source corruption or partial migration has no trustworthy rollback. | Copy, verify, atomic pointer switch. |
| `DE-011` | Last-writer-wins synced labels/annotations. | Offline work silently disappears and causal plurality is lost. | Immutable events with explicit supersession/conflict. |
| `DE-012` | `rm -rf .jjk` as uninstall. | May remove the only refs/metadata proving valuable work and misses common-dir/worktree realities. | Reachability-aware detach transaction. |
| `DE-013` | Assume `.git` is a directory. | Linked worktrees/submodules use indirection; ref backend is not stable filesystem API. | Git plumbing and resolved common/worktree dirs. |
| `DE-014` | Enable WAL everywhere and tune away `SQLITE_BUSY`. | Hides unsupported filesystem/long-reader problems without fixing coherence. | Capability probe, bounded reads, health checks, fail closed. |

---

## 13. Field log contract

This section is append-only after implementation begins. Every unexpected real-world divergence gets one record; do not rewrite history to make the oracle appear complete.

```yaml
- field_id: FIELD-<uuidv7>
  observed_at_utc: <timestamp>
  version: <jjk version + git version + optional jj version>
  platform: <os/filesystem/ref backend/object format>
  operation_id: <typed id or null>
  symptom: <exact error code and observable behavior>
  threatened_invariants: [INV-...]
  expected_oracle_entry: <WG/PB/DE/ESC id consulted>
  observed_fingerprint: <digest/reference to doctor bundle>
  containment: <what was done without guessing>
  resolution: <verified move, or unresolved>
  proof: <command/artifact/result digest>
  architecture_change: <decision/gate/playbook changed, or none>
```

Re-wargame when any of these fires:

- one divergence event;
- one repeated incident class;
- a new Git/JJ/SQLite/ref-backend/filesystem capability;
- a migration format change;
- remote segment schema change;
- a release milestone;
- five accumulated field entries, even if individually resolved.

---

## 14. Completion check for this oracle

This wargame is decision-ready only if all answers remain “yes”:

- Does the sequential path cover init, external Git, capture, historical return, parallel agents, exact pick, conflict, promotion, crash, migration, remote sync, and uninstall?
- Does every step have nominal, loud-failure, lying-half-success, and improbable branches with detection and a pre-approved response?
- Is every cross-layer mutation governed by TX-01?
- Are Git/JJ/JJK identities separate and typed?
- Is SQLite WAL challenged by its actual same-host/single-writer/network-filesystem limits?
- Can every nonterminal operation be repaired without guessing?
- Does transparent Git preserve native argv, cwd, env, stdio, signals, and exit result?
- Are conflicts isolated and promotions CAS/evidence-gated?
- Do migration and detach preserve their source and reachability before switching/deleting?
- Are symptoms mapped to imperative operator moves?
- Are stop/escalation conditions stronger than the temptation to improvise?
- Are non-goals explicit enough to prevent v0.1 from pretending to solve later phases?

If any answer becomes “no” during implementation, the corresponding gate fails and the field log must record the divergence before work continues.

---

## Final chaser

I think the most dangerous moment is not the obvious crash. It is the clean-looking success immediately after one layer moved and another merely *said* it moved. Git, SQLite, JJ, and the worktree will each be individually reliable often enough to tempt an implementer into trusting the seam. Do not. Keep the durable prepare record boring, keep the verification independent, and make “I cannot classify this world” a proud stopping point rather than an invitation to repair by intuition. If the design ever becomes too complicated to explain from the exact bytes at risk, the exact refs expected to move, and the exact event that records why, stop adding features and simplify the transaction.

— Wargame planner, 2026-08-28

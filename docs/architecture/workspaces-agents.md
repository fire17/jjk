# Workspaces and Agents Architecture

Status: decision-grade architecture for JJK v0.1  
Scope: concurrent human and agent attempts, worktrees, ownership, coordination, handoff, validation, recovery, integration, shell directory handoff, and fleet observability

## 1. Context

JJK treats an **attempt** as the semantic line of exploration and a Git branch plus optional worktree as its interoperable substrate. Humans and agents need to explore concurrently without sharing a writable directory, racing a branch ref, corrupting JJK metadata, or losing abandoned work. Git remains the universal substrate, Jujutsu is optional, and JJK owns semantic identity, ownership, evidence, handoff, and recovery.

The current prototype proves that `fork --worktree` and shell-assisted directory changes are useful, but it has no durable worker identity, lease fencing, typed handoff, validation evidence, fleet view, or crash-safe cross-layer transaction. It also shares `.jjk` into linked worktrees with a filesystem symlink. The rewrite replaces that implicit sharing with a repository-common coordination store resolved from Git's common directory.

This design obeys the cross-layer mutation protocol:

> `discover -> lock -> reconcile -> resolve -> plan -> durable prepare -> mutate Git/JJ/files -> append events+projections -> verify -> commit/repair`

A successful command is not merely a successful process exit. It is a verified transition across Git/JJ/filesystem truth and the JJK journal. If the two sides disagree, the operation is `repair_required`, not success.

### 1.1 Vocabulary

| Term | Meaning |
|---|---|
| **Actor** | Stable human, agent, service, or recovery identity. |
| **Worker** | One live execution of an actor in a fleet. A restarted agent is a new worker even when its actor is unchanged. |
| **Attempt** | Semantic line of work rooted at an immutable JJK state. It may outlive every worker and workspace attached to it. |
| **Workspace** | Registered checkout context, normally one Git linked worktree, with a stable ID independent of its path. |
| **Lease** | Exclusive, renewable, fenced authority to mutate one managed resource. Expiration makes a lease suspicious/reclaimable; it does not silently transfer ownership. |
| **Scope claim** | Declared repository-relative output area and access mode for coordination and handoff checking. It is not a substitute for worktree isolation. |
| **Integration boundary** | The only declared place where work from multiple attempts may be combined, compared, or promoted. It pins source states and has one exclusive integrator target. |
| **Handoff** | Immutable typed offer to continue, review, integrate, or recover work. Acceptance atomically transfers authority when transfer is requested. |
| **Validation evidence** | Immutable observation tied to exact content, invocation, environment fingerprint, and outcome. |
| **Directory handoff** | Structured request for the caller or adapter to change directory. A child process never claims it changed its parent shell. |

### 1.2 Command classes

Every public command is documented as exactly one class:

1. **JJK-native**: operates on semantic JJK concepts and may be metadata-only, such as `jjk handoff show` or `jjk fleet status`.
2. **Git-enhanced**: performs Git/JJ/filesystem work through a durable JJK operation, such as attempt/worktree provisioning, checkpointing, integration, or promotion.
3. **Transparent Git passthrough**: `jjk git -- <args...>` executes Git without semantic enhancement and preserves the process contract exactly.

A JJK-native user intention may be implemented by a Git-enhanced command. The command reference must name the implementation class so automation knows whether Git state can change.

## 2. Decisions

### DEC-WA-001: One writer workspace per attempt

An active attempt has at most one writable primary workspace and that workspace has at most one active write lease. Parallel workers always receive child attempts with distinct branches and worktrees. Humans and agents who want to work side by side receive sibling attempts; they do not edit one directory concurrently.

Read-only review workspaces may coexist at pinned state IDs. Managed mutation commands reject a review workspace.

### DEC-WA-002: Isolation is structural, not etiquette

Each concurrent writer gets:

- a distinct `AttemptId`;
- a distinct standard Git branch;
- a distinct `WorkspaceId` and real directory;
- an exclusive workspace lease with a fencing generation;
- an exclusive branch-ref resource reservation;
- a declared owner and optional output scope.

No scheduler promise, branch-name convention, or “agents should coordinate” instruction is accepted as a correctness mechanism.

### DEC-WA-003: Attempts are durable; workers and paths are replaceable

An attempt is not owned by a PID, tmux pane, terminal tab, worktree path, or agent session. Those are observations attached to a worker/workspace generation. An attempt remains valid if the process dies, the path moves, or the harness disappears.

### DEC-WA-004: All convergence crosses an integration boundary

No worker merges directly into another active worker's branch or into a canonical branch. JJK first declares an `IntegrationBoundary`, pins immutable source state IDs, provisions one integration attempt/worktree, and grants one integrator an exclusive lease. Multiple semantic syntheses become sibling integration attempts rather than retries that overwrite each other.

### DEC-WA-005: Lease expiry never means deletion or automatic theft

A missed heartbeat changes health to `suspect` or `unreachable`. It never removes a worktree, branch, state, evidence artifact, handoff, or journal record. It never assigns the same directory to a new worker. Recovery requires confirmed death, an accepted handoff, or an explicit recovery plan.

When liveness is unknown, JJK may fork a new recovery attempt from the last captured state, but it must quarantine and preserve the old workspace. It may not adopt the old directory.

### DEC-WA-006: Fencing, not TTL alone, prevents split brain

Every lease has a monotonically increasing `generation` and an unlogged bearer token. Every managed mutation supplies both. Handoff, release/reacquire, or recovery increments the generation; requests from an old worker fail with `LEASE_FENCED` even if its process resumes.

The lease token is never stored in events, handoffs, logs, validation output, or fleet status. The database stores only its hash.

### DEC-WA-007: The authoritative coordination store is repository-common

All linked worktrees resolve the same local store using:

```text
git rev-parse --path-format=absolute --git-common-dir
<git-common-dir>/jjk/state.sqlite3
```

No worktree-local database and no `.jjk` symlink is authoritative. A worktree-local marker may contain only a non-secret `WorkspaceId` and store locator checksum; it cannot contain shared mutable state.

The default worktree home is:

```text
<registered-primary-root>/.worktrees/<attempt-slug>--<workspace-id-prefix>/
```

JJK adds `.worktrees/` to the repository's local exclude file by default, not the tracked `.gitignore`. A user may explicitly choose a tracked convention. If the registered primary root is unavailable, JJK uses a configured external worktree home; it never guesses a new primary root during mutation.

### DEC-WA-008: SQLite WAL remains the default, with a hard capability boundary

SQLite WAL is the default local coordination engine because it provides cross-process transactions, unique constraints, compare-and-swap updates, indexed fleet queries, and an atomic event-plus-projection commit inside a single-binary Rust product.

It is not treated as magic:

- SQLite WAL permits many readers but only one writer at a time.
- WAL requires processes to share a host and does not provide a safe multi-host network-filesystem coordination model.
- long readers can delay checkpoints;
- SQLite cannot atomically transact with Git refs, Git worktree administration, or ordinary files.

Therefore:

1. semantic events and projection updates share one short SQL transaction;
2. high-frequency heartbeats are coalesced into mutable presence rows rather than appended as product-history events;
3. lease lifecycle transitions remain durable events, while routine renewals do not bloat the journal;
4. reads are snapshot-bounded and never hold a transaction while running Git, validation, or process probes;
5. busy waits have a bounded deadline and return `COORDINATOR_BUSY`, never hang forever;
6. one elected checkpoint owner performs WAL checkpoints;
7. `jjk doctor storage --json` must pass a lock/WAL/filesystem capability probe before fleet mode is enabled;
8. an unsafe or indeterminate filesystem fails closed with `STORAGE_UNSAFE_FOR_FLEET`;
9. multi-host collaboration uses separate clones/worktrees and remote Git/JJK synchronization, or a single explicit coordinator service. Multiple hosts never write one WAL database directly.

A JSONL append file was rejected as the primary store because it cannot by itself enforce unique active leases, atomic event-plus-projection updates, efficient current fleet queries, or portable concurrent append semantics. A database per worktree was rejected because it creates conflicting truths. Git refs alone were rejected for ephemeral presence and lease CAS. SQLite is retained, but only inside its proven local-host envelope.

### DEC-WA-009: Durable operation intent bridges the Git/database transaction gap

Before any external mutation, JJK persists an operation record containing expected refs, expected workspace identity, intended changes, lock set, lease proofs, and repair strategy. Git ref updates use compare-and-swap (`update-ref <new> <expected-old>` or an equivalent transaction). If Git succeeds and the JJK event transaction fails, the prepared operation is sufficient to reconcile forward or produce an exact repair plan.

JJK never “fixes” an ambiguous partial operation by resetting or deleting work.

### DEC-WA-010: Shell directory changes are explicit handoffs

The standalone binary cannot change its parent shell's current directory. It returns a typed `DirectoryHandoff`; one of these consumers performs the transition:

- a shell integration function calls `builtin cd` after a successful command;
- command substitution uses `cd -- "$(jjk workspace enter <id> --print-path)"`;
- an agent harness sets the `cwd` of the next process;
- an editor/terminal adapter opens a new surface at the path;
- `jjk workspace shell <id>` starts a child shell there, without claiming the parent moved.

Without one of those consumers, JJK prints the path and stays in the caller's current directory.

### DEC-WA-011: Work is never automatically deleted

Completion, rejection, lease expiry, worker death, archive, quota pressure, and integration do not delete work. They update lifecycle state and ordinary visibility only. There is no background worktree garbage collector.

Physical removal exists only as the separately named, explicitly confirmed `workspace purge` ceremony. It requires a fresh preview, no active or uncertain lease, exact workspace-ID confirmation, and a retained state/freeze from which work can be restored. Branch deletion and attempt erasure are separate ceremonies; purging a checkout does not delete either.

### DEC-WA-012: Transparent Git passthrough is truly transparent

`jjk git -- <args...>` preserves:

- each argv element losslessly as an OS string/byte sequence;
- the caller's actual cwd;
- stdin, stdout, and stderr as inherited byte streams;
- the complete environment, without JJK-injected Git configuration;
- terminal/PTY attachment;
- signals and terminal process-group behavior;
- Git's exit code or signal termination.

On Unix the preferred implementation is `execvp` after resolving the Git executable, so no wrapper remains to distort signals or streams. Because an exec cannot append a post-command event, transparent passthrough is observed and reconciled by the next JJK command or watcher. It cannot be combined with `--json`; callers wanting structured JJK semantics use a Git-enhanced command instead.

## 3. Invariants

These invariants are construction requirements, not dashboard warnings.

| ID | Invariant | Enforcement |
|---|---|---|
| `INV-WA-001` | One active writable primary workspace per attempt. | Partial unique constraint on active `attempt_workspace` rows. |
| `INV-WA-002` | One active write lease per workspace and branch ref. | Unique resource lease plus generation CAS. |
| `INV-WA-003` | A Git branch is never attached writable to two managed worktrees. | Registry constraint, Git worktree reconciliation, and Git's own branch checkout guard. |
| `INV-WA-004` | Two managed workers never receive the same writable path or ref generation. | Atomic lease acquisition and fenced capability tokens. |
| `INV-WA-005` | Cross-attempt mutation requires an open integration boundary. | Integration API rejects unbound source/target pairs with `BOUNDARY_REQUIRED`. |
| `INV-WA-006` | An integration boundary has exactly one writable target workspace generation. | Boundary target lease uniqueness. |
| `INV-WA-007` | Boundary inputs are immutable state IDs and Git OIDs, never moving labels or branch tips. | Typed source pins and pre-mutation verification. |
| `INV-WA-008` | Canonical ref movement is compare-and-swap against the recorded previous tip. | Git ref transaction plus canonical resource lock. |
| `INV-WA-009` | Validation is valid only for the exact state, tree, workspace generation, invocation, and policy version it names. | Evidence-key equality in policy evaluation. |
| `INV-WA-010` | Handoff acceptance transfers authority atomically or not at all. | One SQL transaction: accept handoff, increment lease generation, change owner, issue new token hash, fence old token. |
| `INV-WA-011` | Labels, paths, PIDs, and branch names are not identities. | Typed stable IDs on all references. |
| `INV-WA-012` | Every external mutation has a durable prepare record before the first side effect. | Operation state-machine guard. |
| `INV-WA-013` | Events are append-only; corrections supersede or compensate. | No event update/delete API. |
| `INV-WA-014` | Event append and affected projection update are one SQLite transaction. | Store interface exposes one commit primitive only. |
| `INV-WA-015` | Reconciliation never overwrites unrecognized external Git/filesystem truth. | Stop with typed divergence and a repair plan. |
| `INV-WA-016` | Lease expiry alone cannot authorize adoption, deletion, or cleanup. | Recovery policy requires liveness proof or explicit quarantine/fork. |
| `INV-WA-017` | Dirty, untracked, ignored, conflicted, and uncommitted files are preserved during park/recovery. | No destructive Git command in those transitions; pre/post manifest verification. |
| `INV-WA-018` | Scope claims are checked against actual touched paths at every checkpoint and handoff. | Git-diff/path manifest comparison; violation blocks readiness/integration. |
| `INV-WA-019` | Missing adapters degrade visibly. | Capability field and stable `ADAPTER_UNAVAILABLE` result; no fake success. |
| `INV-WA-020` | Fleet status distinguishes observed liveness, lease authority, and progress. | Separate typed fields; no composite “active” guess. |
| `INV-WA-021` | Parent-shell cwd changes occur only in parent-controlled integration. | Standalone command returns a handoff and never claims otherwise. |
| `INV-WA-022` | No automatic lifecycle transition invokes physical deletion. | Purge is absent from worker/coordinator/recovery automation APIs. |

### 3.1 Collision guarantee boundary

For actors operating through JJK, `INV-WA-001` through `INV-WA-008` make concurrent physical and ref collisions unrepresentable. Separate attempts may intentionally edit the same logical files because their branches, indexes, and directories are isolated; that is plurality, not collision. Their changes can meet only at a declared integration boundary.

No user-space tool can prevent an uncooperative same-user process from opening another workspace path and writing files directly. JJK detects unmanaged changes during reconciliation and stops before overwriting them. Preventing malicious or bypassing OS-level writers is an explicit non-goal; never weaken this statement into an absolute filesystem claim.

## 4. Data and API shapes

All stable IDs are typed UUIDv7 values with a domain prefix. Examples: `att_...`, `wsp_...`, `wrk_...`, `lea_...`, `bnd_...`, `hnd_...`, `val_...`, `op_...`. Human labels are mutable and never accepted where an unambiguous ID is required by automation.

Repository paths and process arguments are lossless OS values, not assumed UTF-8:

```rust
struct EncodedOsValue {
    encoding: OsEncoding, // Utf8 | UnixBytesBase64 | WindowsUtf16LeBase64
    value: String,
}

struct RepoPath(EncodedOsValue); // validated relative, no root and no `..`

struct ProgramInvocation {
    program: EncodedOsValue,
    argv: Vec<EncodedOsValue>,
    cwd: WorkspaceRelativePath,
    stdin_mode: StdinMode,
    env: EnvironmentEvidence,
}
```

### 4.1 Actor, worker, attempt, and workspace

```rust
enum ActorKind { Human, Agent, Service, Recovery }
enum WorkerKind { HumanShell, AgentSession, HarnessJob, IdeSession, Service }
enum AttemptKind { Feature, Competing, Integration, Recovery, ExternalCandidate }
enum AttemptStatus {
    Planned, Active, HandoffReady, Validating, Candidate,
    Integrating, Integrated, Rejected, Parked, Abandoned
}
enum WorkspaceMode { WritablePrimary, ReadOnlyReview, Quarantined }
enum WorkspaceStatus {
    Provisioning, Ready, Occupied, Released, Parked,
    Orphaned, Missing, Quarantined, Purged
}

enum WorkerLifecycle {
    Registering, Ready, Working, WaitingValidation, HandoffReady,
    Integrating, Paused, Completed, Failed, Disconnected
}

enum LivenessState { ObservedAlive, Quiet, Suspect, ConfirmedDead, Unknown }

struct ActorRef {
    actor_id: ActorId,
    kind: ActorKind,
    display_name: String,
    adapter_subject: Option<String>,
}

struct Attempt {
    attempt_id: AttemptId,
    kind: AttemptKind,
    label: String,
    objective: String,
    root_state_id: StateId,
    current_state_id: StateId,
    branch_ref: FullGitRef,
    owner: ActorRef,
    parent_attempt_id: Option<AttemptId>,
    relation: Option<AttemptRelation>,
    scope_claims: Vec<ScopeClaim>,
    status: AttemptStatus,
    created_by_operation_id: OperationId,
    version: u64,
}

struct Workspace {
    workspace_id: WorkspaceId,
    attempt_id: AttemptId,
    mode: WorkspaceMode,
    path: EncodedOsValue,
    canonical_realpath_fingerprint: Hash,
    git_worktree_admin_id: String,
    branch_ref: Option<FullGitRef>,
    pinned_state_id: StateId,
    head_oid: GitOid,
    status: WorkspaceStatus,
    active_generation: u64,
    last_manifest: WorkspaceManifest,
    version: u64,
}

struct Worker {
    worker_id: WorkerId,
    actor: ActorRef,
    kind: WorkerKind,
    attempt_id: AttemptId,
    workspace_id: WorkspaceId,
    lifecycle: WorkerLifecycle,
    liveness: LivenessObservation,
    harness: Option<HarnessLocator>,
    objective: String,
    current_state_id: StateId,
    last_progress_at: Option<Timestamp>,
    last_heartbeat_at: Option<Timestamp>,
    active_handoff_id: Option<HandoffId>,
    active_validation_id: Option<ValidationRunId>,
    blockers: Vec<Blocker>,
    version: u64,
}
```

`AttemptRelation` is one of `child`, `competes_with`, `recovery_of`, `integration_of`, or `external_projection_of`, and always names stable IDs.

### 4.2 Scope claims

```rust
enum ScopeAccess { ReadOnly, ExclusiveOutput, GeneratedOutput }

struct ScopeClaim {
    claim_id: ScopeClaimId,
    attempt_id: AttemptId,
    access: ScopeAccess,
    pathspecs: Vec<RepoPathspec>,
    exclusions: Vec<RepoPathspec>,
    declared_by: ActorId,
    declared_at_state_id: StateId,
}
```

Scope claims communicate task ownership and catch accidental spill. They do not cause two attempts to share a directory. An actual touched path outside `ExclusiveOutput`/`GeneratedOutput` is recorded as a `ScopeViolation`, makes the handoff incomplete, and blocks integration until the scope is amended or the change is removed. Source reads are not restricted unless an external sandbox adds that policy.

### 4.3 Leases and fencing

```rust
enum ResourceKey {
    Workspace(WorkspaceId),
    Attempt(AttemptId),
    GitRef(FullGitRef),
    WorktreeAdmin(SafeSpaceId),
    IntegrationBoundary(BoundaryId),
    CanonicalRef(FullGitRef),
    Reconciliation(SafeSpaceId),
}

enum LeaseStatus { Active, Suspect, Expired, Released, Revoked, Fenced }

struct Lease {
    lease_id: LeaseId,
    resource: ResourceKey,
    holder_worker_id: WorkerId,
    generation: u64,
    token_hash: Hash,
    status: LeaseStatus,
    acquired_at: Timestamp,
    last_renewed_at: Timestamp,
    clock: LeaseClock,
    version: u64,
}

struct LeaseClock {
    coordinator_boot_id: String,
    monotonic_deadline_ns: u128,
    display_wall_deadline: Timestamp,
}

struct LeaseProof {
    lease_id: LeaseId,
    generation: u64,
    token: SecretBytes,
}
```

Embedded fleet mode is single-host, so monotonic time plus a boot ID determines expiry; wall time is display-only. After reboot, pre-reboot leases become `suspect`, not silently expired and transferable. Coordinator mode uses the coordinator's monotonic clock.

### 4.4 Typed handoff

```rust
enum HandoffPurpose { Continue, Review, Integrate, Recover }
enum HandoffStatus { Offered, Accepted, Declined, Superseded, Withdrawn }
enum AuthorityTransfer { None, AttemptAndWorkspace }

struct Handoff {
    handoff_id: HandoffId,
    revision: u32,
    purpose: HandoffPurpose,
    status: HandoffStatus,
    from_actor_id: ActorId,
    to_actor_id: Option<ActorId>,
    attempt_id: AttemptId,
    workspace_id: WorkspaceId,
    workspace_generation: u64,
    root_state_id: StateId,
    tip_state_id: StateId,
    tip_git_oid: GitOid,
    objective: String,
    change_summary: Vec<ChangeFact>,
    touched_paths: Vec<RepoPath>,
    scope_claim_ids: Vec<ScopeClaimId>,
    validation_run_ids: Vec<ValidationRunId>,
    remaining_work: Vec<RemainingWork>,
    risks: Vec<Risk>,
    blockers: Vec<Blocker>,
    resume: ResumeRecipe,
    reject: RejectRecipe,
    transfer: AuthorityTransfer,
    offered_at: Timestamp,
    version: u64,
}

struct ResumeRecipe {
    required_capabilities: Vec<Capability>,
    directory_handoff: DirectoryHandoff,
    invocations: Vec<ProgramInvocation>,
    first_expected_state_id: StateId,
}
```

A handoff never says only “done.” It states what changed, why, exact ancestry, evidence, remaining work, risk, and how to resume or reject. A correction creates a new revision and supersedes the old one. Acceptance verifies the tip and workspace generation are unchanged. Authority transfer occurs in the same transaction as acceptance; otherwise the handoff fails with `HANDOFF_STALE` and transfers nothing.

### 4.5 Validation evidence

```rust
enum CheckOutcome { Passed, Failed, Inconclusive, Cancelled, TimedOut }
enum ProcessTermination { ExitCode(i32), Signal(i32), Timeout, SpawnFailure }

struct ValidationRun {
    validation_run_id: ValidationRunId,
    attempt_id: AttemptId,
    state_id: StateId,
    git_tree_oid: GitOid,
    workspace_id: WorkspaceId,
    workspace_generation: u64,
    invocation: ProgramInvocation,
    tool_versions: Vec<ToolVersion>,
    platform: PlatformFingerprint,
    policy_id: ValidationPolicyId,
    policy_version: u64,
    started_at: Timestamp,
    finished_at: Timestamp,
    termination: ProcessTermination,
    outcome: CheckOutcome,
    assertions: Vec<AssertionResult>,
    stdout: OutputEvidence,
    stderr: OutputEvidence,
    recorded_by: ActorId,
    evidence_hash: Hash,
}

struct OutputEvidence {
    sha256: Hash,
    byte_count: u64,
    retained_artifact_id: Option<ArtifactId>,
    redacted_excerpt: Option<String>,
    truncated: bool,
}
```

Output is hashed while streaming. Raw output is not retained by default; bounded redacted excerpts are. `--retain-output` stores a content-addressed artifact under the configured evidence policy. Environment evidence records allowed variable names and hashes of non-secret values; secret values never enter the journal. A later state or changed tree makes earlier evidence visible but stale.

### 4.6 Integration boundary

```rust
enum BoundaryKind { DisjointCompose, OverlapCompose, CompetingCompare, Promotion }
enum BoundaryStatus { Declared, Ready, Integrating, Validating, Completed, Blocked, Abandoned }

struct SourcePin {
    attempt_id: AttemptId,
    state_id: StateId,
    git_oid: GitOid,
    handoff_id: HandoffId,
    validation_run_ids: Vec<ValidationRunId>,
}

struct IntegrationBoundary {
    boundary_id: BoundaryId,
    kind: BoundaryKind,
    source_pins: Vec<SourcePin>,
    target_attempt_id: AttemptId,
    target_workspace_id: WorkspaceId,
    integrator_actor_id: ActorId,
    allowed_paths: Vec<RepoPathspec>,
    conflict_policy: ConflictPolicy,
    required_validation_policy_id: ValidationPolicyId,
    expected_target_state_id: StateId,
    expected_target_ref_oid: GitOid,
    status: BoundaryStatus,
    version: u64,
}
```

Source workers may continue beyond a pinned source state; that does not change the boundary. Any newer source work requires a boundary revision or a new boundary.

### 4.7 Directory handoff

```rust
enum DirectoryAction { Stay, ChangeDirectory, OpenTerminal, OpenEditor, SpawnShell }

struct DirectoryHandoff {
    handoff_id: DirectoryHandoffId,
    action: DirectoryAction,
    workspace_id: WorkspaceId,
    absolute_path: EncodedOsValue,
    safe_space_id: SafeSpaceId,
    path_fingerprint: Hash,
    issued_at: Timestamp,
    expires_at: Timestamp,
    nonce: String,
}
```

The consumer re-resolves `workspace_id`, verifies the realpath fingerprint and safe-space registration, rejects a symlink escape, and consumes the nonce once. It never executes text embedded in the path.

### 4.8 Fleet snapshot

```rust
struct FleetSnapshot {
    schema: String, // "jjk.fleet/v1"
    safe_space_id: SafeSpaceId,
    projection_version: u64,
    observed_at: Timestamp,
    storage_capability: StorageCapability,
    workers: Vec<FleetWorkerRow>,
    attempts: Vec<FleetAttemptRow>,
    workspaces: Vec<FleetWorkspaceRow>,
    boundaries: Vec<FleetBoundaryRow>,
    alerts: Vec<FleetAlert>,
    source_freshness: SourceFreshness,
}

struct FleetWorkerRow {
    worker_id: WorkerId,
    actor: ActorRef,
    lifecycle: WorkerLifecycle,
    liveness: LivenessState,
    authority: LeaseSummary,
    attempt_id: AttemptId,
    workspace_id: WorkspaceId,
    current_state_id: StateId,
    dirty: WorkspaceDirtySummary,
    scope_health: ScopeHealth,
    validation: ValidationSummary,
    handoff: Option<HandoffSummary>,
    blocker_codes: Vec<String>,
    last_progress_at: Option<Timestamp>,
    last_heartbeat_at: Option<Timestamp>,
}
```

`lifecycle`, `liveness`, `authority`, and `last_progress_at` stay separate. A process can be alive but lease-fenced; a worker can be quiet but healthy; heartbeats do not prove useful progress.

## 5. Lifecycle protocols

### 5.1 Attempt lifecycle

```text
planned -> active -> handoff_ready -> validating -> candidate
   |          |             |             |           |
   |          +-> parked    +-> active    +-> active  +-> integrating -> integrated
   |          +-> rejected                             +-> rejected
   +-> abandoned
```

All terminal-looking states remain reversible visibility states. `rejected`, `abandoned`, and `integrated` preserve the branch, states, handoffs, evidence, and workspaces. `resume` creates a new lifecycle event and, when necessary, a new workspace generation.

### 5.2 Workspace lifecycle

```text
provisioning -> ready -> occupied -> released -> occupied
                    |         |            +-> parked
                    |         +-> orphaned -> adopted (confirmed dead only)
                    |                       +-> quarantined -> recovery fork
                    +-> missing -> repaired or quarantined

Any preserved state -> purged only by explicit purge ceremony
```

`released` means no worker currently holds authority; it does not remove the directory. `orphaned` means ownership exists but its worker is confirmed dead or gone. `missing` means the registered path is absent; it is evidence of an external action, not permission to delete the record or branch.

### 5.3 Worker lifecycle

```text
registering -> ready -> working -> waiting_validation -> handoff_ready -> completed
                    |      |              |                    |
                    |      +-> paused ----+                    +-> working (handoff declined)
                    +-> disconnected -> failed or reconnected
```

Liveness is orthogonal. A worker becomes `suspect` after missed renewal, `confirmed_dead` only from a trustworthy adapter/process observation, and `unknown` when the system cannot prove either.

### 5.4 Concurrent attempt creation

`jjk attempt create --from <state> --owner <actor> --worktree` performs:

1. discover the safe space, Git common directory, optional JJ capability, storage mode, worktree home, and actor adapter;
2. acquire ordered locks for reconciliation, the new attempt, new branch ref, and Git worktree administration;
3. reconcile Git refs, existing worktrees, workspace manifests, and pending operations;
4. resolve the source by stable state ID (interactive fuzzy resolution must finish before automation continues);
5. produce an exact plan with branch, path, source OID, scope, owner, and undo/repair route;
6. durably prepare `AttemptCreateOperation` with expected source/ref/worktree facts;
7. create the standard Git branch and linked worktree without changing the caller's current checkout;
8. append `AttemptCreated`, `WorkspaceProvisioned`, and initial lease events and update projections in one transaction;
9. verify `git worktree list --porcelain`, branch/HEAD OIDs, realpath, marker, database projection, and lease generation;
10. mark the operation committed or `repair_required`, release locks, and return a `DirectoryHandoff`.

Branch and directory names include an ID prefix, so two equal labels do not race on suffix selection.

### 5.5 Working and checkpointing

A worker mutates only its leased workspace. Before `step`, `save`, `nice`, branch update, or validation recording, JJK verifies:

- workspace and branch lease proofs;
- current lease generation;
- registered realpath and Git worktree identity;
- expected attempt/state/ref version;
- external Git/JJ changes through reconciliation;
- actual touched paths against scope claims.

A worker that changes files outside its scope is not erased or auto-reverted. It receives `SCOPE_VIOLATION`; the facts are recorded, readiness is blocked, and the owner chooses to amend scope, split the state, or remove the change.

### 5.6 Parking, rejection, completion, and archive

- `attempt park`: release authority if requested, retain everything, remove from default active view.
- `attempt reject`: record judgment and reason, retain everything and its graph position.
- `worker finish`: record worker outcome and optionally offer a handoff; never mutate attempt quality implicitly.
- `attempt integrated`: record boundary/promotion result; retain source attempts.
- `attempt archive`: move to archival views and optionally create a freeze; no physical deletion.
- `workspace release`: release lease only.
- `workspace purge`: explicit destructive ceremony described in Section 11.6.

Quota pressure blocks new provisioning with a ranked list of releasable/archivable/purgeable candidates. It never purges on behalf of the user.

## 6. Ownership and lease protocol

### 6.1 Ownership rules

1. `Attempt.owner` is the accountable actor, not necessarily the current executor.
2. `Worker` is the current executor and must hold lease proofs for managed writes.
3. A worker may own several attempts, but each concurrent attempt still has a distinct workspace and lease.
4. One attempt may have many historical workspaces, but only one writable primary workspace at a time.
5. A human reviewing an agent attempt uses a pinned read-only review workspace or accepts a typed handoff; JJK never silently makes both writers.
6. A fleet coordinator may provision attempts but does not become their semantic owner unless explicitly named.

### 6.2 Acquisition

Lease acquisition is an atomic SQL compare-and-swap on `(resource_key, generation, status)`. Success returns a token once. A conflicting request returns the current non-secret holder summary and `LEASE_HELD`; it never waits indefinitely.

Locks and leases are different:

- a **lease** is durable authority lasting across many commands;
- an **operation lock** is short-lived serialization for one transition;
- Git's own lockfiles are the final substrate guard;
- SQLite's writer lock protects one database commit.

Operation locks are acquired by canonical resource-key order to avoid deadlocks. No SQL transaction remains open while Git, filesystem, process, or validation work runs.

### 6.3 Renewal and presence

Workers renew at an adapter-configured cadence. Renewals update one coalesced presence row and the lease's monotonic deadline using generation CAS. Semantic events are emitted only for acquire, suspect, fence, transfer, release, reclaim, and revoke transitions.

A renewal failure never tells a worker to continue optimistically. The worker stops managed mutations, reports the error, and may keep its uncommitted files untouched until authority is restored.

### 6.4 Handoff transfer

For `AuthorityTransfer::AttemptAndWorkspace`:

1. recipient resolves the handoff and inspects the workspace read-only;
2. JJK verifies the offered tip, branch OID, workspace manifest, and generation;
3. one transaction accepts the handoff, changes owner, increments the generation, fences the old token, and installs the new token hash;
4. a new token is delivered only to the recipient;
5. post-transaction verification confirms the old proof fails and the new proof succeeds.

If any offered fact moved, acceptance returns `HANDOFF_STALE`. The sender must create a new handoff revision.

### 6.5 Reclaim

Reclaim has three paths:

| Liveness | Allowed recovery | Forbidden |
|---|---|---|
| `ConfirmedDead` | Fence generation; adopt existing workspace after a fresh manifest, or fork recovery attempt. | Deletion or reset. |
| `Unknown` | Quarantine old workspace; fork a new recovery attempt from last captured state; optionally take a stable read-only forensic snapshot. | Adopting or writing the old workspace. |
| `ObservedAlive` / `Quiet` | Request handoff, wait, or explicit administrative revoke that still quarantines the old workspace. | Automatic reclaim. |

PID checks include host boot ID and process start identity to defeat PID reuse. Harness session status is evidence only when the adapter can prove terminal exit. An unreachable remote adapter yields `Unknown`, not `ConfirmedDead`.

## 7. Integration boundaries

### 7.1 Boundary declaration

A boundary declaration names:

- exact source attempt/state/Git OID/handoff/evidence pins;
- boundary kind and allowed paths;
- one target integration attempt and workspace;
- one integrator actor;
- conflict policy;
- expected target state and ref OID;
- required validation policy;
- completion and rollback criteria.

A source handoff may be used by multiple competing integration boundaries. No source is consumed or deleted.

### 7.2 Disjoint composition

For disjoint scopes, JJK verifies actual touched-path manifests are disjoint before applying deltas. If they are not, it stops with `UNDECLARED_OVERLAP`; it does not silently switch to a merge strategy.

### 7.3 Overlap or semantic composition

Overlapping sources require `OverlapCompose`, `CompetingCompare`, or another explicit overlap-capable boundary. The boundary may produce multiple target attempts. Each candidate has its own workspace, instructions, provenance, validation, and comparison. “Best from A and B” is never modeled as deterministic set union.

### 7.4 Promotion

Canonical promotion is a separate `Promotion` boundary. It requires:

- exact candidate state and tree;
- evidence satisfying the named policy;
- approver/policy identity;
- canonical ref lease;
- expected previous canonical tip;
- atomic Git ref compare-and-swap;
- recorded rollback target.

If the canonical tip changes after planning, promotion fails with `EXTERNAL_REF_MOVED`; JJK replans rather than overwriting.

### 7.5 Source advancement during integration

Because sources are pinned states, source workers may continue. Fleet status shows a `source_advanced` note, but the integration remains reproducible. Updating to newer source states creates a boundary revision and invalidates evidence whose content keys changed.

## 8. Typed handoff protocol

### 8.1 Creating a handoff

`jjk handoff create` refuses a readiness claim unless:

- the worker holds current authority;
- the tip state and Git OID match the workspace;
- actual touched paths are captured;
- scope violations are absent or explicitly listed as blockers;
- all cited validation runs match exact content;
- remaining work and risks fields are present, even when empty;
- resume and reject recipes resolve without fuzzy lookup.

The handoff can still be created as `incomplete`; it simply cannot claim `candidate` or satisfy an integration policy.

### 8.2 Review handoff

A review handoff does not transfer write authority. JJK provisions or reuses a pinned read-only review workspace. Review annotations are new immutable events. If the reviewer wants to change code, JJK creates a child attempt or accepts an explicit ownership transfer.

### 8.3 Continue handoff

A continue handoff normally transfers the attempt and writable workspace. Acceptance fences the sender. The sender may then receive a new sibling attempt if it should continue exploring independently.

### 8.4 Integration handoff

An integration handoff is evidence for a boundary but does not grant access to the source workspace. The integrator receives immutable source pins and an exclusive target integration workspace.

### 8.5 Recovery handoff

A recovery handoff may be authored by the recovery service when a worker is confirmed dead. Facts it could not establish are marked `unknown`; they are never inferred as successful validation or completion.

## 9. Validation evidence

### 9.1 Recording

`jjk validation run --policy <id> -- <program> <args...>` is Git-enhanced because it reconciles the workspace before execution and appends evidence afterward. It does not create a state unless explicitly requested. The runner:

1. verifies lease and workspace generation;
2. captures state/tree/tool/platform/environment fingerprints;
3. runs with inherited terminal semantics unless capture mode was requested;
4. hashes output streams and retains only policy-authorized artifacts;
5. records termination and assertion outcomes;
6. verifies the workspace tree did not change unless the policy explicitly permits generated output;
7. appends immutable evidence.

A command exit code of zero is an observation, not automatically the truth of every assertion. Policies name the assertions they derive from exit code, artifacts, or probes.

### 9.2 Evidence freshness

Evidence is fresh only when all of these match:

```text
(state_id, git_tree_oid, workspace_id, workspace_generation,
 invocation_hash, toolchain_fingerprint, policy_id, policy_version)
```

Fleet status reports `fresh`, `stale`, `failed`, `inconclusive`, or `missing`. Stale evidence remains visible for provenance but cannot satisfy readiness or promotion.

### 9.3 External evidence

CI, forge, reviewer, and external harness results use adapters that map source identity, commit/tree OID, invocation, timestamps, and artifacts into the same record. An adapter that cannot prove the tested Git OID records `Inconclusive`.

## 10. Shell and directory handoff

### 10.1 Plain CLI

The plain command:

```sh
jjk attempt create --from sta_... --name parser --worktree
```

prints the new path and an honest instruction. It does not say “entered worktree.” The caller's cwd is unchanged.

For scripting:

```sh
cd -- "$(jjk workspace enter wsp_... --print-path)"
```

`--print-path` writes exactly one path plus a newline to stdout, writes diagnostics to stderr, and returns nonzero without printing a path on failure.

### 10.2 Shell integration

```sh
eval "$(jjk shell init zsh)"
# or: eval "$(jjk shell init bash)"
```

The installed shell function runs the real binary with inherited stdio and a secure per-invocation handoff file or dedicated descriptor. On exit zero it asks the binary to validate and consume the nonce, then calls `builtin cd -- <validated-path>`. It does not parse magic lines from mixed stdout/stderr, evaluate path text, or capture the command's interactive output in command substitution.

The wrapper preserves the binary's exit status. Interrupting the command does not change directory. Fish, PowerShell, Nushell, IDE, terminal, and harness integrations consume the same `DirectoryHandoff` schema through native adapters.

### 10.3 Agent harnesses

An agent adapter receives a `SpawnDirective`:

```rust
struct SpawnDirective {
    worker_id: WorkerId,
    attempt_id: AttemptId,
    workspace_id: WorkspaceId,
    cwd: EncodedOsValue,
    lease_capability_locator: SecretLocator,
    objective: String,
    scope_claims: Vec<ScopeClaim>,
    required_handoff_schema: String,
}
```

The harness must start the worker with this explicit cwd or reject the spawn. JJK never assumes an already-running agent changed directories because a child CLI printed a path.

### 10.4 Return path

A directory handoff may include a non-authoritative `previous_cwd` suggestion for the shell adapter. The shell owns its directory stack; JJK does not overwrite it. `jjk workspace leave` returns another validated handoff rather than mutating the parent directly.

## 11. Machine-readable command and API contract

### 11.1 Common envelope

All non-passthrough commands support `--format json`. Stdout contains exactly one versioned response; diagnostics are structured inside it. A non-success outcome also returns a nonzero process exit.

```json
{
  "schema": "jjk.command/v1",
  "request_id": "req_...",
  "operation_id": "op_...",
  "projection_version": 184,
  "outcome": "committed",
  "result": {},
  "warnings": [],
  "error": null
}
```

Requests made through the Rust library, CLI, stdio API, local coordinator socket, IDE, or harness use the same command enums and result structs. Mutations accept:

- `idempotency_key`;
- `if_projection_version` when a caller depends on a view;
- required `LeaseProof` values;
- explicit target IDs;
- `dry_run`/plan mode where supported.

Retries with the same idempotency key return the committed result or current repair state; they do not duplicate an attempt, worktree, handoff, or event.

### 11.2 Commands

| Command | Class | Machine result |
|---|---|---|
| `jjk attempt create --from <state-id> --name <label> --owner <actor-id> --worktree --scope <pathspec> --format json` | Git-enhanced | `Attempt`, `Workspace`, lease capability locator, `DirectoryHandoff`. |
| `jjk attempt park|resume|reject <attempt-id> --format json` | JJK-native, or Git-enhanced when provisioning is needed | New lifecycle event and projection. |
| `jjk workspace provision <attempt-id> --mode writable-primary|read-only-review --format json` | Git-enhanced | Workspace and directory handoff. |
| `jjk workspace inspect <workspace-id> --format json` | JJK-native observation | Registered vs observed manifest and divergence. |
| `jjk workspace enter <workspace-id> --print-path` | JJK-native | One validated path only. |
| `jjk workspace release <workspace-id> --lease <proof> --format json` | JJK-native | Released lease; files unchanged. |
| `jjk worker register --attempt <id> --workspace <id> --actor <id> --format json` | JJK-native | Worker and lease proofs/capability locator. |
| `jjk worker heartbeat <worker-id> --generation <n> --format json` | JJK-native | Renewed deadline or fencing error. |
| `jjk worker finish <worker-id> --outcome completed|failed|paused --format json` | JJK-native | Worker lifecycle event; no cleanup. |
| `jjk lease show|renew|release <lease-id> --format json` | JJK-native | Non-secret lease status. |
| `jjk lease reclaim <lease-id> --if-generation <n> --reason <text> --format json` | JJK-native recovery | Recovery plan or fenced new generation; never deletion. |
| `jjk handoff create --request @handoff.json --format json` | JJK-native | Immutable handoff revision. |
| `jjk handoff accept|decline <handoff-id> --if-version <n> --format json` | JJK-native | Atomic transfer or typed stale result. |
| `jjk validation run --policy <id> -- <program> <args...>` | Git-enhanced | `ValidationRun`. |
| `jjk validation record --request @evidence.json --format json` | JJK-native adapter input | Validated external evidence. |
| `jjk boundary declare --request @boundary.json --format json` | JJK-native | Boundary and target plan. |
| `jjk integration start|finish <boundary-id> --format json` | Git-enhanced | Integration operation and candidate state. |
| `jjk fleet status [--fleet <id>] --format json` | JJK-native observation | `FleetSnapshot`. |
| `jjk recover scan --format json` | JJK-native observation | Pending operations, stale leases, missing/divergent workspaces, suggested actions. |
| `jjk recover plan <subject-id> --action adopt|fork|park|reconcile --format json` | JJK-native | Immutable recovery plan with prerequisites. |
| `jjk recover apply <recovery-plan-id> --if-version <n> --format json` | Git-enhanced | Committed recovery or `repair_required`. |
| `jjk git -- <args...>` | Transparent Git passthrough | Git's own byte streams and process status; no JSON. |

`@file` is a CLI transport convenience. The API itself receives typed JSON and does not depend on files.

### 11.3 Stable error envelope

```json
{
  "code": "LEASE_FENCED",
  "message": "workspace lease generation 7 is no longer current",
  "subject_ids": ["wsp_...", "lea_..."],
  "retryable": false,
  "recovery_commands": [
    ["jjk", "handoff", "show", "hnd_...", "--format", "json"],
    ["jjk", "recover", "scan", "--format", "json"]
  ]
}
```

Recovery commands are arrays of lossless arguments in the API; rendered shell examples are display-only.

### 11.4 Fleet status CLI

Human table columns are stable but width-aware:

```text
WORKER  ACTOR  LIFE/HEALTH  ATTEMPT  WORKSPACE  LEASE  STATE  DIRTY  VALIDATION  HANDOFF  BLOCKER
```

`--format json` returns the complete schema. `--watch --format jsonl` emits a full initial snapshot followed by versioned deltas. Every observation carries source freshness; unavailable harness probes are `unknown`, never omitted or treated as healthy.

Fleet status may observe and suggest. It does not reclaim, kill, integrate, or purge.

### 11.5 Fleet provisioning boundary

JJK core provisions attempts/workspaces and emits `SpawnDirective` records. Harness adapters start and observe agent processes. A successful process spawn is `registering`, not `ready`; the worker becomes ready only after registering from the expected workspace and proving its lease capability.

JJK core does not synthesize harness-specific pane IDs into worker identity. Pane/session locators are replaceable adapter data.

### 11.6 Explicit purge ceremony

Physical deletion is intentionally absent from generic lifecycle and recovery commands.

```text
jjk workspace purge <workspace-id> --plan --format json
jjk workspace purge <workspace-id> \
  --execute <purge-plan-id> \
  --confirm-workspace <workspace-id> \
  --format json
```

The plan expires on any workspace, lease, manifest, ref, or projection change. Execution requires:

- workspace is released/parked and not `Unknown` liveness;
- no active or suspect lease;
- exact realpath is registered and outside the primary checkout;
- a retained state or verified freeze covers dirty/untracked work;
- Git reports the expected worktree admin entry;
- the user confirms the stable workspace ID, not a mutable path/label.

Purge removes only the checkout/admin entry named in the plan. It does not delete the attempt, branch, events, evidence, freeze, or source state.

## 12. Transaction and lock model

### 12.1 Operation record

```rust
enum OperationPhase {
    Discovered, Locked, Reconciled, Resolved, Planned, Prepared,
    SubstrateApplied, Recorded, Verified, Committed, RepairRequired, Aborted
}

struct OperationRecord {
    operation_id: OperationId,
    idempotency_key: String,
    command: CommandKind,
    phase: OperationPhase,
    actor_id: ActorId,
    worker_id: Option<WorkerId>,
    lease_generations: Vec<(ResourceKey, u64)>,
    lock_set: Vec<ResourceKey>,
    expected: ExpectedWorld,
    plan: MutationPlan,
    observed_after: Option<ObservedWorld>,
    repair: RepairStrategy,
    created_at: Timestamp,
    updated_at: Timestamp,
    version: u64,
}
```

### 12.2 Exact protocol

Every mutating command runs the following stages and records transitions:

1. **discover**: resolve safe space, common store, adapters, storage capability, actor, current workspace;
2. **lock**: verify durable lease proofs and acquire short operation locks in sorted resource order;
3. **reconcile**: import external Git/JJ/ref/worktree facts idempotently and resolve pending earlier operations that touch the lock set;
4. **resolve**: convert explicit IDs/queries into typed immutable targets; automation never silently chooses an ambiguous fuzzy match;
5. **plan**: compute exact ref/path/event/projection changes and return preview when destructive, ambiguous, or policy-gated;
6. **durable prepare**: commit `OperationRecord::Prepared` with expected old values, intended new values, and repair strategy;
7. **mutate Git/JJ/files**: apply idempotent adapter actions; refs use expected-old CAS; filesystem writes use same-directory temp plus atomic rename where applicable;
8. **append events+projections**: in one short SQLite transaction append typed events and update all affected materialized views;
9. **verify**: independently re-read Git refs, worktree porcelain, HEAD/tree/index/dirty manifest, optional JJ state, events, and projections;
10. **commit/repair**: mark `Committed` only if verification matches. Otherwise mark `RepairRequired`, preserve all work, release locks, and print exact recovery commands.

### 12.3 Lock granularity

Unrelated attempts can checkpoint concurrently at the Git/filesystem layer. The lock manager uses the narrowest correct set:

- attempt checkpoint: workspace, attempt, and its branch ref;
- worktree provision/remove: Git worktree administration plus new workspace/ref;
- reconciliation: a short repository observation lock, then relevant resource locks;
- handoff transfer: handoff, attempt, workspace, and lease resources;
- integration: boundary, target workspace/attempt/ref, and immutable read pins;
- promotion: boundary, target attempt, and canonical ref.

SQLite still serializes each short write transaction. No global repository lock is held while an agent edits or a validation suite runs.

## 13. Fleet health and stale/dead recovery

### 13.1 Health derivation

Fleet health derives from independent sources:

- last lease renewal and generation;
- local PID plus boot/start identity, when local;
- harness adapter status, when trustworthy;
- workspace manifest and mtime/change digest;
- Git worktree/branch/HEAD observation;
- last meaningful state/progress event;
- active handoff, validation, and boundary blockers.

Suggested states:

| Condition | Liveness | Authority | Fleet presentation |
|---|---|---|---|
| Heartbeats current; adapter alive | `ObservedAlive` | current | healthy/working or healthy/quiet |
| Heartbeats late; adapter alive | `Quiet` or `Suspect` | current until fenced | quiet; inspect, do not reclaim |
| Lease deadline passed; adapter unavailable | `Unknown` | retained, not transferable | unreachable; recovery fork only |
| Adapter proves process exited | `ConfirmedDead` | reclaimable via explicit plan | orphaned; adopt/fork options |
| Process alive with old generation | `ObservedAlive` | fenced | split-brain attempt blocked; quarantine old workspace |
| Workspace changed with no current worker proof | `Unknown` | retained | unmanaged writer alert; stop mutations |

### 13.2 Recovery cases

| ID | Observed case | Required behavior | Recovery command path |
|---|---|---|---|
| `REC-WA-001` | Crash after prepare, before branch/worktree creation. | Verify no substrate effects; append abort/repair event; retain operation receipt. | `recover plan <op> --action reconcile`, then `recover apply`. |
| `REC-WA-002` | Branch/worktree created, events absent. | Match exact prepared branch/path/OID; append missing events and projections if unambiguous. Otherwise quarantine. | Reconcile operation. |
| `REC-WA-003` | Events say provisioned, worktree path missing. | Mark workspace `Missing`; preserve attempt/ref/events; never recreate or purge silently. | `recover plan <wsp> --action fork` or explicit repair to a new path. |
| `REC-WA-004` | Worker stopped heartbeating; liveness unknown. | Keep ownership, mark unreachable, quarantine original for write purposes, allow only recovery fork from last captured state. | `recover plan <wrk> --action fork`. |
| `REC-WA-005` | Worker confirmed dead; workspace clean. | Fence generation; adopt same workspace or fork, according to explicit plan. | `recover plan <wrk> --action adopt|fork`. |
| `REC-WA-006` | Worker confirmed dead; workspace dirty/untracked. | Preserve exact directory; record manifest; adopt after fencing or create a freeze then recovery attempt. No reset/stash/delete. | Adopt or forensic/freeze recovery plan. |
| `REC-WA-007` | Old worker returns after handoff/reclaim. | Reject all managed writes with `LEASE_FENCED`; do not modify its files; show current owner and handoff/recovery route. | New sibling attempt or owner-mediated handoff. |
| `REC-WA-008` | Old fenced process continues direct file writes. | Mark workspace unmanaged/divergent and quarantine; current owner moves to a new recovery workspace. Never race in the old directory. | `recover plan <wsp> --action fork`. |
| `REC-WA-009` | External Git moves attempt branch. | Import observation, block planned write, preserve both tips, require adopt/fork/rebase decision. | Reconcile, then explicit repair plan. |
| `REC-WA-010` | Integration changes target ref, event append fails. | Use prepared expected/new OIDs. Reconcile forward if exact; rollback only by CAS and only if no third-party move; otherwise repair-required. | Recover operation. |
| `REC-WA-011` | Database committed, post-verify fails. | Keep events; mark operation repair-required; compare actual substrate before compensating event. | Recover operation. |
| `REC-WA-012` | Storage/WAL capability becomes unsafe. | Stop new fleet mutations; permit status/export/backup; never silently downgrade concurrency guarantees. | `doctor storage`, migrate to local/coordinator mode. |
| `REC-WA-013` | Handoff accepted while sender changed tip. | CAS fails; transfer nothing; retain offered handoff as stale provenance. | Sender issues new revision. |
| `REC-WA-014` | Validation interrupted. | Record `Cancelled`, `TimedOut`, or termination signal against original content; it satisfies no success policy. | Rerun exact validation. |
| `REC-WA-015` | Git worktree metadata is stale but directory exists. | Do not run automatic `git worktree prune`; inspect directory/marker/HEAD and create a repair plan. | Workspace reconcile plan. |
| `REC-WA-016` | Primary root/worktree home moved. | Resolve by WorkspaceId and fingerprint; refuse path guess; allow explicit relocate plan. | `recover plan <wsp> --action reconcile`. |
| `REC-WA-017` | Coordinator/reboot invalidates monotonic lease clocks. | Mark prior leases suspect, probe liveness, and require handoff/recovery. | Fleet status then explicit recovery. |

### 13.3 Forensic snapshot under unknown liveness

JJK may take a read-only forensic snapshot of an unreachable workspace only by reading twice and verifying a stable manifest. It records the interval and result as `Inconclusive` if files changed during capture. It does not claim this is a coherent checkpoint and does not mutate the source workspace.

## 14. Failure modes

| Code | Failure | Safe response |
|---|---|---|
| `LEASE_HELD` | Another worker holds the resource. | Return holder summary and handoff request route; do not wait forever. |
| `LEASE_FENCED` | Caller supplied an old generation/token. | Stop mutation; show current generation/owner without exposing token. |
| `LIVENESS_UNKNOWN` | Death cannot be proven. | Quarantine and recovery-fork; no adoption. |
| `WORKSPACE_IDENTITY_MISMATCH` | Path, marker, Git admin ID, or fingerprint differs. | Stop; inspect and repair explicitly. |
| `WORKSPACE_MISSING` | Registered path absent. | Preserve metadata/ref; mark missing. |
| `UNMANAGED_WRITER` | Files/ref moved without current managed authority. | Reconcile facts, quarantine conflicting workspace, never overwrite. |
| `SCOPE_VIOLATION` | Touched output falls outside declared claim. | Record/block readiness; owner resolves. |
| `BOUNDARY_REQUIRED` | Cross-attempt combine requested without a boundary. | Return boundary declaration template. |
| `UNDECLARED_OVERLAP` | Disjoint boundary sources overlap. | Stop and require overlap-capable boundary or source cleanup. |
| `EXTERNAL_REF_MOVED` | Expected Git ref old OID no longer matches. | Preserve tips; re-resolve/replan. |
| `EVIDENCE_STALE` | Evidence key does not match candidate. | Rerun or record matching evidence. |
| `HANDOFF_STALE` | Tip/workspace generation changed after offer. | Transfer nothing; issue new revision. |
| `COORDINATOR_BUSY` | Bounded SQLite/operation-lock wait expired. | Return retryable error with backoff hint. |
| `STORAGE_UNSAFE_FOR_FLEET` | WAL/locking/multi-host safety cannot be established. | Refuse fleet mutations; use separate clones or one coordinator. |
| `ADAPTER_UNAVAILABLE` | Harness/JJ/shell/IDE capability missing. | Degrade explicitly and offer supported alternative. |
| `REPAIR_REQUIRED` | Substrate and JJK truth differ after a prepared operation. | Preserve all work; expose exact expected/observed values and recovery plan. |
| `PATH_UNSAFE` | Handoff path escapes registered workspace or changed by symlink. | Refuse directory transition. |
| `PURGE_PRECONDITION_FAILED` | Purge plan stale, lease uncertain, or work not retained. | Delete nothing; produce new preview after remediation. |

### 14.1 Blast-radius rules

- A workspace failure cannot move another attempt's ref.
- An attempt failure cannot move a canonical ref.
- A boundary failure cannot mutate source attempts.
- A coordinator/store failure stops semantic mutation but leaves normal Git repositories and branches usable.
- A missing optional JJ/harness/shell adapter cannot make Git-only work invalid.
- Repair never reaches outside the prepared operation's resource set.

## 15. Acceptance checks

Each check has an observable pass criterion and must run in Git-only mode; JJ-enabled variants are additional, not substitutions.

| ID | Check | Pass criterion |
|---|---|---|
| `CHK-WA-001` | Concurrent provision race. | Many simultaneous creates with equal labels produce unique attempt IDs, refs, paths, and leases; no shared writable path/ref. |
| `CHK-WA-002` | Same-workspace lease race. | Exactly one claimant receives generation/token; every loser receives `LEASE_HELD`. |
| `CHK-WA-003` | Fencing. | After handoff/reclaim, every mutation with the old proof fails and the workspace remains unchanged. |
| `CHK-WA-004` | Human/agent side-by-side. | Human and agent receive sibling attempts/worktrees; each can checkpoint without changing the other's HEAD/index/files. |
| `CHK-WA-005` | Shared-output attempt. | Cross-attempt combine without a boundary fails; declared boundary provisions one exclusive integration target. |
| `CHK-WA-006` | Multiple semantic syntheses. | Two “best of A/B” candidates remain independent sibling attempts with complete source pins and evidence. |
| `CHK-WA-007` | Scope spill. | Out-of-scope touched path is detected at checkpoint/handoff and blocks readiness without reverting the file. |
| `CHK-WA-008` | External Git writer. | An out-of-band ref/file change is imported or blocks with divergence; JJK never overwrites it. |
| `CHK-WA-009` | Crash matrix. | Fault injection after every transaction phase yields committed, safely aborted, or repair-required state; no unaccounted dual truth. |
| `CHK-WA-010` | Dead clean worker. | Confirmed-dead workspace can be explicitly adopted with a higher generation; content/HEAD unchanged. |
| `CHK-WA-011` | Dead dirty worker. | Dirty/untracked/ignored files survive detection, recovery planning, and adoption/fork. |
| `CHK-WA-012` | Unknown worker. | Unknown liveness cannot adopt or purge old workspace; recovery fork leaves it untouched. |
| `CHK-WA-013` | Returned old worker. | Managed mutation is fenced; direct-write detection quarantines rather than races. |
| `CHK-WA-014` | Typed handoff CAS. | Concurrent tip change makes acceptance fail atomically with no partial owner/lease transfer. |
| `CHK-WA-015` | Evidence binding. | Any state/tree/generation/invocation/policy change marks evidence stale. |
| `CHK-WA-016` | Canonical promotion race. | Only expected-old CAS succeeds; concurrent canonical change preserves both tips and blocks promotion. |
| `CHK-WA-017` | Plain cwd honesty. | Running standalone worktree creation leaves parent cwd unchanged and reports a directory handoff. |
| `CHK-WA-018` | Shell cwd handoff. | Installed wrapper changes cwd only after success and validated nonce/path; failure or signal leaves cwd unchanged. |
| `CHK-WA-019` | Harness cwd. | Spawned worker proves its registered cwd/workspace before becoming `ready`; wrong cwd is rejected. |
| `CHK-WA-020` | Git passthrough conformance. | Non-UTF-8 argv (where supported), cwd, binary stdin/stdout/stderr, env, PTY, exit codes, and signals match direct Git. |
| `CHK-WA-021` | Fleet truth. | JSON reports lifecycle, liveness, lease authority, progress time, dirty status, validation, handoff, and freshness separately. |
| `CHK-WA-022` | No cleanup by lifecycle. | Complete/reject/archive/expire/crash/quota operations remove zero directories, branches, states, or artifacts. |
| `CHK-WA-023` | Purge ceremony. | Stale/uncertain/dirty-unretained plans delete nothing; valid explicit plan removes only named checkout and retains attempt/ref/events. |
| `CHK-WA-024` | Common-store resolution. | Commands from primary and linked worktrees see one journal/projection without a shared-store symlink. |
| `CHK-WA-025` | WAL stress. | Representative worker count sustains short writes without corruption/starvation; bounded contention returns typed retryable errors. |
| `CHK-WA-026` | Unsafe storage. | Simulated network/indeterminate filesystem refuses fleet mode rather than silently weakening guarantees. |
| `CHK-WA-027` | Uninstall/interoperability. | With JJK absent, every retained attempt remains a valid standard Git branch/worktree and normal Git/GitHub flows work. |
| `CHK-WA-028` | Machine idempotency. | Repeating a request ID after timeout returns the original result and creates no duplicate resources/events. |
| `CHK-WA-029` | Event/projection consistency. | Rebuilding projections from events yields the same attempt/workspace/handoff/evidence topology, excluding explicitly ephemeral presence fields. |
| `CHK-WA-030` | Secret exclusion. | Lease tokens and secret env values are absent from journal, status, handoffs, logs, evidence excerpts, and crash reports. |

## 16. Explicit non-goals

1. Preventing a malicious or uncooperative same-user process from directly editing another workspace at the operating-system level. JJK detects and contains; OS sandboxing is an adapter concern.
2. Letting two writers share one worktree, index, or branch ref because they promised to touch different files.
3. Automatically killing a stale worker, revoking a live worker, or assuming a missed heartbeat means death.
4. Automatically deleting, pruning, resetting, cleaning, stashing, or garbage-collecting user work.
5. Pretending a subprocess changed its parent shell's cwd.
6. Running one embedded SQLite WAL database from multiple hosts or over an unsafe network filesystem.
7. Replacing Git branches, commits, worktrees, remotes, PRs, or CI as the universal substrate.
8. Requiring Jujutsu. JJ acceleration and operation-log support remain capability-gated.
9. Treating semantic composition as an automatic deterministic merge.
10. Inferring completion from process exit, heartbeat, token use, or an agent's prose claim.
11. Storing whole agent transcripts, terminal history, arbitrary environment contents, or secrets as routine handoff metadata.
12. Providing a general distributed job scheduler. JJK supplies attempt/workspace/lease/handoff contracts and harness adapters; harnesses schedule compute.
13. Making labels, paths, branch names, PIDs, panes, or session IDs durable identities.
14. Hiding raw Git/JJ effects from experts. Every Git-enhanced plan and result remains inspectable.
15. Silently selecting a fuzzy state, owner, workspace, recovery action, or integration candidate in automation.

## 17. Implementation boundary

The Rust core should expose these boring, testable ports without coupling to a specific CLI, TUI, IDE, or agent harness:

```rust
trait CollaborationStore {
    fn transact(&self, command: StoreCommand) -> Result<StoreCommit, StoreError>;
    fn fleet_snapshot(&self, query: FleetQuery) -> Result<FleetSnapshot, StoreError>;
    fn rebuild_projections(&self) -> Result<ProjectionDigest, StoreError>;
}

trait WorkspaceAdapter {
    fn observe(&self, id: WorkspaceId) -> Result<WorkspaceObservation, AdapterError>;
    fn provision(&self, plan: &WorkspacePlan) -> Result<WorkspaceObservation, AdapterError>;
    fn remove_explicit(&self, plan: &VerifiedPurgePlan) -> Result<(), AdapterError>;
}

trait GitAdapter {
    fn observe_refs_and_worktrees(&self) -> Result<GitObservation, AdapterError>;
    fn apply(&self, plan: &GitMutationPlan) -> Result<GitObservation, AdapterError>;
}

trait HarnessAdapter {
    fn spawn(&self, directive: &SpawnDirective) -> Result<HarnessLocator, AdapterError>;
    fn observe(&self, locator: &HarnessLocator) -> Result<LivenessObservation, AdapterError>;
}

trait DirectoryHandoffConsumer {
    fn validate(&self, handoff: &DirectoryHandoff) -> Result<ValidatedDirectory, HandoffError>;
}
```

Only `VerifiedPurgePlan` can reach physical workspace removal. There is no generic `delete(path)` port in collaboration/recovery code. That type boundary makes automatic deletion unavailable by construction.

The coordinator, CLI, shell plugin, TUI, IDE, and agent adapters all consume the same typed commands and projections. Fleet rendering is a view over `FleetSnapshot`, not a second source of truth. This keeps Git universal, JJ optional, and JJK the semantic state and collaboration layer.
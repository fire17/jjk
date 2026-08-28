# Git CLI, `gix`, and optional Jujutsu adapters

**Status:** decision-grade architecture for JJK v0.1  
**Scope:** repository discovery, Git/JJ interoperability, local and remote effects, reconciliation, degradation, and conformance  
**Normative words:** MUST, MUST NOT, SHOULD, and MAY are requirements.

## 1. Context

JJK is the semantic state layer. Git remains the universal object, ref, worktree, remote, and forge substrate. Jujutsu (JJ) is an optional local history engine. A repository without JJ, a collaborator without JJK, and CI with only Git MUST continue to see ordinary valid Git history.

This boundary has three very different jobs:

1. **Native Git CLI** supplies Git-compatible behavior and is the authority for repository discovery, configuration, index/worktree semantics, mutations, remotes, credential helpers, and passthrough.
2. **`gix`** is an in-process, read-only accelerator for immutable object and graph reads that have passed differential conformance. It is never an independent mutation implementation in v0.1.
3. **JJ CLI** supplies optional change IDs, operation-log facts, and selected local-history operations in a verified colocated repository. It is an accelerator, not a requirement and not a second source of Git truth.

The old implementation established useful behavior but also exposed boundaries the rewrite MUST remove: ignored failures from `jj git import/export`, fixed SHA-1 constants, shell-based executable discovery, staging the user's real index to capture a state, assuming an `origin` remote, and treating import/export as side effects rather than recorded facts.

### 1.1 Command classes

Every user-visible command is exactly one class:

| Class | Definition | Reconciliation | Output/exit contract |
|---|---|---|---|
| **JJK-native** | Implements semantic operations such as `save`, `return`, `pick`, `fork`, or `promote` through the JJK transaction protocol. | Required before planning and after effects. | JJK owns rendering and exit codes. |
| **Git-enhanced** | An explicitly named JJK operation that invokes Git but adds a declared semantic effect, such as `jjk sync fetch` or `jjk worktree create`. | Required; added effects appear in plan and receipt. | JJK owns rendering; native Git diagnostics are preserved as structured causes. |
| **transparent Git passthrough** | Any top-level argv sequence not claimed by the versioned JJK-native/enhanced registry executes Git without semantic additions; `jjk git -- <argv…>` is the explicit collision-proof form. | None in the passthrough process. The next JJK-native/enhanced command reconciles what Git changed. | Exact native argument, cwd, stdio, environment, signal, and exit behavior defined in §7. |

JJK MUST NOT describe a passthrough command as enhanced, or silently add hooks, imports, network calls, status scans, color flags, pagers, config, or reconciliation to it.

## 2. Decisions

### AD-GIT-001 — Git CLI is the compatibility oracle

The installed Git CLI is mandatory for mutating a Git repository in v0.1. JJK invokes it without a shell, with typed argument vectors, machine-readable `-z`/porcelain formats where available, explicit `--` path boundaries, and locale-independent parsing only where Git lacks a structured protocol.

**Why:** Git configuration includes attributes, ignore rules, filters, hooks, credential helpers, alternates, worktree-local state, repository extensions, and platform behavior. Reimplementing this surface would spend JJK's innovation budget below its semantic moat.

**Consequence:** Git version/capability probing is behavioral, not a single minimum-version comparison. A missing primitive disables only operations that require it. JJK never parses human-oriented `git status`, `git branch`, or `git log` output.

### AD-GIT-002 — `gix` is read-only and disposable

`gix` MAY implement these reads after fixture parity is green for the discovered repository format:

- decode commit/tree/tag objects by full `GitOid`;
- walk commit ancestry from already resolved OIDs;
- compute tree/diff summaries whose output is checked against Git fixtures;
- read object headers and content needed for graph projections.

`gix` MUST NOT in v0.1:

- update refs, HEAD, the index, config, reflogs, worktrees, or remotes;
- decide ignore/filter/attribute behavior;
- replace `git status`, merge machinery, credential handling, hooks, or transport;
- read a repository format or extension that discovery did not positively approve.

Every `gix` read has the same typed result as its Git CLI fallback. On open, decode, unsupported-format, or parity-check failure, JJK records `CapabilityDegraded{component:gix,…}` and reruns the read through Git CLI. No semantic feature becomes unavailable merely because `gix` is disabled.

### AD-GIT-003 — Git-only is the normative mode

The core semantic contract and golden outcomes are defined in Git-only mode. JJ mode MUST produce the same JJK states, logical edges, Git commits/trees, branch destinations, workspace outcome, and preservation guarantees unless an operation is explicitly advertised as JJ-only. Optional JJ identity and operation facts may differ.

JJ is never auto-installed. Existing repositories are never silently converted. Enabling uses an explicit action (`jjk init --with-jj` or `jjk jj enable`) whose plan identifies colocation changes. Disabling JJ leaves a valid Git repository and retained JJK metadata.

### AD-GIT-004 — only verified colocation enables the JJ adapter

The JJ adapter is enabled for a workspace only when all checks pass:

1. the `jj` executable is found without a shell and its version/probes succeed;
2. the nearest `.jj` workspace is discovered by JJ itself;
3. `jj git root --ignore-working-copy` resolves to the same underlying Git common repository discovered by Git;
4. Git is operable from the same workspace;
5. JJ's backend/object-format probe supports this repository;
6. a non-mutating operation-log observation succeeds;
7. the workspace mapping is unambiguous.

A non-colocated JJ repository is reported but not used by JJK v0.1. A `.jj` directory alone is never proof. Symlink/case-normalized filesystem identities, not display strings, are compared.

### AD-GIT-005 — effects are durable but not cross-system atomic

No SQLite transaction spans Git, JJ, filesystem, subprocess, or network mutations. Git multi-ref transactions, when available, improve ref consistency but do not make Git+JJ+JJK atomic; even Git documents that concurrent readers may observe a subset of a multi-ref update. JJ operation-log safety does not include JJK metadata. Remote `push --atomic` is used only when requested by a JJK-native plan and advertised by the server; it does not include local metadata.

JJK therefore uses durable intent, precondition fingerprints, idempotent effects, observation, verification, and repair. It MUST never claim atomicity beyond the primitive actually observed.

### AD-GIT-006 — object IDs are algorithm-tagged

No OID is an untyped hex string. SHA-1 length, a 40-zero sentinel, and the SHA-1 empty-tree constant MUST NOT appear in domain logic. Empty trees and zero OIDs are obtained from the discovered algorithm/native Git.

```rust
pub enum ObjectAlgorithm { Sha1, Sha256 }

pub struct GitOid {
    pub algorithm: ObjectAlgorithm,
    pub bytes: Box<[u8]>, // construction enforces 20 or 32 bytes
}

pub struct GitParentFact {
    pub child_oid: GitOid,
    pub parent_oid: GitOid,
    pub parent_index: u32, // ordered exactly as the commit encodes it
}
```

A Git parent fact MUST NOT be substituted for `LogicalParent { child: StateId, parent: Option<StateId> }`. Atomic pick produces a result Git commit whose sole Git parent is the target-base commit; source ancestry remains composition provenance only.

### AD-GIT-007 — stable semantic identities do not contain mutable Git names

`RepoId` is derived from a persisted repository UUID plus verified common-directory identity, not remote URL. `BranchId` survives ref renames. `WorkspaceId` survives path moves. `StateId`, `JjChangeId`, `JjCommitId`, `JjOperationId`, and `GitOid` are distinct types; no implicit conversion exists.

### AD-GIT-008 — SQLite WAL journals metadata, not substrate state

SQLite WAL remains the default local operation/event journal because it gives typed transactions, constraints, indexed projections, and crash recovery for JJK metadata. It is explicitly challenged at this boundary: a committed WAL record cannot prove that a spawned Git/JJ process, filesystem rename, hook, or remote push occurred, and WAL behavior on network filesystems is not a substitute for repository locking. Therefore the journal stores durable intent and observed receipts; Git/JJ/filesystem truth is re-observed during verification/recovery. Refs are not used as a replacement journal because they cannot encode full operation state and may be remotely omitted. If WAL locking/filesystem probes fail, mutating JJK-native commands stop or use a separately specified supported journal mode; they do not proceed with weaker durability.

## 3. Invariants

| ID | Invariant |
|---|---|
| INV-ADP-001 | Git-only mode implements every stable v0.1 semantic operation. |
| INV-ADP-002 | Every adapter effect belongs to one `OperationId` (`op_` + UUIDv7) and one stable ordinal `EffectId`; retrying the same effect does not create an additional intended effect. |
| INV-ADP-003 | The mutation protocol is exactly `discover → lock → reconcile → resolve → plan → durable prepare → mutate Git/JJ/files → append events+projections → verify → commit/repair`. |
| INV-ADP-004 | The safe-space writer lock is keyed by verified Git common-directory/repository identity and is held from reconcile through committed/aborted/repair-required disposition. It coordinates JJK writers, not arbitrary Git/JJ processes. |
| INV-ADP-005 | Immediately before every external mutation, observed preconditions equal the prepared fingerprint. Ref changes use compare-and-swap old OIDs. Drift aborts before that effect. |
| INV-ADP-006 | If external Git produces a valid result and metadata persistence then fails, repair preserves and adopts observed Git truth; it does not destructively roll Git back merely to match stale metadata. |
| INV-ADP-007 | Read-only commands do not snapshot or mutate a JJ working copy. JJ inspection uses `--at-operation=@ --ignore-working-copy` or a proven equivalent. |
| INV-ADP-008 | Import/export success is observed and journaled; it is never inferred from process launch and failure is never ignored. |
| INV-ADP-009 | JJK never writes `refs/jj/*`, `.jj` internals, `.git` ref files, packed refs, or index bytes directly. JJ owns JJ namespaces; Git plumbing owns Git state. |
| INV-ADP-010 | JJK-owned refs are under `refs/jjk/*`; ordinary collaboration uses `refs/heads/*`, `refs/tags/*`, and configured remote-tracking refs. Custom refs are optional metadata/retention, never required to build or review the project. |
| INV-ADP-011 | An effect manifest declares every intended change to HEAD, refs, index, tracked files, untracked files, ignored files, JJ operation, and remotes. Anything not declared is preserved. |
| INV-ADP-012 | Ignored files are never captured, deleted, or moved unless a command explicitly names them and previews that effect. |
| INV-ADP-013 | A linked worktree has its own `WorkspaceId`, git-dir, HEAD, index fingerprint, and worktree state, while sharing one `RepoId`, common-dir, object store, and repository lock domain. |
| INV-ADP-014 | No network operation assumes a remote named `origin`; the remote/refspec/upstream is resolved explicitly from config or supplied by the user. |
| INV-ADP-015 | Transparent passthrough neither acquires the JJK writer lock nor changes JJK metadata. |
| INV-ADP-016 | A capability is `available` only after its probe succeeds in the current repository/workspace. Executable presence alone is insufficient. |

## 4. Data/API shapes

### 4.1 Capability discovery

```rust
pub struct CapabilitySnapshot {
    pub snapshot_id: CapabilitySnapshotId,
    pub repo: RepoIdentity,
    pub git: GitCliCapabilities,
    pub gix: GixCapabilities,
    pub jj: JjCapabilities,
    pub forge: Vec<ForgeCapabilities>,
    pub degraded: Vec<Degradation>,
    pub observed_at: SystemTime,
}

pub struct RepoIdentity {
    pub repo_id: RepoId,
    pub worktree_root: Option<PathBuf>, // None for bare repositories
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub is_bare: bool,
    pub is_linked_worktree: bool,
    pub object_algorithm: ObjectAlgorithm,
    pub ref_storage: RefStorage,
    pub repository_extensions: BTreeMap<OsString, OsString>,
}

pub struct GitCliCapabilities {
    pub executable: PathBuf,
    pub version: String,
    pub porcelain_v2_z: bool,
    pub update_ref_stdin_z: bool,
    pub pathspec_file_nul: bool,
    pub worktree_porcelain_z: bool,
    pub object_algorithm: ObjectAlgorithm,
    pub supported: BTreeSet<GitPrimitive>,
}

pub enum GixMode { EnabledReadOnly, Disabled }

pub struct GixCapabilities {
    pub mode: GixMode,
    pub object_algorithm_supported: bool,
    pub repository_format_supported: bool,
    pub parity_fixture_version: String,
    pub reason_disabled: Option<DegradationCode>,
}

pub enum JjMode { Absent, DetectedNotColocated, EnabledColocated, Degraded }

pub struct JjCapabilities {
    pub mode: JjMode,
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    pub workspace_root: Option<PathBuf>,
    pub git_root: Option<PathBuf>,
    pub workspace_id: Option<JjWorkspaceId>,
    pub operation_id: Option<JjOperationId>,
    pub object_algorithm_supported: bool,
    pub supported: BTreeSet<JjPrimitive>,
}
```

Discovery runs cheap probes before expensive reads:

1. resolve `git` via the inherited executable-search rules, without `$SHELL -lc`;
2. ask Git for top-level, git-dir, common-dir, bare/worktree status, object format, ref storage/extensions, HEAD, and worktree list using structured outputs;
3. reject unknown required repository extensions for mutation while retaining safe Git passthrough;
4. open/probe `gix` against that exact common-dir and object algorithm;
5. resolve and verify JJ colocation;
6. inspect configured remotes without contacting them;
7. contact a forge/server only for an operation that requires a network capability.

Capability snapshots are cached by executable identity/version plus repository format/config fingerprints. A mutation rechecks relevant capabilities after lock acquisition.

### 4.2 Adapter effect protocol

```rust
pub enum OperationStatus {
    Prepared,
    Applying,
    AwaitingResolution,
    Verifying,
    Committed,
    Aborting,
    Aborted,
    RepairRequired,
}

pub trait EffectAdapter {
    type Prepared;
    type Receipt;
    type Observation;

    fn preflight(
        &self,
        effect: EffectId,
        expected: &PreconditionFingerprint,
    ) -> Result<Self::Prepared, AdapterError>;

    fn apply(
        &self,
        prepared: &Self::Prepared,
        idempotency_key: EffectId,
    ) -> Result<Self::Receipt, AdapterError>;

    fn observe(
        &self,
        prepared: &Self::Prepared,
    ) -> Result<Self::Observation, AdapterError>;

    fn verify(
        &self,
        prepared: &Self::Prepared,
        actual: &Self::Observation,
    ) -> VerificationReport;

    fn compensate(
        &self,
        prepared: &Self::Prepared,
        actual: &Self::Observation,
    ) -> Result<RepairOutcome, AdapterError>; // only if compensation CAS still matches
}
```

`PreconditionFingerprint` contains:

- repository/common-dir identity and object algorithm;
- HEAD kind (`unborn`, symbolic ref, detached) and resolved OID;
- sorted relevant ref names/OIDs plus absence assertions;
- worktree inventory and checked-out branch ownership;
- this workspace's index identity/checksum and porcelain-v2 status digest;
- tracked/untracked/ignored manifests needed by the declared effect;
- JJ workspace/current operation/change/commit IDs when JJ participates;
- remote URL/refspec/upstream fingerprints for network effects.

Fingerprints are comparison proofs, not security hashes. Secrets, credential-helper output, file contents, and full environment values MUST NOT be journaled.

### 4.3 Observed facts

```rust
pub enum AdapterFact {
    GitCommitObserved(GitCommitObserved),
    GitRefObserved(GitRefObserved),
    GitHeadObserved(GitHeadObserved),
    GitWorktreeObserved(GitWorktreeObserved),
    GitIndexObserved(GitIndexObserved),
    JjGitImported(JjSyncFact),
    JjGitExported(JjSyncFact),
    JjOperationObserved(JjOperationObserved),
    RemoteRefsObserved(RemoteRefsObserved),
    CapabilityDegraded(Degradation),
}

pub struct JjSyncFact {
    pub operation_id: OperationId,
    pub effect_id: EffectId,
    pub direction: JjSyncDirection, // GitToJj or JjToGit
    pub jj_operation_before: JjOperationId,
    pub jj_operation_after: JjOperationId,
    pub git_refs_before: RefSnapshotId,
    pub git_refs_after: RefSnapshotId,
    pub imported_or_exported: Vec<RefDelta>,
    pub conflicts: Vec<QualifiedRefName>,
    pub outcome: AdapterOutcome,
    pub stdout_digest: Option<Digest>,
    pub stderr_digest: Option<Digest>,
}
```

Facts report what was observed, including no-op and partial/failure outcomes. An intent to import is not an import fact.

## 5. Operation decision table

| Operation | Native Git CLI | `gix` read | JJ CLI | Rationale / degradation |
|---|---:|---:|---:|---|
| Discover repo/common-dir/worktree/object format/extensions | **Required** | No | Probe only | Git defines the active repository and worktree semantics. |
| Read/decode commit, tree, tag by full OID | Fallback/oracle | Preferred when approved | No | Same typed result; `gix` failure falls back. |
| Walk reachable commit graph/import external commits | Boundary resolution + fallback | Preferred when approved | Identity enrichment only | Git OIDs/parents are facts; JJ IDs never replace them. |
| Read refs, HEAD, worktree list, upstream/config | **Required** | No | No | Handles symbolic refs, linked worktrees, extensions, and config truth. |
| Status/index/untracked/ignored classification | **Required** (`status --porcelain=v2 -z`, `check-ignore`, plumbing) | No | No | Filters, index stages, submodules, sparse checkout, and ignore rules are Git behavior. |
| Create capture tree/commit | **Required** with temporary index + plumbing | Optional post-read | Import after Git effect when enabled | Never stages through the user's real index. |
| Compare-and-swap one/many refs | **Required** (`update-ref`, stdin transaction when supported) | No | Export only after a JJ-originated effect | No direct ref-file writes. Multi-ref visibility caveat remains. |
| Switch/restore tracked worktree and index | **Required** | No | Only for an explicitly JJ-backed operation | Requires durable preservation boundary first. |
| Create/list/remove/prune Git worktrees | **Required** | No | No by default | Git worktrees are the interoperability mechanism. |
| Atomic pick / patch apply | **Required** for canonical v0.1 result | Read summaries only | Optional future accelerator after parity | Result commit's sole parent is target base. |
| Merge/rebase/canonical promotion | **Required** for stable v0.1 path | Read summaries only | Optional explicitly selected path | Plan states which engine; never silently switches engines mid-operation. |
| JJ change IDs and operation-log inspection | No | No | **Required if capability requested** | Read without working-copy snapshot. |
| Import external Git changes into colocated JJ | Verify Git before/after | No | **Required when JJ mode enabled** | Explicit command/fact despite colocation's automatic behavior; failure degrades JJ, not Git. |
| Export JJ-originated changes to Git | Verify refs/objects before/after | No | **Required before JJK records success** | Git-visible verification is the acceptance boundary. |
| Fetch/push/pull/bundle/credential/helper/protocol work | **Required** | No | No for v0.1 transport | Preserve installed Git and forge behavior. |
| Transparent `jjk git -- …` | **Exec Git** | No | No | No JJK side effects or parsing. |
| SHA-256 local operations | **Required** when Git probe succeeds | Only if capability succeeds | Only if capability succeeds | Otherwise Git-only; remote/JJ support is separately reported. |
| Bare-repo graph/ref/remote operations | **Required** | Optional approved reads | Disabled unless verified | Workspace operations return `NoWorktree`. |

## 6. Git capture, workspace, and ref behavior

### 6.1 Capturing a state without abusing the index

A JJK capture that includes the workspace uses a temporary index:

1. durably record the effect plan and preservation manifest;
2. create a private temporary index in the operation directory;
3. seed it from the intended parent tree (or the algorithm-correct empty tree);
4. add the intended worktree paths with native Git, honoring ignore/attributes/filters and including non-ignored untracked files;
5. write the tree and commit with native Git plumbing;
6. compare-and-swap the intended branch/JJK retention refs from prepared old OIDs;
7. leave the user's real index and files untouched unless the effect manifest explicitly says capture also advances/cleans the active workspace;
8. verify tree, parents, refs, status, and original-index preservation.

This capture path deliberately uses Git plumbing and therefore does not pretend to run porcelain `git commit` hooks. The JJK-native plan MUST disclose that hook policy; validations/signing required by repository policy run as separately declared effects. Transparent passthrough and Git-enhanced porcelain commands retain their native hook behavior.

The temporary index path and operation data are on the same trusted local filesystem where possible. Cleanup occurs only after commit/repair makes it unnecessary.

A capture's Git commit stores the aggregate tree, not the staged/unstaged partition. JJK v0.1 preserves the pre-operation index/worktree for rollback and leaves it untouched for non-navigation captures. A command that intentionally activates another state declares that HEAD/index/worktree will change and first protects dirty work. Full historical restoration of staging intent is a separately advertised Timeshift capability, not an implicit v0.1 claim.

### 6.2 Index and untracked preservation

Preservation is effect-specific:

| Command effect | HEAD/refs | Real index | Tracked worktree | Non-ignored untracked | Ignored |
|---|---|---|---|---|---|
| annotate/tag/show/story | unchanged | byte/semantic fingerprint unchanged | unchanged | unchanged | untouched |
| capture without activation | declared retention/branch refs only | unchanged | unchanged | remain present | untouched |
| activate/return/switch | declared target | becomes declared target index | becomes target tree | preflight refuses or protects then removes/replaces only declared collisions | untouched |
| stash/protect dirty work | declared recovery ref/state | declared clean target after verified capture | declared clean target | captured then removed only after object verification | untouched |
| worktree create | new branch/worktree refs | source unchanged; new index belongs to new workspace | source unchanged | source unchanged | source untouched |

Before an activation, JJK inventories index stages (including conflicts), intent-to-add, skip-worktree/assume-unchanged bits, sparse-checkout state, submodule gitlinks, case collisions, symlinks, tracked changes, untracked paths, and collisions with the target tree. Unsupported entries stop before mutation. `git clean -fd` is never a generic cleanup primitive.

Untracked protection requires durable content, not just path names: capture to verified Git objects/recovery commit where semantically valid, or to checksummed JJK recovery blobs that preserve file type, executable bit, symlink target, and relative path. File restoration uses no-follow/path-containment checks. Special files, unsafe symlinks, permission loss, or filesystem normalization ambiguity stop the operation.

### 6.3 Refs and HEAD

- Standard branches remain `refs/heads/<name>` and are suitable for GitHub PRs.
- `refs/jjk/states/<StateId>` MAY retain state commits; `refs/jjk/attempts/<AttemptId>` MAY retain attempt tips. Names are machine IDs, not mutable labels.
- JJK MUST NOT expose every state as a visible branch or tag.
- JJK-owned refs are excluded from default remote sync. Metadata sync uses explicit refspecs and a separately reported result.
- Deletion/archive changes semantic projections first; retention-ref deletion is compare-and-swap and cannot make the only copy of unpushed user work unreachable without a verified recovery root.
- HEAD may be unborn, symbolic, or detached; all three are modeled. No fallback to a guessed `main` branch is allowed.
- Ref creation/deletion/update includes the expected old OID/absence. Multiple related refs use `git update-ref --stdin -z` transaction support when probed, while retaining the documented partial-reader visibility caveat.
- Reflogs are recovery evidence, not the authoritative JJK journal and not guaranteed to exist.

### 6.4 Worktrees

Git worktree inventory comes from structured porcelain with NUL termination when supported. Each entry records `WorkspaceId`, canonical/display path, per-worktree git-dir, shared common-dir, HEAD form/OID, checked-out branch, lock/prunable state, and status fingerprint.

Rules:

1. A branch already checked out in another worktree is not switched or reused behind Git's protection.
2. A ref update affecting another live worktree requires a plan naming that workspace and a post-check that it remains coherent; otherwise JJK refuses.
3. Worktree paths are never guessed from branch names without collision checks. A subprocess never claims to change its parent shell's cwd; it prints a path or uses explicit shell/terminal integration.
4. Removing a worktree requires a clean/protected workspace, path identity recheck, Git-native removal, and proof that no untracked content was lost.
5. JJK metadata lives at repository scope keyed by the common-dir/RepoId, with workspace projections keyed by `WorkspaceId`; it is not copied as divergent per-worktree databases.
6. A Git-linked worktree is not automatically a JJ workspace. JJ is disabled for that workspace unless `jj workspace` discovery proves an explicit unambiguous mapping to the same backend. Git-only behavior remains fully available.
7. Creating a Git worktree and a JJ workspace as one alleged atomic operation is unsupported in v0.1. If a future operation composes them, it must journal two effects and repair partial creation.

## 7. Transparent Git passthrough

Unclaimed top-level argv and the explicit collision-proof form `jjk git -- <argv…>` share one escape path with a strict byte/native-string contract:

- On Unix, JJK carries arguments as `OsString` and forwards each argument's bytes exactly; it performs no UTF-8 round trip, joining, splitting, quoting, globbing, alias expansion, or shell evaluation.
- On Windows, JJK preserves each native wide argument as supplied by the runtime; it does not claim preservation of nonexistent Unix byte semantics.
- The current cwd is unchanged and passed exactly.
- The environment is inherited exactly, including Git config/SSH/credential variables. JJK adds, removes, and rewrites nothing.
- stdin, stdout, and stderr are inherited file descriptors/handles, preserving TTY detection, binary output, prompts, pager, color, and progress.
- Unix uses process replacement (`exec`) where supported so Git receives terminal signals directly. A non-exec platform forwards supported signals/control events, waits, and returns Git's exit code; any platform parity limitation is reported by capability/doctor output.
- Executable resolution follows the platform's normal direct process search. Git aliases then behave exactly as native Git because Git receives the arguments.
- JJK emits no prefix/suffix, parses no output, does not acquire locks, and does not touch its journal. A failed exec uses a distinct launcher failure; an executed Git process returns Git's exact exit status.

If callers want reconciliation, they run a JJK-native command after passthrough. This separation is necessary to make passthrough genuinely transparent.

## 8. External Git reconciliation

Reconciliation converts external substrate truth into idempotent facts and projections; it does not rewrite that truth.

### 8.1 Reconcile algorithm

1. Under the JJK writer lock, discover the current repo/workspace/capability snapshot.
2. Read HEAD, all relevant refs, worktrees, index/status fingerprints, and object algorithm through Git CLI.
3. Compare fast watermarks (HEAD/ref snapshots, packed-ref/reftable/config/index identities) only to skip proven unchanged regions. mtimes are hints, never correctness proofs.
4. For each new/changed ref tip, traverse ordered Git parents until known OIDs/boundaries, using approved `gix` reads or Git fallback.
5. Append `GitCommitObserved` once per `(RepoId, GitOid)`. Multiple refs or JJK states may point to one commit without creating duplicate commit facts.
6. Append separate branch/ref movement facts, including create, fast-forward, rewind/force move, delete, rename inference (marked inference), detached HEAD, and remote-tracking changes.
7. Map an external commit to a semantic `git` state only by deterministic rules. Git parentage remains substrate topology; JJK logical parent mapping is explicit.
8. If several semantic mappings are plausible, record an unresolved observation and stop target-dependent mutation. Never choose by fuzzy label or timestamp.
9. If colocated JJ is enabled, import Git changes as §9 specifies and append the observed sync fact.
10. Materialize projections, verify they reproduce the observed refs/graph, and advance the reconcile watermark.

Reconciliation is repeatable: replaying the same Git/JJ observations appends no duplicate facts and produces the same projections. A ref rewind never deletes immutable commits/states; it changes the ref projection and may make prior work non-tip/unreachable by Git refs, which JJK retention policy then handles.

### 8.2 Concurrent native Git

The JJK lock cannot stop `git commit`, an IDE, a hook, or another JJ process. Every external effect therefore rechecks its prepared fingerprint immediately before apply. Ref effects use Git compare-and-swap. Workspace effects recheck HEAD/index/status/path identities. Drift returns `ExternalMutationDetected` with before/actual deltas and restarts reconciliation; it does not overwrite the winner.

If drift occurs after some JJK effects applied, status becomes `RepairRequired`. Observation determines which effects landed. Repair completes metadata for valid external truth, retries only provably idempotent missing effects, and compensates only when the current fingerprint still exactly equals the effect's recorded postcondition.

## 9. Optional JJ adapter

### 9.1 Colocated behavior

JJ colocation means Git and JJ share Git storage and JJ automatically imports/exports during JJ commands. It does **not** make their workspaces, operation logs, refs, indexes, or JJK journal one transaction.

Read-only JJ probes MUST prevent incidental working-copy snapshots. For example, operation inspection uses the equivalent of:

```text
jj --at-operation=@ --ignore-working-copy op log --limit 1 --no-graph -T <machine-template>
```

Exact templates are version-probed and parsed as NUL/length-delimited fields where JJ supports them. Human graph output is never parsed.

For a Git-originated JJK effect in enabled JJ mode:

1. prepare Git and expected JJ-operation fingerprints;
2. apply and verify Git;
3. run explicit `jj git import --ignore-working-copy` even though colocation often imports automatically, so the boundary has an attributable effect/receipt;
4. observe JJ operation/change/commit mappings and conflicts;
5. append `JjGitImported`; then verify Git remains unchanged except declared JJ-owned consequences.

For a JJ-originated effect:

1. prepare expected JJ operation/workspace and Git fingerprints;
2. execute the exact JJ operation without interactive tools unless the command is explicitly interactive;
3. observe its new JJ operation/change/commit IDs;
4. run/verify `jj git export --ignore-working-copy` when needed for an attributable boundary;
5. reread Git refs/objects/workspace through Git CLI;
6. append `JjGitExported` only for the observed deltas;
7. accept success only if the planned Git-visible state and JJK semantic state verify.

Import/export stderr, conflicts, abandoned working-copy replacement, bookmark conflicts, and divergent JJ operations are product states, not warnings to suppress.

### 9.2 JJ identities and operation-log use

A semantic state MAY carry optional `(JjChangeId, JjCommitId, JjOperationId)` provenance. A change ID can survive rewritten JJ commits, but it never replaces the immutable Git content commit OID or stable JJK StateId. One JJ change may map to successive Git commit OIDs; each mapping is an observed versioned fact.

JJ operation-log restore/undo is not a replacement for JJK repair. JJK may offer an explicitly JJ-backed recovery only when the prepared operation ID is still an ancestor/current operation and the resulting Git state is previewed and verified. `jj op restore` or undo that could discard unrelated concurrent JJ work is refused.

### 9.3 JJ conflicts and downgrade

On import/export conflict, divergent operations, unsupported backend/object format, stale workspace, or parse/probe failure:

- stop the JJ-dependent effect;
- retain the valid Git/JJK state already observed;
- record exact degradation and conflict facts;
- set JJ mode to `Degraded` for that workspace;
- continue stable operations through Git-only mode if and only if they do not overwrite unresolved JJ work;
- require explicit resolution/re-enable before another JJ-originated mutation.

JJK MUST NOT silently fall back from a partially applied JJ mutation to a different Git algorithm. Engine choice is fixed in the durable plan.

## 10. Remotes and GitHub compatibility

### 10.1 Remote policy

Native Git owns network operations. JJK respects configured URLs, insteadOf rules, refspecs, proxies, SSH, signing, LFS/filter processes, credential helpers, protocol negotiation, hooks, and server diagnostics.

- Remote selection is explicit or derived from the active branch's configured upstream. No `origin` default is invented when ambiguous or absent.
- `jjk sync fetch` fetches through Git, records before/after remote/local refs, then reconciles. It does not merge, rebase, or activate implicitly.
- Pull-like semantic operations split fetch from integrate so the plan names fast-forward, merge, rebase, or refuse.
- Push plans list every source/destination ref, expected remote lease, force policy, and whether metadata refs are included. Rewrites require `--force-with-lease` semantics tied to an observed remote OID, never blind force.
- Branch and optional JJK metadata pushes are separate reported effects unless server-advertised atomic push is deliberately used. A successful branch push plus failed optional metadata push remains a successful interoperable branch push with degraded metadata sync.
- Fetch/push never modifies the user's persistent refspec/config merely to make JJK metadata travel. Metadata sync uses command-local explicit refspecs.
- Network retries are not automatic after an ambiguous disconnect; JJK first observes remote/local refs to distinguish applied from unapplied.

### 10.2 GitHub and forge compatibility

The compatibility contract is substrate-first:

1. PR heads are ordinary `refs/heads/*` commits; GitHub does not need JJK or JJ.
2. Commit objects, parents, trees, signatures, tags, merge bases, branch protection, CI checkouts, review diffs, fetch, and merge queues remain standard Git.
3. JJK metadata/custom refs are optional. Their absence changes semantic richness, not buildability or reviewability.
4. JJK does not require `refs/jj/*`, replace refs, grafts, alternates unavailable to CI, custom object types, smudge filters, or rewritten remote helpers.
5. GitHub-specific PR/fork discovery belongs to a forge adapter. Git CLI remote transport remains usable if forge APIs, auth, or rate limits are unavailable.
6. A GitHub claim is published only after a no-JJK clone, branch push, PR/CI simulation, review diff, merge/fetch, and clean uninstall fixture passes.

## 11. SHA-256 repositories

SHA-256 is a repository capability, not a compile-time assumption.

- Discover the storage/input/output algorithms from Git and require one unambiguous storage algorithm.
- Parse OIDs into algorithm-tagged bytes; validate lengths at construction and serialize with the algorithm.
- Derive empty-tree OID using native Git for the active repository. Generate absence/zero values at the algorithm's length.
- Fixtures include SHA-1 and SHA-256 roots, commits, merges, tags, refs, bundles where supported, linked worktrees, and import/reconcile.
- Disable `gix` if the pinned version has not passed the SHA-256 fixture set.
- Disable JJ for that repository unless the installed JJ/backend probe and a disposable round-trip fixture prove support. Git-only remains available.
- Treat remote SHA-256 compatibility independently. A locally valid SHA-256 repository does not imply a particular server or GitHub accepts it. `doctor` reports local Git, `gix`, JJ, each remote, and each forge separately as `supported`, `unsupported`, or `unverified`.
- JJK never converts an existing repository's object algorithm. Migration is out of scope and cannot be a fallback.

## 12. Explicit degradation model

```rust
pub enum CapabilityState { Stable, Experimental, Unavailable }

pub struct Degradation {
    pub code: DegradationCode,
    pub component: Component,
    pub capability: CapabilityName,
    pub state: CapabilityState,
    pub reason: String,
    pub semantic_effect: SemanticEffect,
    pub fallback: Option<Fallback>,
    pub user_action: Option<String>,
}
```

| Condition | Stable behavior | Explicit report |
|---|---|---|
| `gix` absent/unsupported/decode failure | Rerun read through Git CLI | Performance degradation only. |
| JJ absent | Full Git-only mode | JJ identities/op-log acceleration unavailable. |
| JJ present but non-colocated | Git-only mode | Non-colocated JJ detected; unsupported in v0.1. |
| Linked Git worktree lacks proven JJ workspace | Git-only in that workspace | JJ disabled for this `WorkspaceId`. |
| JJ import/export conflict or divergent operations | Stop JJ-dependent mutation; preserve Git truth; repair/degrade | Conflict IDs and resolution command/surface. |
| SHA-256 unsupported by `gix` or JJ | Native Git-only | Component-specific unsupported state. |
| Unknown required Git repository extension/ref format | Safe transparent passthrough and proven read-only commands only | Mutations unavailable until supported. |
| Bare repository | Graph/ref/remote operations only | Workspace/index commands return typed `NoWorktree`. |
| No remote/upstream | Local operations continue | Sync/push unavailable; no invented `origin`. |
| Forge API unavailable/rate-limited | Git remote operations continue | PR Radar/forge enrichment unavailable. |
| Atomic remote push unsupported | Separate planned pushes or refuse when all-or-none is required | No atomicity claim. |
| Concurrent external Git/JJ drift | Abort/reconcile/repair | Before/actual fingerprint delta. |

Degradation is visible in `jjk status --capabilities`, `jjk doctor`, machine JSON, and operation receipts. It is never a debug-only log.

## 13. Failure modes and pre-approved responses

| ID / observable symptom | Risk | Required response |
|---|---|---|
| FM-ADP-001: Git CLI succeeds, journal append fails | Dual truth | Mark `RepairRequired`; observe Git; append missing facts/projections idempotently. Preserve valid Git result. |
| FM-ADP-002: journal prepared, Git never applied | Stuck intent | Observe precondition still present, mark aborted, retain audit record; do not manufacture result facts. |
| FM-ADP-003: some ref updates observed | Partial visibility/concurrency/version fallback | Compare each ref to prepared pre/post values. Complete only missing CAS-safe effects or retain external winner and repair projection. |
| FM-ADP-004: index changed between plan and apply | User/IDE work collision | Stop before workspace mutation, reconcile, show index/status delta; never reset. |
| FM-ADP-005: untracked target collision | Data loss | Protect content durably or refuse with exact paths. Never generic clean. |
| FM-ADP-006: JJ command snapshots working copy during a read | Hidden mutation | Capability test fails; disable JJ adapter, observe/import resulting facts, require repair if state changed. |
| FM-ADP-007: JJ import abandons/replaces working-copy commit | Lost identity illusion | Record old/new change/commit IDs and operation; verify files/Git; never reuse old ID silently. |
| FM-ADP-008: external force-push/branch rewind | Semantic branch drift | Record ref move; preserve prior states; require explicit promotion/integration decision. |
| FM-ADP-009: remote disconnect after push | Duplicate/destructive retry | Observe remote destination OID first; retry only if proven unapplied and lease still matches. |
| FM-ADP-010: branch checked out elsewhere | Cross-worktree incoherence | Refuse implicit switch/update; name owning workspace and offer isolated attempt/worktree. |
| FM-ADP-011: Git hook/filter changes or rejects effect | Planned tree differs | Treat native result/diagnostic as fact; verify object/tree. Never bypass hooks/filters unless the command explicitly promises plumbing semantics. |
| FM-ADP-012: submodule/sparse checkout/conflicted index | Incomplete capture/restore | Use Git semantics and fixture-approved operation or stop as unsupported; never flatten gitlinks or clear conflict stages. |
| FM-ADP-013: case-folding or Unicode path collision | Wrong-file overwrite | Stop during manifest construction; require user resolution on the actual filesystem. |
| FM-ADP-014: `gix` disagrees with Git | Corrupt projection | Quarantine `gix` for the repo/run, use Git result, emit parity failure bundle without secrets. |
| FM-ADP-015: SHA-1-shaped OID encountered in SHA-256 repo | Identity corruption | Reject typed construction; report source parser/metadata migration error. |
| FM-ADP-016: transparent passthrough output differs | Broken trust escape hatch | Conformance blocker; ship no passthrough claim on that platform until argv/cwd/env/stdio/signal/exit parity passes. |
| FM-ADP-017: JJK lock held but native Git mutates | False exclusion assumption | Fingerprint/CAS detects drift; external winner is reconciled. Lock is never described as global repository exclusion. |

## 14. Parity and conformance strategy

### 14.1 Golden semantic parity

Each stable semantic scenario runs first in Git-only mode; its normalized `SemanticOutcome` is the golden contract. The same scenario then runs in verified colocated JJ mode. Normalization removes allowed substrate differences (JJ operation/change IDs, adapter timing, executable version) but compares:

- state IDs and kinds under deterministic fixture clocks/IDs;
- logical parents and composition provenance;
- full algorithm-tagged Git commit/tree OIDs where deterministic identity inputs are fixed;
- ordered Git parents and branch/ref targets;
- worktree files, modes, symlinks, index/status outcome, and untracked/ignored preservation;
- JJK event intent and materialized graph;
- plan declarations, verification verdict, and degradation state.

JJ parity MUST NOT assert identical operation-log topology or claim atomicity. It asserts the same verified semantic and Git-visible end state.

### 14.2 Required fixture matrix

| Axis | Cases |
|---|---|
| Substrate | Git-only; colocated JJ; JJ absent; JJ detected non-colocated; degraded JJ conflict |
| Object format | SHA-1; SHA-256 Git-only; SHA-256 JJ only when positively supported |
| Repository | unborn; ordinary; bare; linked worktree; many worktrees; submodule fixture; sparse checkout; conflicted index; large history/monorepo |
| HEAD/ref topology | symbolic; detached; unborn; merge commit; branch rename/delete/rewind; packed refs; supported alternative ref storage |
| Workspace | clean; staged only; unstaged only; staged+unstaged same file; intent-to-add; untracked; ignored; symlink; executable; filename with tabs/newlines/non-UTF8 bytes where platform permits |
| Remote | no remote; non-`origin`; branch upstream; HTTPS/SSH fixture; rejected lease; disconnect-after-accept; atomic push advertised/unavailable |
| Concurrency/crash | external commit between prepare/apply; concurrent JJK worktrees; kill before/after every effect and journal boundary |
| Platform | macOS/Linux/Windows behavior, with platform-exact passthrough assertions |

### 14.3 Adapter-specific conformance

1. **Git CLI parsing:** property/fuzz machine parsers with NUL-containing protocol boundaries, hostile refs/paths, malformed/truncated output, and version fixtures.
2. **`gix` differential:** for every approved read, compare against native Git on generated DAGs, merges, annotated tags, alternates, shallow repositories where supported, SHA variants, and corrupted objects. Any mismatch disables the fast path.
3. **JJ round trip:** in disposable colocated fixtures, external Git commit/ref move → explicit import → identity observation; JJ mutation → explicit export → Git verification; repeat to prove idempotence; inject conflicts/divergent operations.
4. **Worktree isolation:** mutate one workspace while asserting every sibling HEAD/index/worktree/untracked fingerprint remains unchanged unless named in the plan.
5. **Index/untracked monsters:** retain historical regressions for staged return, untracked stash, ignored `.worktrees`, detached HEAD, and branch-from-historical-state topology.
6. **Remote/GitHub:** clone without JJK/JJ, build/test fixture, create/push branch, produce PR-compatible diff, simulate CI checkout/merge/fetch, omit optional refs, uninstall JJK, and prove the repository remains understandable Git.
7. **Passthrough:** helper Git executable records raw/native argv entries, cwd identity, environment digest, TTY/pipe descriptors, signals, binary streams, and exit status; compare direct invocation to both unclaimed top-level `jjk <argv…>` and explicit `jjk git -- <argv…>` byte/native-string-for-native-string.
8. **Fault injection:** terminate at every transition of `Prepared → Applying → AwaitingResolution/Verifying → Committed` and `Aborting → Aborted/RepairRequired`; restart and assert deterministic observe/repair without work loss.

### 14.4 Evidence levels

A capability is:

- **stable** only when its matrix cells and failure drills pass on supported platforms;
- **experimental** when explicitly enabled and its unsupported cells are reported;
- **unavailable** when discovery or a tripwire fails.

Unit parser tests alone do not establish interoperability. Release evidence includes real Git/JJ CLI runs and no-JJK clone/forge workflows.

## 15. Acceptance checks

| ID | Check |
|---|---|
| AC-ADP-001 | The implementation has one typed decision registry matching §5; no mutating call site chooses `gix`. |
| AC-ADP-002 | Git-only and colocated-JJ golden scenarios produce equal normalized semantic outcomes without comparing JJ-only IDs. |
| AC-ADP-003 | Missing/broken `gix` and missing JJ leave every stable Git-only operation usable and visibly report degradation. |
| AC-ADP-004 | External Git commits, merges, branch creates/renames/rewinds/deletes, detached HEAD, and remote-tracking moves reconcile idempotently as distinct facts. |
| AC-ADP-005 | Capture uses a private index; annotation/capture tests prove the original real-index and source workspace fingerprints remain unchanged except declared effects. |
| AC-ADP-006 | Return/stash/worktree tests prove staged, unstaged, non-ignored untracked, ignored, symlink, executable, conflict-stage, and sibling-worktree preservation or typed refusal. |
| AC-ADP-007 | JJ inspection causes no working-copy snapshot; import/export receipts contain before/after operation and Git ref facts, and failures are not ignored. |
| AC-ADP-008 | Linked Git worktrees without explicit JJ workspace mapping run Git-only and never invoke JJ accidentally. |
| AC-ADP-009 | SHA-1 and SHA-256 fixtures pass native Git paths with no fixed OID length/empty-tree constant; unsupported `gix`/JJ paths disable themselves. |
| AC-ADP-010 | A no-JJK/no-JJ clone can build, inspect history, branch, fetch, push, and participate in the GitHub PR/CI fixture without custom refs. |
| AC-ADP-011 | Non-`origin` and no-upstream fixtures behave correctly; metadata ref sync is optional and never mutates persistent refspecs. |
| AC-ADP-012 | Passthrough parity proves native argv, cwd, environment, stdio/TTY, signals, binary output, and exit status on each supported platform. |
| AC-ADP-013 | Crash injection after external apply/before journal append repairs by observing valid Git truth; it never destructively rolls Git back to satisfy metadata. |
| AC-ADP-014 | Multi-ref and remote reports name the exact atomicity primitive used and retain partial-reader/cross-system limitations; no UI/help text says the whole semantic operation is atomic. |
| AC-ADP-015 | `jjk status --capabilities` and machine JSON explain engine choice and every fallback before a user depends on it. |

## 16. Explicit non-goals for v0.1

- Reimplementing Git, its wire protocols, credential helpers, filters, merge engine, or hosting forges in Rust.
- Mutating repositories through `gix`.
- Supporting non-colocated JJ repositories or pretending Git worktrees are automatically JJ workspaces.
- Requiring JJ for a stable command.
- Claiming an ACID transaction across Git, JJ, SQLite, filesystem, hooks, or remotes.
- Automatically converting SHA-1 repositories to SHA-256, or claiming all remotes/forges support SHA-256.
- Making JJK metadata/custom refs mandatory for clones, CI, PRs, or collaborators.
- Persistently changing user Git config/refspecs, installing hooks, bypassing hooks, or replacing native credential behavior without a separate explicit feature.
- Restoring the complete historical staged/unstaged/shell/editor situation under the basic state contract; that is Timeshift and must advertise adapter-by-adapter limits.
- Inferring semantic intent from every raw Git commit beyond the typed `git` state and deterministic topology facts.
- Hiding conflicts, external drift, partial effects, or degradation to keep a command looking successful.

## 17. Implementation sequence

1. Implement typed `GitOid`, repository/workspace/ref identities, capability snapshots, and Git CLI machine-protocol parsers.
2. Implement Git-only observation/reconciliation and the effect adapter protocol with durable operation records.
3. Implement private-index capture, CAS refs, preservation manifests, worktree inventory, and repair drills.
4. Add native remote behavior and transparent passthrough conformance.
5. Add `gix` read paths one operation at a time behind differential parity and instant fallback.
6. Add JJ discovery/read-only observation, then explicit import/export facts, then selected JJ accelerators only after semantic parity.
7. Publish capability and compatibility matrices from executable conformance results, not hand-maintained claims.

This ordering preserves the key architectural truth: Git interoperability is the foundation, JJK meaning is the product, and JJ is an optional verified acceleration—not a hidden dependency or an atomicity story the substrates cannot provide.

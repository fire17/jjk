# JJK v0.1 Unknowns Oracle

> Future implementers: consult this before solving a failure; the risk, default decision, and proof may already be here.
>
> **Status:** decision-grade architecture input, 2026-08-28.  
> **Scope:** JJK v0.1 rewrite: Git universal substrate, JJ optional, JJK semantic state layer.  
> **Authority:** subordinate to `VISION.md`; grounded in `vision_overhaul.md` and the current TypeScript implementation. This file does not widen the v0.1 product promise.

## 1. Context and evidence

The founding promise—“turn a directory into a safe space”—is unusually load-bearing. The legacy implementation demonstrates both the value and the danger: it creates real commits and refs, but metadata is rewritten as whole JSON files, Git/JJ failures are sometimes allowed, state resolution may select the first fuzzy match, and restoration can use `git clean -fd`. The overhaul correctly calls for typed events, projections, transactions, recovery, and compatibility fixtures. Those mechanisms are necessary but not sufficient: a SQLite transaction cannot atomically commit a Git ref, an index, a worktree, a JJ operation, and a filesystem rename.

Evidence labels used below:

| ID | Observed fact |
|---|---|
| `E-01` | Current `store.ts` writes `repo.json` and `history.json` as whole files without a cross-process lock or durable rename protocol. |
| `E-02` | Current save stages `git add --all -- .`; ignored files are outside the snapshot, while restore can run `git clean -fd`. |
| `E-03` | Current Git/JJ import/export calls may use `allowFailure`, allowing a caller to continue after substrate drift. |
| `E-04` | Current reconciliation walks `git log --all` and assigns one display branch to commits reachable from multiple branches. |
| `E-05` | Current command runner captures and trims stdio instead of behaving as an exact Git process boundary. |
| `E-06` | Current `pick` reads changed files as UTF-8 and applies custom text merging, which is not safe for arbitrary binary/path/mode/rename/submodule changes. |
| `E-07` | The overhaul promises conformance across Git-only, colocated JJ, bare repos, submodules, monorepos, linked worktrees, forks, and concurrent writers. |
| `E-08` | The overhaul sets warm orientation, planning, and graph first-paint budgets of 50/100/100 ms and forbids whole-repo scans on every command. |

## 2. Decisions

### D-01 — Define the v0.1 safety envelope before claiming “never lose work”

A **captured state** MUST preserve tracked content, index stages, working-tree content, untracked content selected by policy, file modes, symlinks, and Git control position. Ignored content is **not captured by default**, because it commonly contains build products, caches, credentials, sockets, and multi-gigabyte artifacts. Every operation MUST return a typed `ExclusionReport`; any operation that could overwrite or remove excluded content MUST stop unless the user explicitly preserves or discards it.

No command may equate “not captured” with “safe to delete.” `git clean`, destructive checkout, and worktree removal require an inventory and a recoverability decision first.

### D-02 — Keep SQLite, but do not confuse database atomicity with operation atomicity

SQLite with one database in the repository’s Git common directory is the best v0.1 default because it provides schema constraints, bounded indexed queries, transactional event+projection updates, online backup, and one shared control plane across linked worktrees. Use WAL only after a capability probe proves local-filesystem locking and durable sync semantics. On unsupported/network filesystems, use rollback-journal mode if the durability probe passes; otherwise allow read-only inspection and refuse JJK-native mutations with a precise diagnostic.

Alternatives rejected for v0.1:

- per-event loose files: portable but expensive to enumerate, difficult to transactionally project, and exposed to partial directory sync;
- one append-only text log: simple until concurrent append, record tearing, compaction, indexing, migration, and arbitrary-byte fields appear;
- one database per worktree: creates divergent semantic truths and makes reconciliation a distributed-systems problem immediately.

SQLite remains challengeable: the release gate includes WAL crash, lock, filesystem, backup, and corruption drills. Failure of those drills changes the default; architecture preference is not evidence.

### D-03 — Use an intent journal plus substrate compare-and-swap

The mandated mutation protocol is:

`discover → lock → reconcile → resolve → plan → durable prepare → mutate Git/JJ/files → append events+projections → verify → commit/repair`

`durable prepare` records observed preconditions, exact intended effects, compensation data, and phase before any substrate mutation. Git refs MUST update with expected-old-OID compare-and-swap. Worktree/index/filesystem fingerprints MUST be rechecked immediately before mutation. Recovery is roll-forward when effects are safely identifiable; rollback is used only when compensation is complete and itself verifiable.

### D-04 — One writer domain is the Git common directory

All linked worktrees share one JJK database, lock namespace, operation journal, and object-retention refs located via `git rev-parse --git-common-dir`. A canonical repository identity is derived from the resolved common directory plus Git object format and an installation UUID—not from cwd, remote URL, branch, inode alone, or user-visible path.

Locks are advisory coordination among JJK processes, not protection from external Git/JJ. Therefore every write also requires substrate leases and final verification. Stale-lock recovery is based on operation state and lease validity, never PID age alone.

### D-05 — Git truth is authoritative; semantic truth is explicit

Git object/ref/index/worktree observations are immutable facts. JJK events attach meaning. JJ facts are optional capabilities and can be rebuilt/re-observed. If Git and JJ disagree and the adapter cannot prove a lossless direction, JJK stops in `needs_reconciliation`; it never silently imports from or exports to whichever command ran last.

JJ support is version/capability gated. Git-only mode is not degraded correctness; it is the v0.1 reference behavior. JJ acceleration ships only after behavioral parity fixtures pass.

### D-06 — Exact Git passthrough is a process contract, not a parser feature

A **transparent Git passthrough** MUST preserve argv as received from the OS, cwd, stdin/stdout/stderr streams, environment, TTY/process-group behavior, signals, and Git’s exit code. It MUST NOT trim output, interpret aliases, inject paging/color/editor settings, translate arguments, hold a JJK write lock, or run foreground reconciliation that changes timing/output/exit behavior. On Unix this should use direct exec where possible; otherwise use inherited stdio and transparent signal forwarding. Windows uses native wide argv and inherited console handles.

Because the exact Git command may invoke aliases, hooks, editors, credential helpers, filters, and arbitrary subprocesses, passthrough observes no trustworthy semantic effect. Reconciliation occurs on the next JJK-native or Git-enhanced operation, or via an optional out-of-band watcher. Commands in docs/UI are always labeled **JJK-native**, **Git-enhanced**, or **transparent Git passthrough**.

### D-07 — No custom content merge in the indestructible core

Atomic `pick` is represented from Git’s exact parent→state delta and delegated to Git plumbing that preserves binary blobs, modes, symlinks, renames, submodule gitlinks, and path bytes. Conflicts produce an isolated, resumable operation; no partial successful files are reported as a completed pick. AI or custom semantic merging is deferred and can only propose new isolated attempts.

### D-08 — Safety-relevant output is typed and machine-readable

Every JJK-native mutation emits an `OperationReceipt` in human form and, with `--json`, a versioned machine form. Non-interactive mode never prompts, never picks a fuzzy result, never launches an editor, and never treats a typo as a free-form state description.

## 3. Invariants

| ID | Invariant | Enforcement |
|---|---|---|
| `INV-01` | A completed operation has exactly one terminal result: `committed`, `repaired`, `aborted_clean`, or `needs_human_repair`. | Unique operation ID and terminal-state constraint. |
| `INV-02` | No committed state points to a missing Git object. | Object existence verification plus retention ref before event commit. |
| `INV-03` | No risky mutation proceeds with unreported uncaptured paths. | Mandatory `ExclusionReport`; fail closed on overwrite intersection. |
| `INV-04` | The user’s pre-existing index semantics survive save/return unless the explicit plan says otherwise. | Capture index tree, conflict stages, intent-to-add and flags; compare after. |
| `INV-05` | A JJK lock never substitutes for Git ref leases or worktree/index fingerprints. | Expected-old OIDs and pre-mutation fingerprint check. |
| `INV-06` | A state has one stable JJK ID even if labels, branches, Git refs, or JJ commit IDs move. | Separate typed IDs and immutable creation event. |
| `INV-07` | External history rewrite creates observations/supersession links; it never silently rebinds an old state ID to new content. | Old OID retained or state marked unavailable with repair path. |
| `INV-08` | Git-only and JJ-enabled modes produce equivalent JJK graph semantics for the v0.1 command set. | Differential fixture suite. |
| `INV-09` | Transparent Git passthrough is observationally equivalent to invoking the selected Git binary directly. | PTY, signal, env, byte-output, and exit-code conformance tests. |
| `INV-10` | Metadata projections are disposable; replaying valid events deterministically rebuilds them. | Projection checksum and replay test. |
| `INV-11` | Events committed in SQLite are immutable. | Deny UPDATE/DELETE on event rows outside explicit migration tooling; checksum chain detects damage, not hostile tamper. |
| `INV-12` | Linked worktrees cannot own divergent JJK databases for the same common Git directory. | Canonical common-dir discovery and duplicate-database doctor check. |
| `INV-13` | Read-only commands never acquire a write lock or mutate Git/JJ/worktree metadata. | Capability-separated APIs and syscall/fixture assertions. |
| `INV-14` | Deferred enrichment cannot make a state less durable than the receipt claims. | Commit identity/provenance synchronously; defer only stats/index/search enrichment. |
| `INV-15` | Backups are not declared valid until metadata, refs, and referenced objects can restore in a scratch environment. | Restore drill and manifest verification. |

## 4. Data and API shapes

Illustrative Rust-like records; names are contracts, not a source-code mandate.

```rust
struct OperationId(UuidV7);
struct EventId(UuidV7);
struct StateId(UuidV7);
struct AttemptId(UuidV7);

struct RepoFingerprint {
    installation_id: Uuid,
    git_common_dir_id: Vec<u8>, // canonical native path bytes, not lossy UTF-8
    object_format: GitObjectFormat, // Sha1 | Sha256 | Future(String)
}

struct WorkspaceFingerprint {
    head: Option<GitOid>,
    symbolic_head: Option<GitRefName>,
    index_checksum: Digest,
    index_tree: Option<GitOid>,
    worktree_probe: Digest,
    jj_operation: Option<JjOperationId>,
}

struct ExclusionReport {
    tracked: CaptureDisposition,
    staged: CaptureDisposition,
    unstaged: CaptureDisposition,
    untracked: Vec<PathRecord>,
    ignored: Vec<PathRecord>,
    nested_repositories: Vec<PathRecord>,
    submodules: Vec<SubmoduleDisposition>,
    special_files: Vec<SpecialFileDisposition>,
    overwrite_intersections: Vec<PathRecord>,
}

struct PreparedOperation {
    id: OperationId,
    command: NativeCommand,
    actor: Actor,
    phase: OperationPhase,
    repo: RepoFingerprint,
    before: WorkspaceFingerprint,
    plan_hash: Digest,
    intended_effects: Vec<SubstrateEffect>,
    compensations: Vec<Compensation>,
    created_at: Timestamp,
}

enum OperationPhase {
    Prepared,
    SubstrateMutating,
    SubstrateMutated,
    EventsCommitted,
    Verifying,
    Committed,
    Repairing,
    Repaired,
    AbortedClean,
    NeedsHumanRepair,
}

struct OperationReceipt {
    schema_version: u32,
    operation_id: OperationId,
    outcome: OperationOutcome,
    state_id: Option<StateId>,
    before: WorkspaceFingerprint,
    after: WorkspaceFingerprint,
    exclusions: ExclusionReport,
    effects: Vec<VerifiedEffect>,
    recovery: RecoveryInstruction,
    warnings: Vec<WarningCode>,
}

struct CapabilityReport {
    git_version: Version,
    object_format: GitObjectFormat,
    repository_form: RepositoryForm,
    filesystem: FilesystemCapabilities,
    jj: Option<JjCapabilities>,
    supported: Vec<Capability>,
    refused: Vec<RefusalReason>,
}

struct GitPassthroughRequest {
    executable: NativePath,
    argv: Vec<OsString>,
    cwd: NativePath,
    environment: InheritedEnvironment,
    stdio: InheritedStdio,
    signal_policy: TransparentForwarding,
}
```

SQLite layout requirements:

- `events`: append-only typed payloads, schema version, operation ID, causal parent, actor, repository fingerprint, checksum;
- `operations`: prepared intent and monotonic phase transitions;
- `states`, `attempts`, `edges`, `annotations`, `branch_observations`, `navigation`: materialized projections changed in the same SQLite transaction as their causal events;
- `projection_meta`: source event watermark, projection schema version, checksum;
- `migrations`: tool version, from/to schema, backup manifest, outcome;
- no Git blobs, full worktree archives, secrets, or arbitrary command output in ordinary rows.

## 5. Ranked risk register

Scoring: likelihood (`L`) and impact (`I`) are 1–5; rank is primarily `L×I`, then irreversibility. `UK` = unknown known (unstated assumption surfaced); `UU` = unknown unknown/future collision.

| Rank / ID | Class | Domain | L×I | Finding and why it matters here | Decision / mitigation | Release disposition |
|---|---|---:|---:|---|---|---|
| 1 / `R-01` | UK | Data loss | 5×5 | “Git snapshot” is not “all work.” Ignored files, nested repos, submodule worktrees, special files, and data outside cwd are absent. A return/remove could destroy something JJK never captured. | Enforce `D-01`, inventory exclusions, prohibit overwrite/removal on uncaptured intersections, and truthfully scope the promise. | **Blocker** |
| 2 / `R-02` | UU | Crash consistency | 4×5 | SQLite, Git refs, index, JJ, and files have no shared transaction. Power loss between layers can create two valid but contradictory truths. | Durable prepared intent, fsync discipline, substrate leases, phase markers, deterministic repair, fault injection after every effect. | **Blocker** |
| 3 / `R-03` | UK | Git index | 4×5 | Users rely on staged/unstaged separation, conflict stages, intent-to-add, sparse/skip-worktree flags. “Save then restore” can flatten this semantic state even when file bytes survive. | Treat the index as first-class state; snapshot/restore and compare all stages/flags. Refuse unsupported index forms rather than normalize. | **Blocker** |
| 4 / `R-04` | UU | Concurrency | 4×5 | External Git/JJ ignores JJK locks; two worktrees share refs and often metadata. Check-then-write races can move a branch after the plan was approved. | Common-dir writer domain, expected-old ref CAS, before-fingerprint leases, retry only by rebuilding/re-presenting the plan. | **Blocker** |
| 5 / `R-05` | UK | Git passthrough | 4×5 | A wrapper that captures output, parses argv, or post-processes synchronously breaks credentials, editors, pagers, hooks, binary output, Ctrl-C, and scripts. | Enforce `D-06`; byte/PTY/signal/exit conformance. No semantic claim from passthrough itself. | **Blocker** |
| 6 / `R-06` | UU | Object retention | 3×5 | A JJK state can outlive its visible branch. Without durable refs, normal reflog expiry/GC makes “reversible” states unrecoverable. Backing up only SQLite preserves pointers to nothing. | Create namespaced retention refs before committing state events; include ref/object manifest in backup; verify scratch restore; explicit GC/retention policy. | **Blocker** |
| 7 / `R-07` | UK | Git edge cases | 4×4 | Binary files, modes, symlinks, renames, path bytes, LFS pointers, and gitlinks make UTF-8 file-by-file merge unsafe. | `D-07`; use exact Git tree/diff/index plumbing and isolate conflicts. Never execute LFS filters merely to “inspect” without a disclosed network/cost effect. | **Blocker** |
| 8 / `R-08` | UU | JJ drift | 3×5 | Colocated JJ may snapshot a working copy or rewrite/export refs; versions and config change semantics. Blind import/export can erase or multiply states. | JJ capability/version matrix, observed pre/post op IDs, no ignored adapter errors, ambiguous drift stops. Git-only remains reference path. | **Blocker for JJ-enabled release**, not Git-only |
| 9 / `R-09` | UK | Repository forms | 3×5 | `.git` may be a file, common dir may be elsewhere, repo may be bare, partial/shallow, sparse, SHA-256, alternates-backed, or a submodule. Root-relative assumptions corrupt or misplace metadata. | Discover through Git plumbing; typed capability report; common-dir DB; explicit support/refusal matrix with no mutation before classification. | **Blocker** for advertised forms |
| 10 / `R-10` | UU | Security | 3×5 | Operating on an untrusted repo can invoke hooks, clean/smudge filters, aliases, credential helpers, diff drivers, submodule commands, and forge-supplied code. JJK cannot call this a sandbox. | Separate trusted local operation from untrusted harvest. Never execute candidate code by default. Show subprocess plan; sandbox validation later; document that transparent Git inherits Git’s trust boundary. | **Blocker** for PR/Fork execution; core must warn |
| 11 / `R-11` | UK | Privacy | 4×4 | Labels, messages, absolute paths, remote URLs, actor identity, environment, transcripts, doctor bundles, backups, and Timeshift can leak secrets even without file blobs. | Store relative/native paths where possible; environment deny-by-default allowlist; redact URLs/credentials; `0600` local metadata; doctor export preview; no transcript/env capture in v0.1. | **Blocker** |
| 12 / `R-12` | UU | Filesystem security | 2×5 | A malicious `.jjk` symlink or swapped worktree path can redirect writes/backups outside the repo; shared repos create ownership problems. | Refuse symlinked control roots by default, resolve and pin parent identities, safe-create with no-follow semantics, validate owner/mode, atomic temp-in-same-dir rename. | **Blocker** |
| 13 / `R-13` | UK | SQLite/WAL | 3×4 | WAL locking/durability assumptions may fail on NFS, SMB, synced folders, container mounts, disk-full, or antivirus interference. A busy timeout is not a recovery design. | `D-02`; filesystem probe, bounded lock timeout, disk-full tests, rollback-journal fallback, read-only refusal mode, online backup API. | **Blocker** |
| 14 / `R-14` | UU | History rewrite | 4×3 | Rebase/filter-repo/force-fetch replaces OIDs. Content-similarity remapping can falsely attach meaning to a different commit. | Never silently rebind; retain old objects when available, record supersession candidates with confidence and require confirmation for semantic reassociation. | **Blocker** |
| 15 / `R-15` | UK | UX/adoption | 4×3 | Free-form `jjk words` can turn a typo into a state; fuzzy return can pick the wrong similarly named state; an agent prompt can hang automation. | Known-command typo guard, exact confidence rules, choices on TTY, structured ambiguity error non-interactively, `--json --non-interactive`, no implicit editor. | **Blocker** |
| 16 / `R-16` | UU | Performance | 4×3 | Accurate `git status --untracked-files=all` and full history reconciliation violate 50 ms on monorepos, network filesystems, or millions of untracked files. Caching can lie about safety. | Split orientation freshness from mutation preflight. OID/index/refs watermarks and incremental projections for reads; mandatory fresh exact scan only before affected mutation; display freshness. | **Blocker** for stated budgets |
| 17 / `R-17` | UK | Background work | 3×4 | “Return quickly and finish expensive work later” can print success before refs/metadata/provenance are durable, then crash. | Foreground boundary includes object retention, event/projection commit, and verification. Only stats/search/enrichment may defer, visibly. | **Blocker** |
| 18 / `R-18` | UU | Remote metadata | 3×4 | Git transports refs but not a shared SQLite log. Copying DBs or merging two event logs introduces replica identity, causal conflict, privacy, and schema negotiation. | Metadata sync is a deliberate non-goal for v0.1. Preserve Git interoperability; design event IDs/causality now without claiming mergeability. | Deferred research |
| 19 / `R-19` | UK | Portability | 3×4 | Rust `String` paths and case-sensitive assumptions lose non-UTF-8 Unix paths and collide on Windows/macOS; reserved names and long paths break generated worktrees. | Use `OsString`/native path bytes internally, lossy text only for display, case/collision preflight, path-length-safe hashed directories, platform fixtures. | **Blocker** |
| 20 / `R-20` | UU | Packaging/supply chain | 3×4 | An auto-updating VCS-adjacent binary is a high-value supply-chain target and runs with access to source, credentials, and hooks. | No silent updater in v0.1; signed checksums/provenance, reproducible-build target, SBOM, pinned release automation, verified clean-machine install/uninstall. | **Blocker** |
| 21 / `R-21` | UK | Backup | 3×4 | A successful copy is not a backup. Live copying DB/WAL separately, omitting refs/objects, or restoring atop a changed repo produces false confidence. | SQLite online backup/checkpoint API, manifest of refs/OIDs/schema/tool, automatic pre-restore point, scratch restore verification, never overwrite only copy. | **Blocker** |
| 22 / `R-22` | UU | Disk pressure | 4×3 | Retaining every state object and untracked payload can grow without bound; disk-full during a “safety” command can itself interrupt writes. | Preflight free space, bounded metadata, content dedup through Git, retention report, user-directed prune with reachability preview; never auto-prune unique states. | **Blocker** for failure behavior; pruning UI may defer |
| 23 / `R-23` | UK | Hooks/config | 3×3 | Git hooks or signing may fail/modify commits; global config can change default branch, filters, line endings, fsmonitor, and commit behavior. | Do not bypass user policy silently. Inventory relevant config, distinguish plumbing-owned commits from user commits, capture stderr verbatim in receipt, test adversarial config. | **Blocker** |
| 24 / `R-24` | UU | Uninstall | 3×3 | JJK-created refs/worktrees/hooks/shell integration can linger; deleting `.jjk` first may orphan objects or remove the only recovery map. | `jjk uninstall --plan` inventories effects; default removes integration only and leaves Git-valid refs/branches. Destructive metadata/ref removal is separate and export-gated. | **Blocker** |
| 25 / `R-25` | UK | Bare repos | 2×4 | “State” and “return” imply a worktree and index, while a bare repo has neither. Pretending parity makes the model incoherent. | Bare v0.1 is read-only graph/import/export/doctor unless a worktree is explicitly provisioned. This is an intentional capability distinction. | Deliberate non-goal for mutating UX |
| 26 / `R-26` | UU | Multi-user repos | 2×4 | Unix shared repos can expose semantic metadata or let another account hold locks/replace control files. SQLite ownership is not a collaboration protocol. | Single-OS-user writer is the v0.1 safety boundary. Detect shared ownership and refuse writes unless an explicit future shared mode exists. | Deliberate non-goal |
| 27 / `R-27` | UU | Event integrity | 2×4 | A checksum chain detects accidental corruption but does not prove actor authenticity; local attackers can rewrite DB and checksums. | Claim crash/corruption detection only, not tamper-proof audit. Signed actor events and transparency logs are deferred. | Deferred research |
| 28 / `R-28` | UK | Semantic graph | 3×3 | A Git merge has multiple parents while a JJK state may have one logical parent plus composition sources. Collapsing these creates false atomic deltas and misleading topology. | Typed edges (`logical_parent`, `git_parent`, `derived_from`, `composed_from`, `supersedes`); atomic delta requires an explicit base edge. | **Blocker** |
| 29 / `R-29` | UU | Future AI | 3×3 | AI grouping/merge can create plausible but wrong provenance, execute untrusted code, or normalize private content into external prompts. | AI can propose annotations/isolated attempts only; deterministic materializer, explicit data egress, provenance, and human/policy promotion remain mandatory. | Deferred feature |
| 30 / `R-30` | UK | Vocabulary | 3×2 | `attempt`, Git branch, worktree, and legacy `lane` can become four labels for one thing, making recovery instructions ambiguous. | `attempt` is semantic, branch/worktree are mappings, `lane` absent from v0.1 public schema unless it gains a distinct invariant. | **Blocker** for public API/docs |

## 6. Failure modes and prepared responses

| Symptom | Likely condition | Required response | Forbidden response |
|---|---|---|---|
| Prepared operation exists; ref moved; no event | Crash after Git mutation | Re-observe expected OIDs and worktree. If exact intended effects exist, append/rebuild events and verify; otherwise compensate or enter `needs_human_repair`. | Delete the operation row and retry blindly. |
| Event exists; retention ref/object missing | External ref deletion/GC or incomplete restore | Search local object DB/alternates/remotes/bundle by exact OID; mark state unavailable with recovery candidates. | Rebind state to a similar commit. |
| SQLite reports busy | Live writer, orphaned operation, or bad filesystem locking | Bounded wait, show owner/operation, then inspect durable phase and leases. | Delete lock based only on elapsed time. |
| JJ import/export differs from Git | JJ snapshot/config/version drift | Stop JJ mutation, preserve both observations, give Git-only recovery or explicit reconcile plan. | Pick newest timestamp. |
| Index changed during plan | Editor/IDE/Git raced JJK | Abort clean and rebuild plan from new fingerprint. | Continue because HEAD is unchanged. |
| Excluded ignored file intersects return target | State cannot protect that content | Preserve to explicit recovery payload or require explicit discard. | `git clean` it. |
| Disk fills after object creation | State may be in Git but not JJK | Keep/ref the object if possible, write minimal repair marker, report exact OID/operation. | Claim save failed and leave object unreachable. |
| User sends SIGINT during Git subprocess | Operation may be partially mutated | Forward signal; then recovery process reads durable phase on next invocation. | Swallow SIGINT or convert Git’s exit status to zero. |
| Schema migration fails | Old DB still authoritative | Restore byte-verified pre-migration backup; never open newer projections with older writer. | Best-effort column changes in place. |
| Doctor bundle contains a secret candidate | Redaction uncertainty | Default omit, show manifest/preview, require opt-in inclusion. | Upload automatically. |

## 7. Six-month pre-mortem

Assume the rewrite failed publicly six months after release.

| ID | What happened | Earliest warning | Prepared prevention/response |
|---|---|---|---|
| `PM-01` | A user returned to an old state and lost an ignored local database. The product’s central promise became untrustworthy. | Exclusion report missing or ignored paths counted only as “clean.” | `R-01` blocker: destructive intersection inventory, explicit preserve/discard, safety-envelope wording in every receipt. |
| `PM-02` | Two agents saved from linked worktrees; the later SQLite write won while an earlier branch ref won, producing a graph state that never existed. | Multiple `.jjk` databases or ref updates without expected-old OIDs. | Common-dir DB, single writer domain, CAS refs, concurrent fault tests. |
| `PM-03` | Ctrl-C during `jjk return` left a mixed tree and the next invocation auto-reconciled it as a valid state. | Operations without durable phases; reconciliation treats any HEAD/tree as intentional. | Prepared intent, repair-before-reconcile ordering, mixed-effect detection, no inferred success. |
| `PM-04` | JJ upgraded and its colocated import behavior changed. JJK silently rewrote branch tips. | Unknown JJ version accepted; ignored import/export errors. | Capability pin/matrix, adapter quarantine, Git-only fallback, no silent errors. |
| `PM-05` | `jjk git commit` broke an interactive signing prompt and returned the wrong status, so teams abandoned passthrough. | Captured stdio or argument reconstruction in passthrough tests. | Direct exec/inherited stdio/signals; compare against native Git across PTY and scripts. |
| `PM-06` | A monorepo’s shell prompt became slow because `status` recursively scanned millions of files; users disabled JJK. | p95 exceeds budget on untracked-heavy fixture. | Cached orientation with visible freshness; exact scan only for mutation preflight; benchmark gates. |
| `PM-07` | Backups restored metadata but not pruned Git objects. “Backup succeeded” was false. | No scratch restore or referenced-object manifest. | `INV-15`, online DB backup plus bundle/ref manifest and restore drill. |
| `PM-08` | PR Harvest ran a malicious project’s test script and exfiltrated credentials. | Candidate validation shares host env/network/home. | PR/Fork execution remains unshipped until sandbox/egress/secrets gates pass. Discovery is metadata-only. |
| `PM-09` | Early adopters could not upgrade because event payloads and plugin API froze accidental v0.1 internals. | Public raw DB/schema promises or plugins writing core tables. | Versioned read API; no write plugins/remote sync in v0.1; migration fixtures from every released schema. |
| `PM-10` | Safety accumulated invisible disk use until CI/dev machines filled, causing unrelated failures. | No object-retention/disk forecast in doctor/status. | Preflight space, retention accounting, user-driven prune/export; never silent automatic loss. |

**Could JJK be bad? Yes.** A tool that promises safety and sits above Git can increase harm if it causes users to take risks they would otherwise avoid. It also widens access to source, Git credentials, hooks, remote metadata, and eventually transcripts/environments. The architecture must earn increased trust through narrower truthful guarantees, not comforting language. v0.1 is unacceptable if it cannot distinguish captured from excluded work, repair partial mutations, and remain removable while leaving ordinary Git valid.

## 8. Future scenarios played 10+ steps ahead

### FS-01 — Probable adoption path: solo user becomes a multi-agent maintainer

1. User installs a signed JJK binary and initializes an existing SHA-1 Git repository.
2. Capability discovery finds a linked worktree layout and puts one DB in the Git common directory.
3. Existing commits become observations; no commits or refs are rewritten during import.
4. The user saves a state with staged, unstaged, and untracked files; ignored secrets appear in exclusions.
5. The user returns to history; JJK proves ignored paths do not intersect the overwrite or requires preservation.
6. Saving from history creates a sibling attempt and retention ref; the old future remains reachable.
7. The user invokes transparent Git passthrough to rebase; JJK neither parses nor “fixes” it.
8. The next JJK-native status detects rewritten OIDs and records supersession candidates without rebinding state IDs.
9. Two agents request worktrees concurrently; the shared lock serializes semantic preparation while Git ref CAS prevents external races.
10. One agent’s ref lease fails after a human Git push/switch; its plan aborts and is rebuilt rather than replayed blindly.
11. The maintainer picks one exact delta; a binary conflict creates a resumable isolated operation, not partial completion.
12. Promotion verifies policy evidence and atomically CAS-updates the canonical ref.
13. Backup uses SQLite online backup plus refs/object manifest and passes scratch restore.
14. Uninstall preview shows branches, retention refs, worktrees, shell hook, and metadata; default removal leaves a comprehensible Git repository.

**Early warning:** any step lacks a typed receipt or changes a ref without old-OID evidence.  
**Prepared response:** stop release; the apparent happy path is not evidence without races, rewrite, excluded-content, and uninstall coverage.

### FS-02 — Improbable but severe: crash, disk exhaustion, network filesystem, and hostile repository collide

1. A repository lives on SMB and contains a symlink named `.jjk` pointing outside the checkout.
2. Discovery refuses the symlinked control root before opening or creating metadata.
3. The user chooses a safe external local metadata placement bound to the common-dir fingerprint.
4. WAL probe fails on SMB; rollback-journal durability probe also fails, so mutations remain disabled rather than “best effort.”
5. The repo is moved locally and passes the probe; JJK prepares a return while an IDE updates the index.
6. Fingerprint recheck catches the index race and aborts before mutation.
7. Retry records durable intent, then Git runs a checkout hook configured by the repository.
8. The hook modifies another tracked file and consumes the remaining disk.
9. JJK cannot append full events, but the minimal prepared record and Git reflog/OIDs identify the partial effects.
10. The user sends Ctrl-C; the signal reaches the subprocess, and JJK does not print success.
11. Next invocation enters repair before ordinary reconciliation, detects hook-added content and disk pressure, and refuses automatic rollback because compensation would overwrite unknown work.
12. `doctor repair --plan` offers an external recovery destination, exact refs/OIDs, and excluded paths without executing repo code.
13. After space is freed, the user preserves the hook output, JJK completes/aborts the operation, verifies all fingerprints, and appends the terminal repair event.
14. A doctor bundle previews redacted paths/config and omits credentials and arbitrary hook output by default.

**Early warning:** mutation enabled on an unprobed filesystem, symlink-following control files, or “automatic repair” despite unrecognized changes.  
**Prepared response:** fail closed and preserve evidence; inconvenience is preferable to manufacturing a clean but false state.

### FS-03 — Future pressure: remote metadata, AI composition, and Timeshift arrive

1. v0.1 ships local event IDs, causal parents, actors, and typed edges without claiming distributed merge.
2. Two collaborators exchange ordinary Git branches; neither needs JJK for valid Git use.
3. Both annotate the same Git object differently in separate local databases.
4. A later remote-metadata prototype attempts synchronization and discovers concurrent annotations and schema skew.
5. Because labels are annotations rather than identity, both can coexist without mutating the Git object or state ID.
6. The sync layer proposes a conflict policy and records replica provenance; it cannot write core tables directly.
7. PR Radar discovers an untrusted fork using metadata-only APIs and does not fetch/execute by default.
8. Feature Harvest fetches into an isolated object/worktree boundary only after user approval and trust classification.
9. An AI service proposes three semantic compositions, with explicit outbound-data manifest and immutable source hunk links.
10. Deterministic materialization creates three sibling attempts; AI output never advances a canonical branch.
11. Validation runs in a bounded sandbox; evidence records exact command/image/input digest rather than “tests passed.”
12. A human promotes one attempt using a CAS policy gate and can roll back to the prior canonical tip.
13. Timeshift capture later asks to include shell state; environment is deny-by-default and secrets/transcript are excluded unless component-specific consent is given.
14. Restore on another OS reports repository success but marks terminal/editor/process components unsupported; it never claims full restoration.

**Early warning:** sync assumes total ordering, AI writes canonical refs, plugins write SQLite, or Timeshift serializes the ambient environment.  
**Prepared response:** keep each feature behind a capability boundary; ship partial truthful restore/proposal rather than universal automation.

## 9. Release-blocking unknowns

These questions need executable evidence before v0.1 can be called stable. The recommended default is the decision unless evidence disproves it.

| ID | Unknown | Required evidence / acceptance check |
|---|---|---|
| `B-01` | Can prepared operations always classify and repair every crash point? | Kill/power-loss fault injection after each protocol effect; exact before/after/ref/index/worktree assertions; no unclassified terminal state. |
| `B-02` | What exactly is captured, excluded, and overwritten for every command? | Matrix across tracked/staged/unstaged/untracked/ignored/conflicted/sparse/submodule/nested/special files; typed report golden fixtures. |
| `B-03` | Does common-dir discovery remain unique across normal, linked worktree, submodule, bare, symlinked, alternate-object, SHA-256, and moved repos? | Fixture matrix; duplicate DB detector; no root-relative `.git` assumptions. |
| `B-04` | Does transparent passthrough truly preserve native behavior? | Differential tests for arbitrary/non-UTF-8 argv where supported, env, cwd, binary stdout, stderr interleaving, TTY, pager, editor, credential/signing prompt, SIGINT/SIGTERM, and exit 0/1/128. |
| `B-05` | Is SQLite mode durable on supported filesystems and failure conditions? | WAL/rollback mode crash tests, two-writer contention, disk full, read-only, permission failure, WAL loss, online backup during writes, corruption detection. Publish support matrix. |
| `B-06` | Can state objects survive ordinary Git maintenance and restore elsewhere? | Aggressive reflog expiry/GC followed by state access; bundle/backup scratch restore with no source repo; missing-object diagnostic test. |
| `B-07` | Is exact atomic pick correct for all Git object changes? | Parent→state fixtures for binary, rename/copy, mode, symlink, submodule, LFS pointer, empty tree, merge state with explicit base, conflict/resume/abort. |
| `B-08` | Can external rewrites reconcile without false semantic identity? | Rebase, amend, squash, filter-repo, branch delete, force-fetch, replace/graft, shallow deepen; prove no silent state-ID rebinding. |
| `B-09` | Are security/privacy boundaries truthful? | Malicious hooks/filters/aliases/config/symlinks; permission tests; doctor/backup redaction corpus; prove no ambient env/transcript storage. |
| `B-10` | Do performance budgets hold without stale safety decisions? | p50/p95/p99 on small, 1k-state, large-history, monorepo, many-worktree, untracked-heavy, and network/read-only fixtures; distinguish cached orientation from fresh mutation preflight. |
| `B-11` | Can every released schema upgrade and roll back? | Golden databases from legacy JSON and every prerelease/release schema; interrupted migration; byte-verified backup; older-binary refusal. |
| `B-12` | Does Git-only/JJ parity actually hold? | Same command trace produces equivalent semantic graph/receipts in Git-only and every advertised JJ version; divergent behavior disables JJ capability. |
| `B-13` | Can install/update/uninstall be trusted on each advertised target? | Clean macOS arm64/x64, Linux glibc/musl targets actually shipped, Windows/WSL targets actually shipped; signature/checksum/provenance verification and offline uninstall. |
| `B-14` | Does automation remain deterministic and noninteractive? | JSON schema contract, ambiguity errors, no editor/pager/prompt, stable exit taxonomy, concurrent-agent fixture. |

A release candidate with an unanswered blocker may be labeled experimental only; the scope cannot be silently narrowed in marketing after failure.

## 10. Deferred research and deliberate non-goals

| ID | Topic | v0.1 position | Revisit trigger |
|---|---|---|---|
| `DNR-01` | Remote mergeable JJK metadata | Local only; Git refs/commits remain interoperable. Do not copy/merge SQLite files. | Two-real-user prototype defines replicas, causal conflict, encryption, schema negotiation, deletion, and offline behavior. |
| `DNR-02` | Semantic/AI merge | Suggestions and isolated attempts only; no canonical mutation. | Deterministic provenance materializer and sandbox/data-egress gates exist. |
| `DNR-03` | PR Radar/Feature Harvest execution | Metadata discovery may prototype; execution is not core v0.1. | Threat model and sandbox prove secret/network/home/process isolation. |
| `DNR-04` | Timeshift environment/transcript/process restore | Not captured in v0.1. Repository state only, with extensible component manifest. | Per-component consent, secret-exclusion tests, portable capability reporting. |
| `DNR-05` | Shared multi-user OS repositories | One OS-user writer boundary. | ACL, identity, lock, encryption, and audit protocol designed and tested. |
| `DNR-06` | Bare-repo mutation | Read-only graph/import/export/doctor only. | A concrete worktree-provisioning UX is approved. |
| `DNR-07` | Write-capable plugin SDK | No plugin writes to core DB/events. Stable read API only when useful. | Core event/schema has survived migrations and capability/permission model exists. |
| `DNR-08` | Tamper-proof audit/signatures | Checksums detect accidental corruption only. | Threat model needs actor non-repudiation or remote trust. |
| `DNR-09` | Automatic pruning | Never automatically delete unique state content. | Retention/export UX proves reachability and reversible quarantine. |
| `DNR-10` | Silent auto-update | Not allowed. | TUF-like rollback/freeze protection and administrator policy exist. |
| `DNR-11` | Perfect branch assignment for shared ancestors | Git commits can belong to many refs; graph stores reachability, not one invented owner. | No revisit needed; this is a model correction. |
| `DNR-12` | Universal network-filesystem mutation | Read-only/refusal is acceptable where durability cannot be proven. | A supported filesystem/locking implementation passes the same crash suite. |

## 11. Cool but bounded value additions

These improve trust without pulling later roadmap layers into v0.1.

| Rank / ID | Addition | Value | Bound |
|---|---|---|---|
| 1 / `VA-01` | `jjk plan <command…>` | Shows exact refs, paths, exclusions, adapter calls, leases, and recovery before mutation; makes the transaction model visible. | Pure read; output is the same typed plan consumed by execution, not a second planner. |
| 2 / `VA-02` | Safety receipt + `jjk explain <operation-id>` | Lets humans/agents answer what happened, what was protected, and how to recover without log archaeology. | Local structured receipt; no telemetry or cloud service. |
| 3 / `VA-03` | `jjk doctor --compat` fingerprint | Reports repository form, object format, filesystem/SQLite mode, Git/JJ versions, hooks/filters, support/refusal reasons. | Never executes project code; redacted by default. |
| 4 / `VA-04` | Restore rehearsal | Restores a chosen backup into a temporary scratch location and verifies graph/objects without touching the active repo. | Manual, local, disposable; no scheduler/daemon. |
| 5 / `VA-05` | `jjk uninstall --plan` | Makes the universal-Git/no-hostage promise demonstrable. | Inventory and safe default removal only; unique state deletion remains separate. |
| 6 / `VA-06` | Compatibility capsule | A small, secret-redacted manifest users can attach to issues: versions, capabilities, repo form, operation ID, phase, checksums—not content. | Preview required; no automatic upload. |
| 7 / `VA-07` | Graph truth export | Deterministic JSON export of states, typed edges, availability, and freshness lets future TUI/GUI/IDE share one truth. | Read-only versioned API; no daemon required in v0.1. |
| 8 / `VA-08` | Safety budget meter | Status reports retained object bytes, DB/WAL bytes, oldest unique state, and projected backup size before disk pressure becomes failure. | Advisory accounting; never prunes automatically. |

## 12. Acceptance checks for this oracle’s decisions

The architecture and implementation are not ready if any answer is “we probably handle it.”

1. Every `R-01`–`R-30` entry has a mitigation, blocker, deferred gate, or explicit non-goal.
2. The public safety promise exactly matches `D-01` and the machine-readable `ExclusionReport`.
3. The operation design implements the full mandated protocol and makes every transition durable/recoverable.
4. SQLite WAL remains conditional on filesystem evidence, and one DB is shared through Git common-dir discovery.
5. Transparent Git passthrough satisfies `D-06` without hidden reconciliation or altered process behavior.
6. Git-only mode passes all core release gates; JJ mode is enabled only for tested version/capability combinations.
7. External history rewrite never silently transfers a stable state ID to new content.
8. Index, binary/mode/symlink/path-byte/submodule cases are tested as first-class content, not incidental files.
9. Backups pass restore rehearsal after aggressive Git GC with the source checkout unavailable.
10. Security tests prove control-path symlinks, repo hooks/config, doctor bundles, permissions, and untrusted candidates fail safely.
11. Benchmarks publish both cached-read freshness and exact mutation-preflight cost; no stale read authorizes a write.
12. Packaging claims name only targets verified from clean machines, with signatures/checksums/provenance and safe uninstall.
13. Deferred research remains mechanically distinguishable from stable commands and cannot advance canonical refs.
14. A user without JJK can clone/use the resulting Git repository, and removing JJK integration does not invalidate its branches or commits.

## 13. Explicit non-goals of this document

- It does not select CLI spelling beyond safety-relevant distinctions.
- It does not prescribe UI layout, graph rendering, or visual design.
- It does not define the complete event catalog or SQLite DDL; it fixes the invariants those designs must satisfy.
- It does not promise capture of ignored files, processes, editor state, environment, transcripts, remote metadata, or untrusted-code execution in v0.1.
- It does not treat Git hooks/config/JJ as sandboxed or harmless.
- It does not make “append-only” synonymous with tamper-proof.
- It does not permit safety claims to be weakened because a failure is rare; improbable irreversible loss outranks ordinary convenience.

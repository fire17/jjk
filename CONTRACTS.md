# JJK — Acceptance Contracts

These contracts are the release gates for the rewrite. A claim is complete only when its proof command has run against the release artifact and its evidence is recorded.

## Product contract

JJK turns a Git working directory into a safe space for stateful, reversible human-and-agent development while leaving a valid, understandable Git repository behind. Git remains the durable object and transport substrate. Jujutsu is optional. JJK owns semantic states, attempts, provenance, navigation, validation evidence, and recovery.

## Command classes

| Class | Rule | Examples |
|---|---|---|
| JJK-native | Deliberately non-Git vocabulary; implements semantic state, topology, curation, recovery, collaboration, and operation behavior | `setup`, `save`, `step`, `nice`, `star`, `unstar`, `see`, `return`, `pick`, `fork`, `freeze`, `current`, `story`, `back`, `forward`, `up`, `down`, `archive`, `recover`, `undo`, `redo`, `backup`, `load`, `handoff`, `validate`, `doctor`, `completion` |
| Git-enhanced | Uses a Git name only when JJK deliberately adds state-aware value; the exact flag grammar is owned by the versioned routing registry | `status` |
| Git passthrough | Every unenhanced Git command is executed by the real Git binary with original argv bytes, cwd, environment, stdio/TTY, signals, and exit status | `init`, `clone`, `rebase`, `merge`, `fetch`, `remote`, `config`, unknown future Git verbs |

Routing invariant: an argv sequence not claimed by the versioned JJK-native/enhanced registry is passthrough, not an error and not guessed behavior.

## Hard release gates

| ID | Observable contract | Required proof |
|---|---|---|
| VAL-CORE-001 | `jjk setup` on an existing Git repository imports reachable commits and refs idempotently without changing HEAD, index, tracked bytes, untracked bytes, ignored bytes, or user Git config. | Before/after repository fingerprint fixture; second setup produces no new semantic facts. |
| VAL-CORE-002 | A state capture creates one stable JJK state identity backed by a reachable Git object, with label, description, kind, actor, logical parent, attempt, workspace, stats, provenance, and operation identity. | CLI JSON contract plus database/event replay equality. |
| VAL-CORE-003 | Returning to a historical state restores its exact tree and preserves every descendant future. The next capture creates a sibling attempt only when divergence actually occurs. | Green→purple; return green→orange fixture; both tips remain reachable and visible. |
| VAL-CORE-004 | `pick` applies exactly the source logical-parent→source-state delta, never the source's accumulated ancestry. | Purple+fast to orange yields orange+fast, never purple+fast; patch identity and source/base provenance recorded. |
| VAL-CORE-005 | Delete/archive hides but does not erase. Recover restores original topology. Undo/redo restores the complete JJK+Git control state. | Archive/recover and whole-control-state round-trip fixtures. |
| VAL-CORE-006 | `star [state]` and `unstar [state]` change only the memorable-anchor annotation on an existing state, are idempotent, and never create a duplicate snapshot. | CLI JSON contract plus unchanged state count and durable `current`/`see`/`story` projection checks. |
| VAL-CORE-007 | Control snapshots contain only Git-visible paths (index entries plus untracked, non-ignored files). Navigation and recovery remove only paths JJK captured or tracks; uncaptured files — untracked extras created after a capture and all ignored content — survive `return`, `up`, `down`, `undo`, `redo`, and conflict abort. | Extras-survive fixtures in staged and never-staged repositories; a 20 MB ignored artifact leaves the control database under 5 MB and survives `return`. |
| VAL-TXN-001 | Every mutating operation is recoverable across a process crash at every durable boundary. | Fault injection at each transaction state; reopen deterministically rolls back or completes forward. |
| VAL-TXN-002 | Recovery never overwrites bytes or refs changed externally after the recorded preimage. | Concurrent external-change fixture returns `recovery_required` and preserves the external state. |
| VAL-TXN-003 | Multiple readers proceed concurrently; conflicting writers serialize or return a typed bounded-time conflict. No silent lost update. | Multi-process stress fixture with event/projection version checks. |
| VAL-GIT-001 | Git remains usable without JJK before, during, and after JJK operations. Removing JJK metadata leaves valid commits, branches, worktrees, refs, remotes, and working files. | Native Git fsck/status/log/worktree checks before and after uninstall/export. |
| VAL-GIT-002 | Unenhanced commands are transparent passthrough. | Differential corpus compares `git ARGS` vs `jjk ARGS` stdout bytes, stderr bytes, exit code, TTY behavior, signal termination, cwd effects, and side effects. |
| VAL-GIT-003 | `jjk status` is the deliberate enhanced command: it reports native Git status truth plus current JJK state/attempt and recovery condition without suppressing Git detail. | Golden TTY, `NO_COLOR`, narrow-width, non-TTY, and `--json` fixtures. |
| VAL-GIT-004 | External Git commits, branch moves, worktree changes, fetches, rebases, and merges reconcile as immutable observed facts or explicit ambiguity; JJK never invents ancestry. | Differential Git-only mutation corpus and repeated reconcile idempotency. |
| VAL-JJ-001 | Git-only mode is complete. When colocated JJ is present, capability use is explicit and parity-tested; missing/broken JJ degrades loudly to Git-only before mutation. | Same golden workflows in Git-only and JJ modes; capability report names active adapter. |
| VAL-GRAPH-001 | State graph is acyclic on logical-parent edges, permits explicit composition edges, separates state/attempt/Git/JJ/operation identities, and produces deterministic traversals. | Property tests plus golden orange/purple graph. |
| VAL-GRAPH-002 | CLI, JSON API, TUI/GUI adapters consume the same graph/query/action model; no surface reconstructs semantics independently. | Schema contract and cross-surface golden identity/topology equality. |
| VAL-AGENT-001 | Concurrent agents receive isolated worktrees and exclusive workspace ownership. Integration requires a declared boundary. Worktrees are never deleted automatically while unique work exists. | Parallel multi-process scenario with lease expiry and dead-worker recovery. |
| VAL-AGENT-002 | Agent handoff is typed: owner, objective, base state, produced state, validation evidence, remaining risks, and exact resume command. | JSON schema fixture and resume smoke flow. |
| VAL-MIG-001 | Current `.jjk/repo.json` version 1, histories, refs, backups, freezes, navigation, lanes, and timeshifts import once, preserve provenance, and can roll back to the prior installation. | Golden migration corpus from current JJK plus byte/checksum manifest. |
| VAL-BACKUP-001 | Backup/load and freeze are distinct, checksummed, previewable, and restore exact declared scope. Load always creates a pre-load recovery point. | Disaster drill after metadata loss, ref loss, and interrupted load. |
| VAL-UX-001 | A new user can complete setup→capture→see→return and explain the six-verb loop within five minutes. | Fresh-user protocol, n≥3, 3/3 completion without intervention. |
| VAL-UX-002 | Terminal output remains legible at 40/80/120 columns, with `NO_COLOR`, common color-vision deficiencies, non-TTY pipes, and machine JSON. Current state is not encoded by color alone. | Snapshot and accessibility checks. |
| VAL-PERF-001 | Warm `current` and `status` p95 <50 ms on the representative ordinary repository fixture. | Hyperfine-equivalent benchmark, ≥50 warm samples, raw results retained. |
| VAL-PERF-002 | Return/fork planning emits feedback p95 <100 ms; graph first paint p95 <100 ms at 1,000 states. | Release-binary benchmark on stated hardware and fixtures. |
| VAL-PERF-003 | Passthrough adds p95 <5 ms process overhead beyond native Git for local no-op commands. | Paired benchmark with interleaved order and confidence interval. |
| VAL-PERF-004 | Hot orientation paths perform bounded metadata reads and no full repository/history scan. | Trace/count assertion plus large-history fixture. |
| VAL-SEC-001 | Paths, symlinks, hooks, Git config, remotes, environment, backup manifests, and timeshift adapters cannot escape declared roots or leak secrets by default. | Adversarial corpus; secret canaries absent from bundles/doctor output. |
| VAL-RELEASE-001 | Release is a single versioned binary/library with deterministic schema migrations, shell completions, source build, and verified install/uninstall instructions. | Clean macOS/Linux/Windows-or-WSL install matrix before advertising a channel. |
| VAL-SOURCE-001 | Every founding requirement is mapped to implementation and proof; no stable claim is merely planned or mocked. | Requirements matrix audit against `VISION.md`, `origins.md`, and recovered `vision_overhaul.md`. |

## Representative fixtures

1. Empty directory becoming a Git+JJK safe space.
2. Existing SHA-1 repository with staged, unstaged, untracked, ignored, executable, symlink, rename, conflict, and subdirectory invocation states.
3. Existing SHA-256 repository where supported by installed Git.
4. Bare repository (inspection only unless an operation explicitly supports it).
5. Linked worktrees with branch ownership contention.
6. Monorepo with deep invocation path and large index.
7. Submodule and nested-repository boundaries.
8. Git-only and colocated-JJ variants.
9. Green→purple / green→orange / purple→fast / exact fast-only pick onto orange.
10. Current `repo.json` v1 corpus with deleted states, navigation, lanes, freezes, backups, and timeshifts.
11. Remote/fork/upstream simulation with force-push, rewritten branch, and PR-ready projection.
12. Crash matrix at every operation state and durable-write boundary.

## Performance measurement rules

- Release binary only; debug timings are not evidence.
- State hardware, filesystem, Git/JJ versions, repository fixture, warm/cold condition, and sample count.
- Report median, p95, max, and raw samples; never quote a best run.
- Separate JJK overhead from underlying Git work.
- A missed budget blocks the release or requires an explicit contract change—never silent relaxation.

## Stable v0.1 scope

Stable v0.1 must fully implement and prove: setup/reconcile, capture (`save`/`step`/`nice`), graph (`see`/`current`/`status`/`story`), exact return/navigation, fork/worktree, exact atomic pick, archive/recover, whole-control undo/redo, backup/load/freeze, typed validation/handoff, enhanced status, transparent Git passthrough, migration from current v1 metadata, Git-only operation, and explicit optional-JJ capability reporting.

PR Radar, Feature Harvest, semantic multi-candidate composition, functional-history projections, full terminal/editor/conversation Timeshift, remote metadata service, and GUI are research/experimental unless their own contracts are implemented and proven. No placeholder command appears in stable help.

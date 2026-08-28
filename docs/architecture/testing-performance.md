# Testing and Performance Architecture

**Status:** decision-grade architecture for the JJK v0.1 rewrite  
**Scope:** conformance, property, fault-injection, compatibility, performance, distribution, and end-to-end validation  
**Authority:** `VISION.md`, `vision_overhaul.md`, and the historical JJK source/tests. This document defines proof; it does not weaken product invariants to match the old implementation.

## 1. Context

JJK is a semantic state layer above Git and, when present, Jujutsu. Its most dangerous failures are plausible-looking partial successes: Git moved but the journal did not, a projection advanced while a sibling ref disappeared, `pick` imported ancestry instead of one atomic delta, or transparent Git passthrough changed process behavior. A green unit suite is therefore insufficient. Release proof must observe the repository through all three views:

1. filesystem and process behavior visible to the caller;
2. independent Git/JJ plumbing commands;
3. JJK's event journal, materialized projections, and public query output.

All mutating tests exercise the protocol:

`discover → lock → reconcile → resolve → plan → durable prepare → mutate Git/JJ/files → append events+projections → verify → commit/repair`

The historical TypeScript suite is a regression source, not the oracle. Its 93-test baseline and fixtures are ported into named v0.1 contracts, then strengthened with state-machine, crash, concurrency, and differential tests.

## 2. Decisions

### TST-D001 — Layered proof pyramid

| Layer | Purpose | Runs |
|---|---|---|
| Pure unit | parsers, graph algorithms, resolvers, migration transforms, event reducers | every local test run |
| Model/property | generated state graphs and command sequences checked against a small reference model | every PR with fixed seeds; extended nightly |
| Adapter contract | same typed Git/JJ cases against real binaries | every PR on primary OS; matrix nightly |
| Conformance | repository-shape and historical-failure fixtures through the compiled CLI | every PR |
| Fault/recovery | kill or fail at every mutation seam, restart, repair, and compare | every PR for short matrix; exhaustive nightly |
| Compatibility/differential | transparent Git passthrough versus the same Git executable; Git-only versus JJ-backed semantic outcomes | every PR core corpus; matrix nightly |
| E2E/distribution | clean-machine installation, shell integration, upgrade, uninstall | release candidates |
| Benchmarks | latency, scaling, memory, metadata growth, lock contention | PR smoke; dedicated release host for gates |

A release is blocked by any required `VAL-*` failure. Tests are not allowed to silently skip because `git`, `jj`, a shell, or an installer is missing. The harness reports a prerequisite failure; matrix configuration decides whether that target is required.

### TST-D002 — Hermetic real-repository harness

Each test receives a unique `SandboxId` and creates every repository, remote, HOME, XDG directory, Git config, SSH wrapper, credential helper, hooks directory, and JJK data directory beneath one temporary root. It must never read or mutate the developer's global Git/JJ configuration.

```rust
struct Sandbox {
    id: SandboxId,
    root: PathBuf,
    home: PathBuf,
    xdg: PathBuf,
    repos: BTreeMap<RepoId, RepoFixture>,
    clock: TestClock,
    git: ToolBinary,
    jj: Option<ToolBinary>,
    evidence: EvidenceDir,
}

struct CommandObservation {
    argv: Vec<OsString>,
    cwd: PathBuf,
    env_delta: BTreeMap<OsString, Option<OsString>>,
    stdin: Vec<u8>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit: ExitStatus,
    termination: Termination,
    elapsed: Duration,
}
```

Mandatory process environment:

```text
HOME=<sandbox>/home
XDG_CONFIG_HOME=<sandbox>/xdg/config
XDG_STATE_HOME=<sandbox>/xdg/state
GIT_CONFIG_NOSYSTEM=1
GIT_CONFIG_GLOBAL=<sandbox>/gitconfig
GIT_TERMINAL_PROMPT=0
GIT_ASKPASS=<sandbox>/bin/fail-if-called
LC_ALL=C
TZ=UTC
NO_COLOR=1              # except explicit color/PTY cases
JJK_TEST_CLOCK=<fixture-controlled clock socket>
```

Commit identities, timestamps, terminal width, color capability, path roots, and random seeds are explicit fixture inputs. Waiting uses child-process readiness pipes, lock acquisition notifications, filesystem event acknowledgements, or bounded `waitpid`; no correctness test uses `sleep` as synchronization.

### TST-D003 — Typed fixture corpus

Fixtures are builders plus small checked-in declarative manifests. Git object IDs and platform paths are discovered during setup, never hard-coded.

```rust
enum RepoShape {
    Unborn,
    SingleBranch,
    DivergedBranches,
    DetachedHead,
    LinkedWorktree,
    Bare,
    SubmoduleSuperproject,
    SubmoduleRepository,
    Monorepo,
    ShallowClone,
    PartialClone,
    SparseCheckout,
    MergeInProgress,
    RebaseInProgress,
    ForkWithUpstream,
    ColocatedJj,
    NetworkFilesystem,
}

struct FixtureSpec {
    id: FixtureId,
    shape: RepoShape,
    object_format: ObjectFormat,       // Sha1 or Sha256 where supported
    files: Vec<FileSpec>,              // text, binary, symlink, executable, non-UTF-8 path
    history: Vec<GitAction>,
    workspace: WorkspaceState,         // staged, unstaged, untracked, ignored, conflicts
    metadata_schema: Option<SchemaVersion>,
    expected: FixtureOracle,
}
```

#### Required repository-shape matrix

`fixtures/repos/` must generate all of the following:

- empty directory and unborn Git repository;
- ordinary existing history, root commit, merge commit, octopus history, tags, notes, replace/graft refs, and detached HEAD;
- branch names containing slash, Unicode, shell metacharacters, leading dashes where Git permits them, and case-colliding names on case-sensitive hosts;
- clean, staged-only, unstaged-only, untracked-only, ignored-only, mixed, conflicted, and intent-to-add workspaces;
- linked worktrees sharing one common Git directory, worktree-local config, locked/prunable worktrees, and a worktree whose path contains spaces and Unicode;
- bare repositories (read/query/pass-through only unless an operation explicitly supports bare mutation);
- submodule superproject and direct operation inside a submodule; JJK discovery must never confuse their roots;
- monorepo with 100k tracked paths, nested packages, sparse checkout, and pathspec-heavy operations;
- shallow and partial clones, alternates, promisor remote unavailable, and missing objects;
- fork with `origin` and `upstream`, upstream advancing, force-updated remote-tracking ref, replay conflict, and PR projection;
- colocated JJ, JJ absent, JJ too old, JJ command failure, and a valid Git repo with unrelated `.jj` data;
- SHA-1 and Git SHA-256 object formats on matrix entries whose Git supports SHA-256;
- text, binary, large-file pointer, executable-bit, symlink, empty file, CRLF, Unicode content, and non-UTF-8 filename cases on Unix;
- merge/rebase/cherry-pick/revert/bisect in progress: read-only and transparent Git passthrough remain available; JJK-native mutations stop with an exact, non-destructive diagnosis unless explicitly designed for that state;
- read-only metadata directory, full disk simulation, permission denial, stale lock, process death, and network-filesystem latency/error injection.

### TST-D004 — Historical failure corpus is permanent

Each historical bug gets a stable fixture and failure ID. A fix may add a smaller unit test, but must not delete the reproducing conformance case.

| Failure ID | Fixture and forbidden regression |
|---|---|
| `HF-SNAKE-001` | canonical green → purple/orange → fast-purple → pick flow; final file is orange and fast, never purple |
| `HF-BRANCH-002` | purple and orange are siblings rooted at green; fast-purple extends purple; picking into orange does not move purple |
| `HF-MAIN-003` | JJK-native saves on attempts never advance `main` unless an explicit promotion/update targets it |
| `HF-MESSAGE-004` | every JJK-created interoperable Git commit has a non-empty meaningful subject/body; internal snapshots do not pollute ordinary Git history by default |
| `HF-LEAF-005` | leaf markers derive from branch/attempt tips; a historical state on a continuing attempt is not marked a leaf, while a canonical tip remains one even if other descendants exist |
| `HF-RETURN-006` | returning to an attempt tip attaches to its branch; returning to a historical state restores its aggregate tree into HEAD, index, and worktree with a clean index while preserving the old tip. Exact historical staged/unstaged partition is restored only when a state carries an explicit Timeshift workspace capsule; ordinary states never claim it |
| `HF-DIVERGE-007` | navigation alone creates no branch; the first state-making action after historical return creates a sibling at the returned state, not at the previous tip |
| `HF-PICK-008` | pick makes the new `cherry` current and records base, source, source parent, patch identity, and conflict resolution |
| `HF-UPDATE-009` | moving a branch/state mapping updates Git and JJK consistently without duplicate state IDs for one semantic fact |
| `HF-UNDO-010` | undo/redo restores the exact prior control snapshot—refs, HEAD attachment, index stages, worktree bytes, current state, and projection—not merely the last row |
| `HF-IMPORT-011` | init imports existing commits in topological/chronological order with correct ancestry and branch tips; external Git commits reconcile once and only once |
| `HF-RETURN-DIRTY-012` | clean/staged-only navigation creates no auto-state; work that would be overwritten is preserved exactly once |
| `HF-RENDER-013` | multiline/long labels cannot break terminal rows; current, leaf, star, branch color, and filtered/incomplete indicators remain truthful |

The canonical snake fixture has four independent file assertions: `color=orange`, `fast=true`, purple's source state is unchanged, and the patch-id of the picked delta equals the parent→fast-purple delta.

### TST-D005 — Golden outputs are semantic and terminal-aware

Goldens are checked for:

- JSON event and graph API schemas;
- `jjk see`, `story`, `show`, `diff`, resolver choices, plans, diagnostics, repair reports, and `doctor --bundle` manifests;
- terminal widths 40, 80, 120, and 200 columns; color off/on; non-TTY/TTY; light/dark-neutral ANSI semantics;
- help/completions for every declared stable command;
- Git/JJ mutation plans and audit event streams.

Normalization may replace only declared volatile fields (`$ROOT`, wall-clock instant, PID, dynamically discovered OID) while preserving field presence, topology, order, ref names, exit status, whitespace, and ANSI role. Every normalizer is unit-tested with a counterexample proving it cannot erase a meaningful difference. JSON goldens are canonicalized by key order but arrays retain domain order. Golden updates require `JJK_ACCEPT=1 cargo test --test goldens -- <fixture>` and review of the generated unified diff; CI never accepts automatically.

### TST-D006 — Property and model-based testing

A small in-memory reference model owns only externally promised semantics; it does not share reducers or graph algorithms with production code.

```rust
struct ModelState {
    states: BTreeMap<StateId, ModelRecord>,
    refs: BTreeMap<RefName, StateId>,
    current: Cursor,
    workspace: WorkspaceImage,
    events: Vec<ModelEvent>,
}

enum Action {
    ExternalGitCommit,
    ExternalBranchMove,
    Save,
    Step,
    Nice,
    Star,
    Return,
    Fork,
    AtomicPick,
    Delete,
    Recover,
    Undo,
    Redo,
    BackupLoad,
    Reopen,
}
```

Generated sequences run against model and compiled CLI. After every action, the harness checks:

- every visible state resolves to one reachable Git object or an explicitly degraded/missing-object status;
- state IDs are stable and unique; labels are mutable and never identity;
- logical parentage is acyclic; composition edges do not impersonate ancestry;
- all refs not named in the mutation plan are byte-for-byte unchanged;
- sibling futures and source attempts are monotonic unless the action explicitly archives/deletes them reversibly;
- journal sequence numbers and transaction IDs are unique, strictly ordered, and replay to the same projection;
- reconciliation is idempotent: a second reconcile emits no domain event and changes no projection bytes;
- success implies Git fsck passes, projections equal journal replay, and postcondition verification passed;
- failure implies either no externally visible change or a discoverable prepared transaction repaired deterministically on reopen;
- backup→mutate→restore and migrate→export→rollback preserve the declared observable state;
- `show(state)` equals the logical-parent→state delta; applying that delta to a compatible base affects no unrelated path/hunk.

PR seeds: 256 sequences × up to 80 actions. Nightly: 10,000 sequences × up to 500 actions. Every failure prints a replayable seed and minimized action list and stores it under the failure corpus after triage.

Exact commands:

```bash
cargo test --test properties -- --seeds 256 --max-actions 80
cargo test --test properties -- --seed <u64> --case <case.json>
cargo test --test properties -- --seeds 10000 --max-actions 500 --ignored
```

### TST-D007 — Exhaustive mutation fault injection

Production mutation code exposes named test-only failpoints at boundaries, never line-number hooks:

```text
FP-01 after-discover
FP-02 after-lock
FP-03 after-reconcile
FP-04 after-resolve
FP-05 after-plan
FP-06 after-durable-prepare-fsync
FP-07 after-first-git-ref-change
FP-08 after-git-jj-mutation
FP-09 after-worktree-index-mutation
FP-10 after-event-append-fsync
FP-11 after-projection-replace
FP-12 before-verify
FP-13 after-verify-before-commit-marker
FP-14 after-commit-marker-before-unlock
```

Each failpoint supports `error`, `SIGKILL`, short write, `ENOSPC`, `EACCES`, fsync failure, child Git/JJ nonzero exit, and corrupt/truncated last journal frame where meaningful. The child is killed from a supervisor process so destructors cannot fake crash safety. On restart, `jjk repair --check` followed by normal discovery must converge to exactly one of two declared outcomes: the pre-operation snapshot or the fully verified post-operation snapshot. Mixed outcomes, duplicated events, lost sibling refs, unexplained commits, or an unrecoverable lock are failures.

For every JJK-native mutator (`init`, capture, return, fork, atomic pick, promotion, delete/recover, undo/redo, backup/load, migration), run:

```bash
cargo test --test crash_matrix -- <operation> --all-failpoints --modes error,kill,short-write,enospc
cargo run --bin jjk-test -- replay-crash artifacts/crash/<case-id>.json
```

`repair --check` is read-only. A mutating repair requires `jjk repair --apply <transaction-id>` or documented automatic roll-forward only when the durable prepare record proves the complete intended plan.

### TST-D008 — Concurrent-writer proof

Concurrency tests use independent OS processes, never tasks sharing one runtime. A barrier releases them after discovery but before lock acquisition. The corpus covers same worktree, sibling linked worktrees, separate worktrees sharing the common Git dir, transparent Git concurrent with JJK, JJK plus JJ, stale-lock owner death, and 32 agent writers.

Pass conditions:

- the repository/common-store writer lock is exclusive and fair enough that every non-cancelled writer completes within the test deadline;
- each successful operation has one transaction ID and a contiguous event group;
- conflicting plans re-reconcile after lock acquisition and either apply to fresh truth or fail with a retryable conflict—never apply a stale plan;
- non-conflicting sibling-worktree captures preserve all siblings and both commits;
- no lost updates, duplicate state IDs, ref lock leaks, SQLite `BUSY` escape, corrupt WAL, or projection/journal mismatch;
- readers observe only committed projections and remain available while a writer holds durable prepare;
- a killed lock holder is recoverable and the next writer repairs before mutation.

Hard command and threshold:

```bash
cargo test --test concurrent_writers -- --writers 32 --rounds 100 --deadline-secs 120
```

This must pass 10 consecutive runs on the release host. Any intermittent failure is a product defect; retrying in CI is prohibited.

### TST-D009 — Transparent Git passthrough is differential

Transparent Git passthrough means JJK delegates to the configured Git binary without interpreting or rewriting Git arguments. The harness executes a case twice from cloned-identical sandboxes: once as `git <args>` and once through the transparent JJK route. It compares:

- exact `OsString` argument bytes and order received by a recording Git shim, including empty args, non-UTF-8 bytes on Unix, spaces, glob characters, leading dashes, `--`, and `-c key=value`;
- exact cwd inode/path semantics and inherited environment with no permitted delta; JJK's recursion guard is internal process state and MUST NOT be exported to the Git child;
- stdin bytes, stdout bytes, stderr bytes, TTY/PTY behavior, interactive prompts, pager/editor/credential-helper invocation, and file-descriptor closure;
- termination by exit code or signal, including `SIGINT`, `SIGTERM`, `SIGPIPE`, and child stop/continue behavior;
- resulting files, refs, index stages, reflog, and config.

```rust
struct PassthroughOracle {
    direct: CommandObservation,
    via_jjk: CommandObservation,
}
```

The required command corpus includes `status`, `diff`, `log`, `show`, `commit`, `commit --amend`, `switch`, `checkout`, `branch`, `merge`, `rebase`, `cherry-pick`, `reset`, `restore`, `stash`, `worktree`, `submodule`, `fetch`, `pull`, `push`, `config`, `credential`, aliases, external `git-*` commands, hooks, pager, editor, and an unknown subcommand. For pure transparent passthrough, output and termination must be byte-identical. Passthrough performs no reconciliation in that process; the next JJK-native/enhanced invocation separately proves idempotent reconciliation.

Exact gate:

```bash
cargo test --test git_passthrough -- --git "$GIT_UNDER_TEST" --cases all
```

Run against the oldest supported Git, newest stable Git, and platform Git. `VAL-PASS-001` fails on any unapproved delta.

### TST-D010 — Git-only/JJ parity is semantic, not storage-identical

Each adapter consumes one `OperationPlan` and returns typed effects. Differential tests run the same fixture/action sequence in Git-only and colocated-JJ sandboxes, then compare a canonical semantic snapshot:

```rust
struct SemanticSnapshot {
    states: Vec<CanonicalState>,
    attempts: Vec<CanonicalAttempt>,
    current_tree: TreeDigest,
    current_logical_state: StateId,
    sibling_trees: BTreeMap<AttemptKey, TreeDigest>,
    atomic_deltas: BTreeMap<StateId, PatchDigest>,
    provenance: Vec<CanonicalProvenance>,
}
```

Git OIDs, JJ change IDs, operation IDs, and adapter-only recovery facts are deliberately excluded but checked for internal validity. Capability differences must appear in `jjk capability --json`; unsupported JJ acceleration falls back to Git-only before durable prepare or fails without mutation. It must never silently change semantics.

### TST-D011 — Schema migration and rollback corpus

Every released schema version has an immutable fixture containing metadata, refs, journal/WAL where applicable, expected semantic snapshot, and the binary version that produced it. The first migration target is legacy `.jjk/repo.json` version 1, including real recovered samples with paths anonymized but structure unchanged.

For each `vN → vN+1`:

1. copy the fixture; capture Git refs, HEAD, index, worktree digest, and metadata checksum;
2. `jjk migrate --check --format json` must report a plan and make zero writes;
3. `jjk migrate --apply` must create a verified pre-migration backup and commit the new schema atomically;
4. open/query every state, replay journal, run `git fsck --full`, and compare the semantic snapshot;
5. rerun apply: it is an idempotent no-op;
6. run the documented rollback/export path and prove an older supported binary can read the result when backward export is promised;
7. inject a crash at every migration failpoint and prove restart preserves either valid old or valid new schema;
8. unknown future schema exits with `EX_DATAERR` (65), names the supported range, and writes nothing.

```bash
cargo test --test migrations -- --from all-released --to current
cargo test --test migrations -- legacy-repo-json-v1
cargo test --test migrations -- --crash-matrix
```

Fixture checksums are reviewed assets; migration tests never rewrite source fixtures in place.

### TST-D012 — Performance corpus and measurement discipline

Benchmarks run the release binary, not a test build, on a pinned dedicated host with turbo/power policy recorded. The harness records OS, filesystem, CPU, RAM, Rust version, Git/JJ versions, binary checksum, cold/warm classification, and corpus checksum. It uses wall time around the process boundary for user latency and an internal trace only to attribute stages. Warm cases run 10 warmups and 100 measured samples; cold cases evict only JJK-owned caches through an explicit test hook and run 30 isolated samples. Results report p50, p95, p99, median absolute deviation, peak RSS, bytes read/written, child process count, and metadata growth. A gate compares the candidate against an immutable last-release baseline from the same host.

#### Benchmark corpus

| ID | Shape |
|---|---|
| `PERF-SMALL` | 200 files, 100 commits, 8 states, 1 worktree |
| `PERF-ORDINARY` | 10k files, 10k commits, 1k states, 8 attempts, 4 worktrees |
| `PERF-MONOREPO` | 100k files, 50k commits, 10k states, sparse paths, 16 worktrees |
| `PERF-HISTORY` | 1k files, 1m commits generated by fast-import, 100k states |
| `PERF-WORKTREES` | 10k files, 1k commits, 1k states, 128 linked worktrees |
| `PERF-NFS` | `PERF-ORDINARY` through the supported latency/fault filesystem harness: 2 ms metadata latency, 8 ms fsync latency |
| `PERF-DIRTY` | ordinary repo with 10k modified paths, staged/unstaged/untracked mix |
| `PERF-BINARY` | 10 GiB tracked history with unchanged large blobs and 100 changed small files |

The corpus is generated from deterministic manifests; checked-in assets contain no giant repositories. `jjk-bench corpus verify` checks counts and digests before timing.

#### Release budgets

| Operation | Fixture | Hard threshold |
|---|---|---|
| `jjk current --json` warm | ordinary | p95 ≤ 50 ms, p99 ≤ 75 ms, RSS ≤ 35 MiB |
| `jjk status --json` warm, clean | ordinary | p95 ≤ 50 ms, p99 ≤ 75 ms, RSS ≤ 40 MiB |
| `current` / `status` warm | monorepo | p95 ≤ 75 ms; zero full worktree walk, proven by trace counters |
| return/fork plan to first feedback | ordinary | p95 ≤ 100 ms |
| capture, 1 changed 4 KiB file | ordinary | p95 ≤ 150 ms end-to-end |
| no-op capture | ordinary | p95 ≤ 75 ms and creates zero state/event |
| graph first complete terminal paint, 1k states | ordinary | p95 ≤ 100 ms |
| graph query, 100k states, first page 200 | history | p95 ≤ 150 ms, peak RSS ≤ 150 MiB |
| reconcile, no external changes | monorepo | p95 ≤ 75 ms and ≤ 16 bounded metadata/stat probes |
| reconcile one external commit | ordinary | p95 ≤ 150 ms |
| concurrent reader while writer prepared | ordinary | reader p95 ≤ 75 ms; no blocked read > 250 ms |
| metadata amplification | 100k captures | journal + projections ≤ 2.0 KiB per state median excluding user messages/evidence blobs; startup does not grow with WAL tail after checkpoint |
| NFS orientation | NFS | p95 ≤ 250 ms, no more than one fsync for a read command (target: zero) |

Additionally, no gated benchmark may regress p95 by >10% or peak RSS by >15% versus the last release even when still below the absolute budget; an intentional regression requires a recorded architecture decision and a new baseline. Results with coefficient of variation >5% after outlier-free rerun are invalid measurements, not passes or failures.

Exact commands:

```bash
cargo build --release --locked
./target/release/jjk-bench corpus build --manifest benches/corpus.toml --seed 0x4a4a4b01
./target/release/jjk-bench run --suite release --samples 100 --output target/bench/result.json
./target/release/jjk-bench compare --baseline benches/baselines/<host>/<version>.json --candidate target/bench/result.json --gate
```

### TST-D013 — End-to-end user stories

The E2E driver is a real process/PTY harness. It never calls library internals. Each story saves stdout/stderr, terminal transcript, event stream, ref snapshot, tree/index digest, and final `doctor` report.

Required stories:

1. **Solo:** init existing Git, capture baseline, risky step, inspect graph, return, continue from history, undo/redo, remove JJK metadata, continue with Git.
2. **Atomic composition:** canonical snake scenario and a conflict case with explicit plan/abort/resolve evidence.
3. **Pair:** two linked worktrees create sibling states concurrently, exchange one atomic delta, and preserve both originals.
4. **Agent fleet:** 32 writers in isolated worktrees leave typed handoffs; one promotion occurs only with validation evidence.
5. **Maintainer:** origin/upstream fork, upstream advances, submission projection refreshes, conflict abort leaves exploration unchanged.
6. **External Git user:** JJK initialized, then a user without JJK commits/branches/merges/pushes; later JJK reconciliation imports facts idempotently.
7. **Disaster:** kill during each transaction stage, reopen, diagnose, repair, and recover exact pre/post state.
8. **Upgrade:** install previous release, create representative data, upgrade current release, migrate, use, rollback/export where supported.
9. **JJ optionality:** same daily workflow with JJ absent, present, broken, and removed between invocations.
10. **Uninstall:** uninstall binary/integrations and optionally JJK repository metadata; all ordinary Git workflows remain valid and understandable.

### TST-D014 — Install, upgrade, and uninstall surfaces

Release candidates are installed into clean, disposable machines/containers/VMs from the exact published artifact, never the workspace binary.

| Surface | Required proof |
|---|---|
| macOS arm64/x86_64 | signed tarball and Homebrew bottle install/upgrade/uninstall |
| Linux x86_64/aarch64 | signed tarball; glibc compatibility floor; package channel when advertised |
| Windows x86_64 | signed archive/native binary in PowerShell; path and signal/exit behavior |
| WSL2 | Linux artifact plus Windows-mounted-path warning/capability behavior |
| source | `cargo install --path . --locked` from clean checkout |
| shells | bash, zsh, fish, PowerShell completion/init install and exact removal |

Every artifact test verifies signature/checksum, `jjk --version`, `jjk doctor --json`, basic solo story, no undeclared dynamic runtime dependency, upgrade from every supported release, and package-manager uninstall. Package channels remain unadvertised until their live install gate passes.

Uninstallability has two distinct operations:

- package uninstall removes executable, completions, manpages, and only its own shell-init block; it leaves repository data untouched;
- `jjk uninstall --repo --dry-run` lists JJK-only metadata/refs/hooks/config edits and proves no Git object becomes unreachable solely because cleanup removes a JJK ref; `--apply` removes only listed JJK-owned artifacts after an explicit reachability/export policy.

After repository uninstall, these exact checks must pass:

```bash
git fsck --full
git status --porcelain=v2
git log --all --decorate --oneline -n 50
git worktree list --porcelain
git branch --format='%(refname)'
git commit --allow-empty -m 'post-jjk git-only proof'
git switch -c post-jjk-branch
git merge --no-edit <fixture-branch>
```

The expected project files, canonical refs, index, user hooks/config, remotes, submodules, and worktrees must equal the pre-uninstall ownership manifest. No command may depend on the JJK binary afterward.

### TST-D015 — SQLite/WAL is accepted only behind observable invariants

SQLite WAL plus materialized projections is the default because it offers checksummed transactions, concurrent readers, schema migration primitives, and crash-tested durability. Tests must not couple product semantics to SQLite row layout. A storage backend passes the same `JournalStore` contract:

- append one transaction atomically;
- replay by monotonic sequence;
- provide snapshot-consistent readers;
- detect corruption/truncation;
- checkpoint without losing accepted events;
- migrate and back up online within declared lock bounds.

SQLite-specific tests use `PRAGMA integrity_check`, WAL frame truncation/corruption, checkpoint interruption, busy readers/writers, and power-loss simulation. `synchronous=FULL` is required for release durability tests. A faster setting may be benchmarked but cannot become default unless the crash matrix proves the same contract on supported filesystems. WAL on a network filesystem is not assumed safe: `RepoShape::NetworkFilesystem` must either select a proven safe journal mode or reject mutation with an actionable capability error. This evidence-based constraint is preferable to claiming universal SQLite WAL support.

## 3. Data and API shapes

The test architecture depends on narrow public testing seams rather than privileged access to product internals:

```rust
trait RepoFixtureBuilder {
    fn build(&self, spec: &FixtureSpec, sandbox: &Sandbox) -> BuiltFixture;
}

trait ObservableOracle {
    fn filesystem(&self) -> FilesystemManifest;
    fn git(&self) -> GitManifest;
    fn jj(&self) -> Option<JjManifest>;
    fn journal(&self) -> JournalManifest;
    fn projection(&self) -> ProjectionManifest;
}

struct MutationManifest {
    transaction_id: TransactionId,
    planned_refs: BTreeSet<RefName>,
    planned_paths: BTreeSet<RepoPath>,
    before: ObservableDigest,
    after: Option<ObservableDigest>,
}

struct BenchmarkResult {
    case: BenchmarkId,
    corpus_sha256: Digest,
    binary_sha256: Digest,
    host: HostFingerprint,
    samples_ns: Vec<u64>,
    peak_rss_bytes: u64,
    io: IoCounters,
    trace: StageCounters,
}
```

The compiled CLI exposes only three test accommodations: a deterministic clock endpoint, named failpoint activation, and bounded stage counters. Release builds reject test controls unless built with the dedicated test feature; semantic behavior and storage code are otherwise identical. Fixture builders and oracles invoke public CLI commands and independent Git/JJ plumbing. They may inspect the journal database for integrity and event truth, but may not patch it to arrange a passing state.

## 4. Invariants

- **INV-TST-001:** no test may use a live user repository, global config, global credential store, or shared mutable fixture.
- **INV-TST-002:** every mutation success is independently valid in filesystem, Git/JJ, journal, and projection views.
- **INV-TST-003:** every failure is classified as clean abort, recoverable prepared transaction, or verified committed transaction; no fourth partial state exists.
- **INV-TST-004:** atomic pick is parent→source delta, never root→source history.
- **INV-TST-005:** an operation may mutate only refs/files declared in its durable plan; sibling preservation is checked by full before/after inventories.
- **INV-TST-006:** transparent Git passthrough preserves argv bytes, cwd, stdio, environment, signals, and exit semantics.
- **INV-TST-007:** reconciliation and migration are idempotent.
- **INV-TST-008:** concurrent writers serialize/refresh at the repository integration boundary; readers see committed snapshots.
- **INV-TST-009:** all output goldens disclose filtering, degradation, missing capabilities, and incomplete history.
- **INV-TST-010:** performance claims are tied to corpus checksum, host identity, binary checksum, sample distribution, and cold/warm mode.
- **INV-TST-011:** install/uninstall tests use published artifacts and leave Git operable without JJK.
- **INV-TST-012:** a flaky hard gate is a failed gate, not eligible for retry-to-green.

## 5. Observable validation contracts

Each validator writes `target/evidence/<VAL-ID>/result.json`:

```rust
struct ValidationResult {
    id: ValidationId,
    verdict: Verdict,                 // Pass, Fail, Blocked
    command: Vec<OsString>,
    fixture_ids: Vec<FixtureId>,
    binary_sha256: Digest,
    observations: Vec<EvidenceRef>,
    started_at: Timestamp,
    duration: Duration,
}
```

### VAL-CORE-001 — Atomic pick hard gate

**Surface:** CLI + filesystem + Git + event API.  
**Needs:** `HF-SNAKE-001`, clean Git-only fixture.  
**Command:** `cargo test --test conformance -- VAL_CORE_001_atomic_pick --exact`.  
**Behavior:** on orange, `jjk pick fast_purple` yields `color=orange, fast=true`; source purple and both siblings remain byte/OID-identical; resulting kind is `cherry`; base/source/source-parent/patch identity are present; current points to the cherry.  
**Evidence:** file digest, `git diff-tree` for source parent/source/result, stable patch IDs, all ref OIDs before/after, canonical event JSON.  
**Fail:** any unrelated hunk/path, missing provenance, changed sibling/source, or non-current result.

Mandatory delta matrix: add/delete/rename, executable-bit transition, symlink target/type transition, binary blob, gitlink entry, empty file, mode-only change, and overlapping three-way conflict. For every clean case, compare exact `(path bytes, mode, object OID)` tree-entry transformations from source-parent→source against target-base→result. Conflict cases must prove durable plan, `continue` determinism, `abort` restoration, and crash recovery before/after conflict materialization.

### VAL-CORE-002 — Sibling preservation hard gate

**Surface:** CLI + Git graph.  
**Needs:** `HF-BRANCH-002`, clean/dirty historical-return variants.  
**Command:** `cargo test --test conformance -- VAL_CORE_002_sibling_preservation --exact`.  
**Behavior:** return to historical green and create orange; previous purple future remains reachable and unchanged; orange's logical and Git parent is green; navigation without mutation creates no ref/state/event.  
**Evidence:** complete refs and state graph before/after, `git merge-base`, tree digests, journal delta.  
**Fail:** overwritten/moved sibling, ancestry from prior tip, duplicate navigation state, or unexplained branch.

### VAL-COMPAT-001 — Transparent Git passthrough hard gate

**Surface:** process/CLI parity.  
**Needs:** supported-Git matrix and recording shim.  
**Command:** `cargo test --test git_passthrough -- --git "$GIT_UNDER_TEST" --cases all`.  
**Behavior:** direct Git and transparent JJK route are identical for the D009 corpus.  
**Evidence:** paired `CommandObservation`, repository digests, helper/hook recordings.  
**Fail:** any argv/cwd/stdio/environment/signal/exit/result delta. The harness seeds a value under JJK's internal recursion-guard name and requires byte/native-string-for-native-string preservation.

### VAL-RECOVERY-001 — Crash recovery hard gate

**Surface:** killed process + reopened CLI.  
**Needs:** all mutators and `FP-01..FP-14`.  
**Command:** `cargo test --test crash_matrix -- all-mutators --all-failpoints --modes error,kill,short-write,enospc`.  
**Behavior:** reopen converges to exact pre or verified post state; repair is deterministic and idempotent.  
**Evidence:** supervisor kill record, pre/post manifests, fsck/integrity output, event/projection comparison.  
**Fail:** mixed state, lost work/ref, duplicate event, corrupt store, or unrecoverable lock.

### VAL-CONCURRENCY-001 — Concurrent writers hard gate

**Surface:** 32 CLI processes in linked worktrees.  
**Needs:** shared common Git dir and deterministic barrier.  
**Command:** `for i in $(seq 1 10); do cargo test --test concurrent_writers -- --writers 32 --rounds 100 --deadline-secs 120 || exit 1; done`.  
**Behavior:** all non-conflicting writes survive; conflicts refresh/fail cleanly; readers see committed snapshots.  
**Evidence:** per-process transaction log, lock trace, refs/states inventory, SQLite integrity and journal replay.  
**Fail:** one failed run, lost update, stale-plan apply, busy leak, deadline, or corrupt projection.

### VAL-MIGRATION-001 — Migration/rollback hard gate

**Surface:** data migration + old/new binary compatibility.  
**Needs:** every released schema fixture, including legacy `repo.json` v1.  
**Command:** `cargo test --test migrations -- --from all-released --to current --crash-matrix`.  
**Behavior:** check is read-only; apply is backed up, atomic, idempotent, semantically equal; supported rollback/export is old-reader compatible; future schema is refused without writes.  
**Evidence:** fixture checksums, semantic snapshots, backup, old/new query output, crash matrix.  
**Fail:** data/provenance loss, in-place fixture mutation, partial schema, non-idempotence, or silent future-version open.

### VAL-UNINSTALL-001 — Uninstallability hard gate

**Surface:** clean machine + Git-only repository after removal.  
**Needs:** each advertised installer and ownership manifest.  
**Command:** `cargo test --test distribution -- VAL_UNINSTALL_001 --exact --artifact "$RELEASE_ARTIFACT"`.  
**Behavior:** package uninstall removes only installation-owned files; repository cleanup removes only approved JJK-owned artifacts; Git-only commit/branch/merge/status/fsck work after binary removal.  
**Evidence:** filesystem ownership diff, refs/config/hooks/worktrees diff, exact Git command transcript.  
**Fail:** orphaned required work, altered user config/hook, invalid Git, remaining required dependency on JJK, or undeclared residue.

### VAL-PERF-001 — Release performance hard gate

**Surface:** release binary on benchmark host.  
**Needs:** verified D012 corpus and same-host last-release baseline.  
**Command:** `./target/release/jjk-bench run --suite release --samples 100 --output target/bench/result.json && ./target/release/jjk-bench compare --baseline "$BASELINE" --candidate target/bench/result.json --gate`.  
**Behavior:** every absolute and relative D012 budget holds with valid variance.  
**Evidence:** raw samples, host/corpus/binary manifests, summary and comparison JSON.  
**Fail:** threshold breach, >10% p95 regression, >15% RSS regression, invalid corpus, or CV >5% after controlled rerun.

### VAL-INSTALL-001 — Published install surface hard gate

**Surface:** macOS, Linux, Windows, WSL, source, advertised package managers.  
**Needs:** signed published candidate artifacts.  
**Command:** `cargo test --test distribution -- VAL_INSTALL_001 --exact --matrix release-matrix.toml`.  
**Behavior:** signature, install, version, doctor, solo story, upgrade, completions, and uninstall pass per advertised surface.  
**Evidence:** VM/container identity, package transcript, signature output, binary checksum, E2E evidence.  
**Fail:** workspace-binary substitution, undeclared dependency, advertised but unavailable package, or incomplete removal.

### VAL-E2E-001 — Golden user stories hard gate

**Surface:** real CLI/PTY.  
**Needs:** D013 story fixtures.  
**Command:** `cargo test --test e2e -- --stories solo,atomic,pair,fleet,maintainer,external-git,disaster,upgrade,jj-optional,uninstall`.  
**Behavior:** each story meets its filesystem/Git/JJ/JJK oracle and output golden.  
**Evidence:** PTY transcript, command observations, graph/ref/tree/event manifests, doctor report.  
**Fail:** skipped story, mocked CLI, internal API shortcut, or mismatch on any view.

## 6. CI and release topology

```text
PR fast gate
  ├─ unit + schema + golden
  ├─ properties (256 × 80)
  ├─ conformance + historical corpus
  ├─ passthrough on primary Git
  ├─ short crash matrix
  └─ performance smoke (informational unless gross >25% regression)

Nightly matrix
  ├─ all OS/filesystems/Git/JJ versions/repo shapes
  ├─ properties (10k × 500)
  ├─ exhaustive crash and concurrency stress
  ├─ Git-only/JJ parity
  └─ sanitizer/model checker jobs where supported

Release candidate gate
  ├─ every VAL hard gate
  ├─ dedicated-host performance
  ├─ signed published-artifact install matrix
  ├─ upgrade/migration from every supported release
  └─ uninstall and Git-only continuation
```

Jobs upload evidence on failure and retain minimized reproducers. Success summaries retain manifests and benchmark distributions. Matrix shards are assigned by fixture ID hash, not execution timing, so reruns reproduce membership. Timeouts terminate the process tree and preserve evidence; they never convert to skips.

## 7. Failure modes and harness countermeasures

| Failure mode | Countermeasure |
|---|---|
| A test accidentally uses global Git config or credentials | hermetic env plus sentinel global helpers that fail if invoked |
| Parallel tests collide on branch names, ports, HOME, or SQLite | unique sandbox roots; no shared daemon/database; OS-assigned ports |
| Sleep-based race passes locally | readiness/barrier protocol; scheduler perturbation; 10/10 release stress gate |
| Snapshot normalization hides a regression | normalization allowlist and mutation countertests |
| Mock Git disagrees with real Git | mocks only for pure error classification; conformance uses supported real binaries |
| Crash test runs destructors | external supervisor sends SIGKILL |
| Fault injection creates impossible states | named production boundaries plus OS-level I/O fault modes; both are reported separately |
| Benchmarks warm unintended caches or compare hosts | explicit warm/cold modes and same-host signed baseline |
| Benchmark corpus drifts | deterministic manifest and checksum gate |
| Performance test measures test build | binary checksum and release-profile assertion |
| CI retry conceals flake | no automatic retry for correctness; any failed attempt is terminal evidence |
| Platform capability absent | required matrix job is blocked/failed, not skipped; capability manifest attached |
| Migration fixture gets upgraded in place | read-only source fixture copied to sandbox and checksum checked afterward |
| Direct library call makes E2E pass | E2E accepts only artifact path and process/PTY observations |
| WAL behaves unsafely on network FS | capability gate selects proven mode or refuses mutation before prepare |
| Passthrough wrapper absorbs signals or reformats output | direct-vs-wrapper PTY differential with signal cases |
| Cleanup destroys the repro | failed sandboxes retained and emitted as a replay command; successful sandboxes deleted |

## 8. Acceptance checks for the test system itself

The harness is ready to validate v0.1 only when:

1. deliberately mutating atomic-pick logic to use root→source makes `VAL-CORE-001` fail;
2. deleting a sibling ref makes `VAL-CORE-002` fail;
3. swapping two passthrough args, absorbing SIGINT, or writing one stderr byte makes `VAL-COMPAT-001` fail;
4. disabling fsync or projection recovery causes at least one crash failpoint case to fail;
5. removing post-lock reconciliation causes the concurrent-writer stale-plan case to fail;
6. a lossy migration or unknown future schema write makes `VAL-MIGRATION-001` fail;
7. leaving a shell-init line or JJK-only ref after cleanup makes `VAL-UNINSTALL-001` fail;
8. injecting a 20% latency regression makes `VAL-PERF-001` fail;
9. running the concurrent gate ten times on the unmodified candidate passes 10/10 without retries;
10. every failure prints one exact local replay command and retains attributable evidence.

## 9. Explicit non-goals

- Tests do not treat the historical TypeScript implementation, its output, or its storage layout as normative when it conflicts with the product constitution.
- Unit tests do not substitute for real Git/JJ/process/repository conformance.
- Byte-identical Git-only/JJ storage is not required; semantic parity and adapter-specific integrity are.
- Network filesystem mutation is not promised merely because SQLite can open the path; it requires explicit durability proof.
- Performance gates do not promise identical latency on arbitrary hardware; they bind the named corpus and reference host while absolute UX budgets remain product targets.
- The harness does not test against live GitHub accounts, user credentials, or mutable public repositories; forge E2E uses disposable local servers or dedicated ephemeral test organizations with recorded fixtures.
- Golden updates are not an approval mechanism and CI does not rewrite expected output.
- Automatic retries, widened timeouts, ignored failures, and platform skips are not accepted remedies for nondeterminism.
- Experimental/research capabilities do not block v0.1 unless advertised as stable, but their capability labels and safe refusal paths are tested.

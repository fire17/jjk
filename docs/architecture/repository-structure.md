# Repository Structure

**Status:** normative v0.1 architecture  
**Decision IDs:** `REP-*`

## Context

JJK is a semantic state layer above Git and, optionally, Jujutsu. Git remains the universal object, history, transport, and collaboration substrate; JJ is an optional local-history accelerator; JJK owns meaning, provenance, topology, recovery, and human/agent workflows.

The legacy TypeScript implementation proved the product but concentrated unrelated concerns in `commands.ts`, `store.ts`, `git.ts`, and `render.ts`. The Rust rewrite must make cross-layer safety hard to bypass, keep frequent commands at hand speed, and remain understandable after six months. It must not manufacture a micro-workspace before components have independent consumers or release cycles.

## Decisions

### REP-001 — One Cargo package, one library, one binary

V0.1 is one `jjk` Cargo package with `src/lib.rs` and a minimal `src/main.rs`. Internal boundaries are Rust modules, visibility, sealed traits, typed capabilities, and architecture checks—not crates.

A workspace of internal crates is rejected: it would add feature-unification, API, navigation, release, and incremental-build overhead without an independent consumer. A component may become a crate only when it has a real external consumer or security/process boundary, a stable small API, a distinct release need, measured build benefit, and no need to expose transaction internals. Plausible later candidates are a wire-only protocol crate and an out-of-process extension SDK; neither is created in v0.1.

### REP-002 — Dependencies point inward

```text
main
  ├─> cli ─────────────> render
  └─> bootstrap             ^
          │                 │ typed outcomes/read models
          v                 │
       app commands + transaction coordinator
          │                 ^
          v                 │
       domain <────────── ports (traits)
          ^                 ^
          └──── adapters ───┘
                │
                v
        Git/JJ/SQLite/OS/network
```

- `domain`: identities, invariants, events, graph, policies, and pure plans. No I/O.
- `ports`: minimum effect interfaces required by use cases; no implementations.
- `app`: reconciliation, resolution, use cases, transaction orchestration, and repair.
- `adapters`: Git, optional JJ, SQLite, filesystem, locks, processes, clock, and IDs.
- `cli`: grammar, routing, I/O mode, and exit contract; never repository mutation.
- `render`: pure presentation of typed results; never queries or mutation.
- `main`: process setup and final exit only.

Cycles and “shared/common/utils” dumping grounds are forbidden. A type lives in the layer that owns its invariant.

### REP-003 — One mutation gateway

`app::transaction::Coordinator` is the sole gateway for JJK-native and Git-enhanced mutations. Every mutation follows:

```text
discover → lock → reconcile → resolve → plan → durable prepare
→ mutate Git/JJ/files → append events+projections → verify → commit/repair
```

Command handlers supply typed intent and policy; they cannot manually sequence stores and backends. Mutation-capable adapter calls require an unforgeable `PreparedOperation<'lock>` issued only after the lock and durable prepare. Projection writes require the journal transaction context, so projections cannot advance independently of events.

Transparent Git passthrough is deliberately different: it invokes Git without creating JJK semantic state, locks, observation, or reconciliation in that process. The next JJK-native/enhanced invocation performs bounded reconciliation when repository facts require it.

### REP-004 — SQLite WAL behind storage ports

The default control store is one per-safe-space SQLite database in WAL mode containing the append-only event journal, durable operation records, schema metadata, and materialized projections. It earns its place through atomic multi-record operations, crash recovery, concurrent readers, migrations, and indexed bounded queries.

SQLite is private to `adapters::sqlite`. `Journal`, `ProjectionStore`, and `OperationStore` ports expose domain records, never SQL rows or `rusqlite` types.

Before freezing the format, benchmark SQLite WAL against a framed append-only file with rebuilt projections on small-event, 1k-state, concurrent-reader, and injected-crash fixtures. Replace SQLite only if the alternative also proves atomic prepare/commit, migration, repair, and bounded queries with materially lower measured cost. A hand-rolled log is not accepted on aesthetic grounds.

### REP-005 — Narrow public API, private-by-default internals

`lib.rs` deliberately re-exports only embedding-safe, versioned contracts:

```rust
pub struct Engine { /* private fields */ }
pub struct EngineBuilder { /* private fields */ }
pub struct Invocation { /* typed request */ }
pub struct Outcome { /* result, diagnostics, suggested actions */ }
pub struct CancellationToken { /* private representation */ }

impl EngineBuilder {
    pub fn build(self) -> Result<Engine, BuildError>;
}
impl Engine {
    pub fn invoke(
        &self,
        request: Invocation,
        cancel: &CancellationToken,
    ) -> Result<Outcome, JjkError>;
}
```

Opaque IDs, input/output wire types, capability reports, and structured errors may be public. Adapter implementations, SQL rows, locks, operation phases, migration functions, projection writers, and Git builders are private or `pub(crate)`. Port traits are `pub(crate)` in v0.1, not an accidental SDK. `clap::ArgMatches`, terminal styles, and database types never cross the facade. Every new `pub` item requires a compatibility statement and external-consumer contract test.

### REP-006 — Generated artifacts are committed derivatives

Canonical Rust boundary types generate JSON Schemas for events, freeze/backup manifests, handoffs, validation evidence, and later daemon messages. The single CLI definition generates completions and man pages. Explicit tools write committed files under `generated/`; normal builds and runtime never run generators. CI regenerates into a temporary directory and fails on drift.

Generated files identify source type, schema version, and generator version and say “do not edit.” SQLite migrations remain hand-authored SQL because operational order, repair, and rollback cannot safely be inferred from structs.

### REP-007 — Extensions are capabilities, not native plugins

V0.1 extension seams are typed ports with built-in adapters: Git, optional JJ, filesystem/worktrees, lock, process, clock, IDs, output, and later forge/shell/editor/agent integrations. Rust dynamic plugins are rejected because Rust has no stable ABI and repository-trusted in-process code has excessive blast radius. Future third-party extensions use a versioned stdio/local-socket protocol with scoped capabilities; they never receive database or coordinator access.

## Complete target tree

Directories appear only when their first real implementation lands. Later-only seams named below are not created as empty packages.

```text
JJK/
├── Cargo.toml                         # one package; lean defaults
├── Cargo.lock                         # committed application lockfile
├── build.rs                           # build metadata only; no generators
├── rust-toolchain.toml                # pinned stable toolchain
├── deny.toml                          # dependency/license/advisory policy
├── LICENSE
├── README.md                          # verified user entry point
├── CHANGELOG.md                       # behavior and schema compatibility
├── VISION.md                          # sacred intent, not implementation-owned
├── origins.md                         # sacred provenance
│
├── src/
│   ├── main.rs                        # tiny process boundary and exit mapping
│   ├── lib.rs                         # curated public facade
│   ├── bootstrap.rs                   # lazy capability/adapter assembly
│   ├── error.rs                       # structured errors and exit classes
│   ├── domain/
│   │   ├── mod.rs
│   │   ├── id.rs                      # distinct opaque stable IDs
│   │   ├── state.rs                   # State and StateKind invariants
│   │   ├── attempt.rs                 # exploration/canonical relationships
│   │   ├── graph.rs                   # typed edges and pure graph queries
│   │   ├── event.rs                   # versioned event envelope/payloads
│   │   ├── operation.rs               # phases, recovery, typed mutation plan
│   │   ├── provenance.rs              # actor, source, causal/patch identity
│   │   ├── evidence.rs                # validation and verification status
│   │   ├── workspace.rs               # branch/worktree/index/dirty facts
│   │   ├── navigation.rs              # candidates, confidence, visit history
│   │   ├── capability.rs              # Git/JJ/etc. facts and degradation
│   │   └── policy.rs                  # safety, promotion, retention policies
│   ├── ports/
│   │   ├── mod.rs
│   │   ├── repository.rs              # discovery and observed repository truth
│   │   ├── git.rs                     # Git object/ref/index/worktree effects
│   │   ├── jj.rs                      # optional JJ effects/capabilities
│   │   ├── journal.rs                 # typed event append/read
│   │   ├── projection.rs              # materialized read/write contract
│   │   ├── operation.rs               # durable prepare/phase/commit record
│   │   ├── lock.rs                    # safe-space lock contract
│   │   ├── filesystem.rs              # atomic files/worktrees/snapshots
│   │   ├── process.rs                 # argv/cwd/env/stdio/signal/exit runner
│   │   ├── clock.rs                   # UTC plus monotonic deadlines
│   │   └── ids.rs                     # injectable ID source
│   ├── app/
│   │   ├── mod.rs                     # Engine and typed dispatch
│   │   ├── context.rs                 # request-scoped discovered context
│   │   ├── query.rs                   # read-only fast query service
│   │   ├── resolve.rs                 # exact/fuzzy target resolution
│   │   ├── reconcile.rs               # external facts into import events
│   │   ├── plan.rs                    # pure previews and effect plans
│   │   ├── transaction.rs             # sole mutation coordinator
│   │   ├── repair.rs                  # interrupted-operation recovery
│   │   └── command/
│   │       ├── mod.rs                 # typed command enum/dispatch
│   │       ├── init.rs                # initialize and import Git history
│   │       ├── capture.rs             # save/step/nice
│   │       ├── inspect.rs             # current/status/show/diff
│   │       ├── see.rs                 # graph/story read models
│   │       ├── navigate.rs            # return/back/forward/up/down
│   │       ├── fork.rs                # attempt/branch/worktree
│   │       ├── pick.rs                # exact logical-parent delta
│   │       ├── annotate.rs            # star/tags/message
│   │       ├── archive.rs             # reversible hide/recover
│   │       ├── undo.rs                # whole-control-plane undo/redo
│   │       ├── backup.rs              # backup/load/freeze
│   │       └── doctor.rs              # integrity/capability/repair report
│   ├── adapters/
│   │   ├── mod.rs                     # concrete constructors, no registry
│   │   ├── git/
│   │   │   ├── mod.rs
│   │   │   ├── command.rs             # byte/native-string safe Git argv
│   │   │   ├── discover.rs            # common-dir/worktree/submodule discovery
│   │   │   ├── observe.rs             # HEAD/index/status/refs/objects
│   │   │   ├── mutate.rs              # explicit planned Git effects
│   │   │   ├── patch.rs               # atomic parent-to-state delta
│   │   │   └── passthrough.rs         # transparent exec/supervision
│   │   ├── jj/
│   │   │   ├── mod.rs                 # capability probe and adapter
│   │   │   ├── observe.rs             # change/commit/op-log facts
│   │   │   └── mutate.rs              # planned optional JJ effects
│   │   ├── sqlite/
│   │   │   ├── mod.rs                 # connection/WAL/transaction setup
│   │   │   ├── journal.rs             # event codec and journal
│   │   │   ├── operation.rs           # prepare/phase/repair records
│   │   │   ├── projection.rs          # projection writers/queries
│   │   │   ├── row.rs                 # private SQL codecs
│   │   │   └── migrate.rs             # ordered migration runner
│   │   ├── os/
│   │   │   ├── mod.rs
│   │   │   ├── filesystem.rs          # atomic rename/fsync/path behavior
│   │   │   ├── lock.rs                # advisory lock and owner diagnostics
│   │   │   ├── process.rs             # child/exec, signals, cancellation
│   │   │   ├── clock.rs
│   │   │   └── ids.rs
│   │   └── legacy/
│   │       └── repo_json.rs            # removable one-way v1 JSON import
│   ├── cli/
│   │   ├── mod.rs                     # parse into Invocation
│   │   ├── definition.rs              # one clap definition for CLI/assets
│   │   ├── route.rs                   # native/enhanced/passthrough classifier
│   │   ├── input.rs                   # OsString and stdin/TTY policy
│   │   ├── output.rs                  # human/json/quiet contract
│   │   └── exit.rs                    # result/error to stable exit code
│   └── render/
│       ├── mod.rs
│       ├── human.rs                   # concise results/actionable errors
│       ├── json.rs                    # versioned machine envelope
│       ├── graph.rs                   # deterministic width-aware layout
│       ├── table.rs                   # ambiguity/inspection tables
│       └── style.rs                   # color and TTY policy only
│
├── migrations/
│   ├── 0001_initial.sql               # journal, operations, projections
│   ├── 0002_*.sql                     # only with real transitions
│   └── fixtures/                      # old DB and legacy JSON inputs
├── schemas/
│   └── compatibility.toml              # reader/writer version policy
├── generated/
│   ├── schemas/                        # events/manifests/handoff/evidence JSON Schema
│   ├── completions/                    # bash/zsh/fish/PowerShell
│   └── man/jjk.1
├── tools/
│   ├── generate-schemas.rs             # explicit, non-runtime generator
│   ├── generate-cli-assets.rs          # completions/man generator
│   └── check-architecture.rs           # forbidden-edge/public-API checks
├── tests/
│   ├── contract/                       # public API, schema, JSON CLI, passthrough
│   ├── conformance/                    # init/capture/return/fork/pick/external/concurrency/crash
│   ├── migration/                      # legacy JSON and SQLite versions
│   ├── cli/                            # basic loop, ambiguity, graph goldens
│   └── support/                        # real-repo builders, faults, narrow fakes, assertions
├── fixtures/
│   ├── repos/                          # Git bundles: bare/submodule/merge/many-branch/etc.
│   ├── scenarios/                      # declarative historical regressions
│   ├── graphs/                         # deterministic expected read models
│   └── corrupt/                        # WAL/event/operation repair corpus
├── benches/
│   ├── startup.rs                      # help/current/status/passthrough
│   ├── graph.rs                        # 1k+ states
│   ├── capture.rs                      # small/large/dirty repos
│   └── reconcile.rs                    # gates and many worktrees
├── docs/
│   ├── architecture/                   # decisions and cross-layer contracts
│   ├── reference/                      # CLI/machine contracts
│   ├── guides/                         # workflows, recovery, agents
│   └── development/                    # build/fixture/release compatibility
├── packaging/
│   ├── install.sh
│   ├── install.ps1
│   ├── homebrew/jjk.rb
│   └── scoop/jjk.json
└── .github/
    ├── workflows/{ci,conformance,release}.yml
    └── ISSUE_TEMPLATE/bug.yml
```

Later-only directories, absent until implemented: `src/adapters/forge/`, `src/protocol/`, `tests/protocol/`, editor integrations, and any SDK crate.

## Ownership boundaries

| Area | Owns | Must not own |
|---|---|---|
| `domain` | identities, invariants, typed events/edges/plans | I/O, SQL, CLI, terminal, subprocesses |
| `ports` | minimum required effects | adapter selection, policy, retries |
| `app` | use cases, reconciliation, transaction and repair | SQL, terminal styling, raw commands |
| Git/JJ adapters | faithful substrate observation/effects | semantic policy or rendering |
| SQLite adapter | persistence mechanics and migrations | product ontology or command behavior |
| `cli` | grammar, routing, stdio/exit promises | mutation and storage |
| `render` | deterministic presentation | queries, effects, graph truth |
| `tests/support` | fixture construction and fault injection | production helpers |
| `generated` | reproducible derivatives | hand-authored behavior |
| `packaging` | channel install metadata | copied implementations |

## Compile-time boundaries

Allowed edges:

```text
main -> {bootstrap, cli, error}
bootstrap -> {app, ports, adapters}
cli -> {app facade, render, public domain contracts}
render -> {read/output contracts, error}
app -> {domain, ports, error}
ports -> {domain, error}
adapters -> {ports, domain, error}
domain -> {std, approved value/serialization crates}
```

Enforcement:

1. `domain` cannot import `clap`, SQLite, terminal, HTTP, filesystem/process APIs, or adapter modules.
2. `app` cannot import concrete adapters or SQL crates.
3. `render` cannot import ports/adapters.
4. Direct process spawning occurs only in `adapters/os/process.rs` and transparent passthrough.
5. Direct SQLite usage occurs only in `adapters/sqlite/**`.
6. Adapter mutation APIs require `PreparedOperation<'_>`; read APIs do not.
7. Projection writes require the same storage transaction as journal append.
8. `tools/check-architecture.rs` checks forbidden imports/edges; a purpose-built check is cheaper than fake crate boundaries.
9. A pinned public-API snapshot rejects accidental exports.
10. Compile-fail tests prove external code cannot construct IDs, mutation capabilities, operation phases, or adapters illegally.

## Startup and passthrough contract

Routing happens before expensive bootstrap:

1. Inspect native `OsString` arguments without opening SQLite or probing JJ.
2. Classify the command as **JJK-native**, **Git-enhanced**, or **transparent Git passthrough**.
3. Transparent passthrough preserves argv bytes/native strings, cwd, environment, stdin/stdout/stderr, terminal behavior, signals, and exit code. Never lossily convert arguments or capture/reformat normal Git output.
4. Use process replacement where post-reconciliation is unnecessary; otherwise supervise the child with inherited stdio and signal forwarding, returning the exact exit status.
5. Read-only JJK commands lazily open only projections and required Git observations.
6. Mutation commands instantiate only adapters named by the plan.

No async runtime, network/forge client, JJ probe, schema generator, or rich renderer initializes on help/version/transparent paths. Startup benchmarks enforce `<50 ms` warm orientation and a measured passthrough overhead budget.

## Data/API shapes

Distinct validated newtypes represent `StateId`, `AttemptId`, `OperationId`, `EventId`, and `RepoFingerprint`; Git OIDs, JJ IDs, and mutable labels are separate types and never silently interchangeable.

```rust
pub struct EventEnvelopeV1 {
    pub schema_version: u16,
    pub event_id: EventId,
    pub operation_id: OperationId,
    pub causal_parent: Option<EventId>,
    pub repository: RepoFingerprint,
    pub occurred_at: UtcTimestamp,
    pub actor: Actor,
    pub payload: EventV1,
}

pub enum EventV1 {
    SafeSpaceInitialized(SafeSpaceInitializedV1),
    GitCommitObserved(GitCommitObservedV1),
    StateCaptured(StateCapturedV1),
    StateAnnotated(StateAnnotatedV1),
    AttemptForked(AttemptForkedV1),
    StateActivated(StateActivatedV1),
    DeltaApplied(DeltaAppliedV1),
    ValidationRecorded(ValidationRecordedV1),
    CanonicalPromoted(CanonicalPromotedV1),
    StateArchived(StateArchivedV1),
    StateRecovered(StateRecoveredV1),
    BackupCreated(BackupCreatedV1),
    RestoreApplied(RestoreAppliedV1),
}
```

Persisted versions are immutable. A v2 is a new payload/envelope type with an explicit upgrader, not conditional interpretation of v1. Projections are disposable and rebuildable from the journal plus verified substrate facts. Operation records store phase, intended effects, observed effects, recovery boundary, and verification result.

Machine CLI output uses a versioned envelope independent of terminal rendering:

```rust
pub struct MachineOutcomeV1<T> {
    pub schema_version: u16,
    pub operation_id: Option<OperationId>,
    pub result: T,
    pub diagnostics: Vec<DiagnosticV1>,
    pub suggested_actions: Vec<ActionV1>,
}
```

## Feature flags

Keep defaults small and semantics stable. Features remove optional integrations, never core safety.

| Feature | Default | Enables | Rule |
|---|---:|---|---|
| `git-cli` | yes | required Git CLI adapter | v0.1 always enabled in distributed binary |
| `sqlite` | yes | SQLite WAL store | canonical v0.1 store |
| `jj` | no | JJ capability probe/adapter | Git-only remains behaviorally complete |
| `tui` | no | interactive terminal surface | same app/query APIs; no duplicate logic |
| `forge` | no | later GitHub/GitLab discovery | network stack absent otherwise |
| `daemon` | no | later local protocol server | protocol types versioned separately |
| `dev-faults` | no | deterministic phase failure hooks | forbidden in release profile |

Do not feature-gate individual commands or domain types: that creates an untestable product matrix. JSON output, migrations, repair, graph queries, and transparent Git are core. Packaging builds named profiles (`minimal`, `standard`, later `integrations`) from an explicit matrix and runs conformance for each shipped profile.

## Fixtures and test layout

Fixtures are immutable inputs with provenance and expected contracts, not copied temporary repositories. Git bundles reconstruct histories portably. Scenario TOML declares operations and assertions. The canonical “fast, not purple” snake scenario permanently proves atomic parent-to-state pick. Corrupt fixtures cover every durable phase boundary. `tests/support` builds fresh repos and injects faults; production never depends on it.

Unit tests stay beside pure modules. Cross-module observable contracts live under `tests/`. Golden files are appropriate only for deterministic user/machine output and include an intentional-update mechanism; they never replace semantic assertions.

## Packaging

Release output is a signed, reproducible single binary with no runtime dependency beyond Git for normal Git mode. JJ is detected only when requested/needed. Installers and package manifests are sources or release-generated artifacts, not independent logic. Completions/man pages always come from `cli::definition`. Package channels are advertised only after clean-machine install and upgrade verification. Source install remains supported through Cargo.

## Failure modes and containment

| Failure | Structural containment |
|---|---|
| command mutates before recovery point | mutation token unavailable until lock + durable prepare |
| Git succeeds, metadata append fails | durable operation records intended/observed effects; repair reconciles before new mutation |
| event/projection divergence | same SQLite transaction; projections rebuild from journal |
| external Git change races JJK | lock, pre-plan reconcile, expected-OID checks, verify/repair |
| optional JJ missing/broken | explicit capability result and Git-only plan; never correctness dependency |
| CLI leaks into library | typed `Invocation`; clap types remain private |
| schema silently changes | committed generated schemas, drift check, migration/compatibility gate |
| non-Unicode Git args corrupted | native string/byte path through classifier and process adapter |
| passthrough changes behavior | inherited stdio/env/cwd/signals and exact exit; conformance fixture |
| feature matrix explodes | only integration-level flags; no per-command/domain flags |
| architecture fragments early | one package; evidence gate for crate extraction |
| native plugin corrupts repo | no in-process third-party plugins; later capability-scoped protocol |
| startup regresses | lazy bootstrap plus benchmark budgets for every fast path |
| legacy importer becomes permanent coupling | isolated one-way adapter with deletion criterion after supported migration window |

## Staged implementation order

Each stage creates only modules it uses and leaves the repository runnable. No empty “future architecture” directories.

1. **Skeleton and fast process boundary:** package, `main`, facade, structured errors, CLI definition/routing, process adapter, transparent Git passthrough, and startup/passthrough contract tests. Establish native-argument fidelity before semantic commands.
2. **Domain spine:** opaque IDs, state/attempt/workspace facts, event v1, operation phases, policies, ports, and compile-time boundary checks. Implement no speculative event types beyond the first vertical slice.
3. **Durable core vertical slice:** SQLite migration 0001, journal/operation/projection adapters, lock/filesystem/clock/ID adapters, coordinator, repair, Git discovery/observation, `init`, `current`, and `status`. Fault-inject each transaction phase.
4. **Meaningful state loop:** capture (`save`/`step`/`nice`), external Git reconciliation, exact `return`, `show`/`diff`, and archive/recover. Import legacy `.jjk/repo.json` only after the target event model works.
5. **Topology and composition:** attempt/fork/worktree, graph read model/rendering, navigation history, exact atomic pick, canonical snake regression, concurrent-writer fixtures.
6. **Whole-control recovery:** undo/redo, backup/load/freeze, migration snapshots, corrupt-store corpus, restoration drills, and generated manifest/event schemas.
7. **Daily UX and distribution:** fuzzy resolution with confidence, polished `see`/story, JSON envelope, shell assets, man page, benchmarks, signed packaging, clean-machine verification.
8. **Optional JJ acceleration:** create JJ adapter only now; prove deterministic parity and explicit degradation against Git-only conformance.
9. **Post-v0.1 seams, evidence-gated:** TUI, daemon/protocol, forge discovery, agent/editor adapters. Add each directory and feature only with a working vertical slice and consumer.

Within every stage, implement walking skeleton → real fixture → failure injection → user-visible command → contract evidence. Do not build all domain modules first and connect them later.

## Acceptance checks

- `cargo metadata` reports one package and no internal workspace fragmentation.
- The complete basic loop can be traced from CLI to coordinator to ports/adapters and back without a dependency cycle.
- Architecture checks reject a domain-to-SQL/CLI import, an app-to-concrete-adapter import, and render-to-port import.
- External code cannot construct mutation capabilities or access adapter/storage internals.
- A transparent Git fixture proves native argv, cwd, environment, stdin/stdout/stderr, signals, and exit code preservation.
- `current`/`status` do not initialize JJ, forge, an async runtime, or migration writers and meet the warm latency budget.
- Fault injection after every protocol phase yields either the old verified state or a discoverable repairable operation, never silent dual truth.
- Journal replay rebuilds projections byte-for-byte at the semantic/read-model level.
- Generated schemas/completions/man pages reproduce without diff.
- Every shipped feature combination runs the same Git-only safety conformance corpus.
- Fixtures cover Git-only, colocated JJ, bare repositories, submodules, monorepos, linked worktrees, forks, external commits, concurrent writers, and corrupt/interrupted storage.
- No directory in the tree is empty or present solely for an imagined future component.

## Explicit non-goals

- No internal microservice or multi-crate workspace in v0.1.
- No stable plugin ABI, dynamic library loading, WASM host, or general dependency-injection framework.
- No second storage implementation until benchmark and correctness evidence challenge SQLite.
- No ORM; explicit SQL keeps journal/projection transactions and migrations inspectable.
- No `utils`, `common`, generic repository pattern, event-bus framework, or one-file-per-trivial-type ceremony.
- No runtime schema generation or build-time network access.
- No JJ requirement for correctness or Git interoperability.
- No editor-specific duplicate business logic; later surfaces use the same library/protocol.
- No speculative forge, AI composition, timeshift, or remote-sync packages before their vertical slice is scheduled and implemented.

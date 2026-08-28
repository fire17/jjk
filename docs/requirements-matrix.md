# JJK v0.1 Requirements Matrix

Disk-truth audit of `VISION.md`, `origins.md`, `CONTRACTS.md`, and material requirements recovered in `legacy/jjk_v1/vision_overhaul.md`. `VISION.md:1` and `origins.md:6` contain the same canonical sentence; `F-*` rows split its independently testable clauses without counting the duplicate twice.

**Status vocabulary:** **proven** = an executable proof asset on disk exercises the compiled binary or named behavior; **implemented** = a concrete implementation symbol exists but the full required proof does not; **blocked** = stable/release-required behavior or proof is absent/incomplete; **experimental** = explicitly outside stable v0.1. Architecture prose and type declarations are never counted as proof.

## Founding sentence

| ID | Sentence-level requirement (source) | Exact implementation | Exact proof / blocker | Status |
|---|---|---|---|---|
| F-01 | State-of-the-art, forward-looking, cleanly organized architecture and file/folder structure (`VISION.md:1`; `origins.md:6`) | Layered modules `src/{domain,app,ports,adapters,cli,render}`; public boundary `src/lib.rs` | No behavioral acceptance test can prove a qualitative “state of the art/perfect” claim; release remains gated by rows C-* | blocked |
| F-02 | Branching, graph connections, and special operations are first-class (`VISION.md:1`; `origins.md:6`) | `domain::graph::{StateGraph,GraphEdge,EdgeKind}`; `domain::attempt::Attempt`; `runtime::{fork,traverse,visibility}` | `tests/state_runtime.rs::graph_navigation_fork_and_visibility_are_durable`; full edge/acyclicity and composition proof absent (C-GRAPH-001) | implemented |
| F-03 | Wargame unknowns and execute a complete development/refinement loop (`VISION.md:1`; `origins.md:6`) | Planning artifacts `docs/wargame.md`, `docs/unknowns.md`, `ROADMAP.md` | Process/architecture artifacts only; not product proof | implemented |
| F-04 | Produce a stable usable JJK v0.1 rewrite honoring the vision and all human instructions (`VISION.md:1`; `origins.md:6`) | Package version `Cargo.toml:3`; binary/library targets `Cargo.toml:13-19` | Stable release gates C-CORE-004..C-SOURCE-001 remain open | blocked |
| F-05 | Better engineered, “faster than instant,” and powerful (`VISION.md:1`; `origins.md:6`) | Release profile `Cargo.toml:49-53`; bounded orientation design in `runtime::{current,status,list}` | No release-binary benchmark assets/raw samples for C-PERF-001..004; “instant/perfect” is not measurable as written | blocked |
| F-06 | JJK-native command names deliberately differ from Git unless deliberately enhancing Git (`VISION.md:1`; `origins.md:6`) | Versioned registry `cli::route::{REGISTRY_VERSION,NATIVE,ENHANCED,claimed_commands,classify}` | `src/cli/route.rs::tests::final_registry_claims_only_stable_commands`; `tests/git_routing.rs::enhanced_status_and_help_are_classified_without_bootstrap` | proven |
| F-07 | `jjk status` is a deliberate Git-like enhancement (`VISION.md:1`; `origins.md:6`) | `cli::route::dispatch` claims only recognized status presentation forms; `runtime::status` | Classification proven by `tests/git_routing.rs::enhanced_status_and_help_are_classified_without_bootstrap`; required Git+JJK golden matrix absent | implemented |
| F-08 | `rebase`, `clone`, and every unenhanced/future Git verb auto-forward to real Git with full compatibility (`VISION.md:1`; `origins.md:6`) | `cli::route::{dispatch,route}` default `Passthrough`; `main::delegate_git`; `adapters::git::passthrough` | `tests/git_routing.rs::{future_git_verb_preserves_entire_argv,supervised_passthrough_preserves_exit_code}`; full byte/TTY/signal differential corpus absent | implemented |
| F-09 | Design and execution must be “unimaginably perfect” (`VISION.md:1`; `origins.md:6`) | No finite implementation symbol can satisfy an unbounded superlative | Mechanically replaced by all explicit C-* release gates; not independently provable | blocked |

## Recovered material requirements

| ID | Recovered requirement (`legacy/jjk_v1/vision_overhaul.md`) | Exact implementation | Exact proof / blocker | Status |
|---|---|---|---|---|
| L-01 | Semantic state is the user primitive with stable JJK/Git identity, intent, metadata, parentage, provenance, and evidence (:80-98) | `domain::state::{State,StateKind}`; `domain::provenance::Provenance`; `domain::evidence::ValidationRecord`; runtime `capture` | Binary graph/capture smoke: `tests/state_runtime.rs::canonical_state_engine_captures_graph_and_restores_content`; complete field/replay equality absent | implemented |
| L-02 | Git stores interoperable objects; JJ is optional; JJK owns semantics and cannot trap the repository (:100-106) | `ports::{git,jj,repository}`; `adapters::{git,jj}`; `domain::capability::CapabilityReport` | `tests/git_routing.rs::broken_jj_degrades_without_affecting_git`; uninstall/fsck and parity matrices absent | implemented |
| L-03 | Existing Git history/refs import and external Git changes reconcile idempotently without disturbing repository truth; ambiguity stops (:108-123) | Git discovery `adapters::git::GitCli::discover`; runtime `setup`; domain observation events `domain::event::EventV1` | Setup idempotency smoke exists, but import/reconcile implementation and differential mutation corpus required by C-CORE-001/C-GIT-004 are absent | blocked |
| L-04 | Historical return restores exact content, preserves prior futures, and branches only on real divergence (:125-136) | `runtime::{restore,activate_state,capture}`; `domain::attempt::Attempt` | Exact tree return in `tests/state_runtime.rs::canonical_state_engine_captures_graph_and_restores_content`; sibling-divergence fixture absent | implemented |
| L-05 | Attempts are the high-level concept; worktrees enable honest concurrent isolation; never claim subprocess cwd mutation (:138-149) | `domain::attempt::Attempt`; `domain::workspace::{Workspace,WorkspaceLease}`; `runtime::fork` emits worktree path | `tests/state_runtime.rs::worktree_fork_is_isolated_and_shares_repository_state`; ownership/dead-worker proof absent | implemented |
| L-06 | Atomic pick applies only logical-parent→state delta with full provenance; semantic composition preserves multiple attempts (:151-158) | Model vocabulary `domain::graph::EdgeKind::{DeltaDerivedFrom,ComposedFrom}` and `domain::event::EventV1::DeltaApplied` | `runtime::dispatch_native` has no `pick` arm; no canonical fast-only proof | blocked |
| L-07 | Canonical promotion is explicit, evidence-gated, atomic, reversible (:160-173) | Models `domain::event::EventV1::CanonicalPromoted`, graph promotion edges, validation records | No runtime promotion command or end-to-end rollback proof; experimental composition/promotion is not stable registry scope | blocked |
| L-08 | Graph is the primary deterministic explanation surface across terminal/GUI, exposing identities, topology, dirty/incomplete truth (:175-194) | `domain::graph::StateGraph`; `runtime::list`; `render::{graph,table,human,json}` | Binary human/JSON smoke `tests/state_runtime.rs::human_output_is_readable_while_json_remains_machine_stable`; full facts, widths, TUI/GUI parity absent | implemented |
| L-09 | Navigation supports exact/fuzzy return, toggle, visit history, ancestry, orientation, show/diff/story; ambiguity is never silently chosen (:196-209) | Registry contains `return/back/forward/up/down/current/story`; runtime implements `return/up/down/current/story` | `tests/state_runtime.rs::graph_navigation_fork_and_visibility_are_durable`; `back/forward` unavailable and fuzzy ambiguity/show/diff proofs absent | blocked |
| L-10 | Delete/archive is reversible hiding; undo/redo restores whole JJK+Git control state; backup/load differs from portable freeze (:211-221) | `runtime::{visibility,backup,load}`; `SqliteStore::{archive_runtime_state,backup_to}`; event models archive/backup/restore | Archive/recover and backup/load binaries exercised in `state_runtime`; `undo/redo/freeze` have no runtime arm and exact whole-control restoration is absent | blocked |
| L-11 | Agents use an explicit safe protocol, isolated worktrees, typed factual handoff, validation evidence, resume/reject/promote context (:223-248) | `domain::workspace::{WorkspaceHandoff,ResumeCommand,WorkspaceLease}`; registry claims `handoff/validate` | No runtime arms or compiled-binary handoff/resume proof | blocked |
| L-12 | Fork/PR projections separate rich exploration from continuously refreshed upstream-ready submissions; PR Radar/Feature Harvest sandbox candidates (:250-261) | Graph/event vocabulary for derived/external candidates only | Explicitly experimental in `CONTRACTS.md:79`; no runtime/proof | experimental |
| L-13 | Functional history is an immutable, provenance-linked, human-correctable derived view; AI proposes, deterministic core materializes (:263-274) | General provenance/graph types only | Explicitly experimental in `CONTRACTS.md:79`; no functional projection runtime/proof | experimental |
| L-14 | Timeshift is componentized, secret-excluding, previewable, partial, and honest about adapter limits (:276-289) | Event vocabulary includes Timeshift capture/restore; no stable command claim | Explicitly experimental in `CONTRACTS.md:79`; no adapters/runtime/proof | experimental |
| L-15 | New user trusts six-verb loop in <5 min; expert can inspect effects; agents are noninteractive; surfaces agree; capabilities distinguish stability (:604-619) | Registry and JSON/human render primitives | UX study, effect inspection, cross-surface schema parity, and capability-availability proof absent; covered individually by C-UX/C-GRAPH/C-AGENT | blocked |
| L-16 | Removal leaves a valid understandable Git repository (:620) | Git-backed captures and passthrough architecture | No uninstall/export plus `git fsck/status/log/worktree` proof | blocked |

## Acceptance contracts and stable scope

Every hard gate in `CONTRACTS.md:23-50` is represented once. A model/type alone is implementation, never proof.

| ID | Stable claim | Exact implementation symbol(s) | Exact proof asset or honest release blocker | Status |
|---|---|---|---|---|
| C-CORE-001 | Setup imports commits/refs idempotently without changing repository/user config | `runtime::setup`; `SqliteStore::{open,open_existing}` | `state_runtime::canonical_state_engine_captures_graph_and_restores_content` proves repeated setup identity only; fingerprint/import-facts proof absent | blocked |
| C-CORE-002 | Capture creates complete stable semantic identity backed by reachable Git | `runtime::{capture,runtime_event,state_view}`; `SqliteStore::capture_runtime_state`; `domain::state::State` | Binary capture/JSON/graph smoke exists; database/event replay and every required field equality absent | implemented |
| C-CORE-003 | Exact return preserves descendants; sibling attempt only on divergence | `runtime::{restore,activate_state,capture}` | Exact file restore proven in `state_runtime`; green→purple/orange reachability/divergence fixture absent | implemented |
| C-CORE-004 | Pick applies exactly one atomic delta with provenance | Registry claims `pick`; no `runtime::dispatch_native` arm | Missing runtime primitive and purple+fast→orange proof | blocked |
| C-CORE-005 | Archive/recover and whole-control undo/redo | `runtime::visibility`; `SqliteStore::archive_runtime_state`; registry claims undo/redo | Archive/recover proven by `state_runtime::graph_navigation_fork_and_visibility_are_durable`; undo/redo unavailable | blocked |
| C-TXN-001 | Crash recovery at every durable boundary | `app::transaction`; `domain::operation::{OperationPlan,OperationPhase,OperationReceipt}` | No fault-injection compiled-binary crash matrix | blocked |
| C-TXN-002 | Recovery preserves externally changed bytes/refs | Operation precondition/fingerprint models | No recovery runtime fixture returning `recovery_required` | blocked |
| C-TXN-003 | Concurrent readers; serialized/typed-conflict writers; no lost update | SQLite/storage locking and workspace lease models | No multiprocess stress/version proof | blocked |
| C-GIT-001 | Git remains valid/useful before/during/after; metadata removal is safe | `adapters::git::GitCli`; Git-backed runtime captures | No uninstall/export + native Git fsck/status/log/worktree matrix | blocked |
| C-GIT-002 | Unenhanced Git is transparent | `cli::route::dispatch`; `main::delegate_git`; `adapters::git::passthrough` | Argv and exit fragments proven in `tests/git_routing.rs`; stdout/stderr/TTY/signal/cwd/side-effect differential corpus absent | implemented |
| C-GIT-003 | Enhanced status combines unsuppressed Git truth and JJK orientation/recovery | `cli::route::dispatch`; `runtime::status` | No TTY/NO_COLOR/width/non-TTY/JSON golden contract | implemented |
| C-GIT-004 | External Git mutations reconcile as immutable facts or ambiguity, idempotently | Domain observation events; `app::reconcile` | `app::reconcile` is not wired into runtime; no mutation corpus | blocked |
| C-JJ-001 | Git-only completeness; optional JJ explicit/parity-tested; broken JJ degrades before mutation | `adapters::jj::{probe,JjCapabilities}`; capability model | `git_routing::broken_jj_degrades_without_affecting_git` proves probe degradation only; workflow parity/capability report absent | implemented |
| C-GRAPH-001 | Acyclic logical graph; explicit composition; separated identities; deterministic traversals | `domain::graph::{StateGraph,GraphEntity,EdgeKind}` | Domain tests may check construction, but required property suite and orange/purple compiled-binary golden are absent | implemented |
| C-GRAPH-002 | CLI/JSON/TUI/GUI share one query/action graph model | `app::query`; `domain::graph`; `render::*` | No TUI/GUI adapters or cross-surface golden identity/topology proof | blocked |
| C-AGENT-001 | Concurrent agents have isolated owned worktrees and declared integration boundary | `domain::workspace::{Workspace,WorkspaceLease,WorkspaceLeaseTable}`; `runtime::fork` | Single worktree isolation proven; parallel lease/dead-worker recovery absent | implemented |
| C-AGENT-002 | Typed handoff includes all required facts and exact resume | `domain::workspace::{WorkspaceHandoff,ResumeCommand}` | Registry claims `handoff`, runtime unavailable; no JSON schema/resume smoke | blocked |
| C-MIG-001 | Import legacy v1 metadata once with provenance and rollback | Legacy adapter modules and event migration vocabulary | No compiled-binary golden corpus/checksum/rollback proof | blocked |
| C-BACKUP-001 | Backup/load/freeze distinct, checksummed, previewable, exact; pre-load recovery point | `runtime::{backup,load}`; `SqliteStore::{backup_to,verify_backup}`; backup/freeze domain commands | Online backup verification/refusal/corruption proven in `state_runtime`; freeze, preview, pre-load point, exact disaster scopes absent | blocked |
| C-UX-001 | Fresh user completes and explains loop in five minutes | Implemented basic runtime loop | No n≥3 fresh-user protocol evidence | blocked |
| C-UX-002 | Accessible 40/80/120, NO_COLOR, non-TTY, JSON; color not sole signal | `render::{human,json,table,style}`; runtime presentation parsing | Human/JSON smoke only; accessibility/width snapshots absent | implemented |
| C-PERF-001 | Warm current/status p95 <50 ms | `runtime::{current,status}` | No release benchmark/raw ≥50 samples | blocked |
| C-PERF-002 | Planning/graph first paint p95 <100 ms at 1,000 states | `runtime::{fork,list}` | No release benchmark fixture | blocked |
| C-PERF-003 | Passthrough p95 overhead <5 ms | Unix `main::delegate_git` uses `exec` via `OsProcess` | No paired interleaved benchmark/confidence interval | blocked |
| C-PERF-004 | Hot orientation has bounded metadata reads/no full scan | `runtime::{current,status}` and SQLite queries | No trace/count assertion or large-history fixture | blocked |
| C-SEC-001 | Roots/config/hooks/remotes/env/backups/timeshift cannot escape or leak secrets | Path/domain types; `domain::workspace::SecretBytes` redaction; backup verification | No adversarial compiled-binary corpus or secret-canary evidence | blocked |
| C-RELEASE-001 | Single versioned binary/library, deterministic migrations, completions, source build, verified install/uninstall | Cargo targets/version; registry claims `completion` | Completion runtime unavailable; no macOS/Linux/Windows-or-WSL install matrix | blocked |
| C-SOURCE-001 | Every founding requirement maps to implementation and proof | This file | Matrix exists, but audit cannot pass until every stable blocked row closes | blocked |

### Stable-command availability invariant

`src/cli/route.rs::{NATIVE,ENHANCED}` is the sole versioned command-ownership registry (`REGISTRY_VERSION = 1`). Unowned, unknown, future-global, non-UTF-8, malformed, and unowned-status invocations route to real Git without repository access or argv rewriting (`dispatch`, `route`, `main::delegate_git`). **Release invariant:** every name advertised as native/enhanced must have a reachable `runtime::dispatch_native` implementation and compiled-binary proof; every other verb must remain byte-transparent Git.

Current registry/runtime audit:

- **Available runtime (17):** `setup`, `save`, `step`, `nice`, `see`, `current`, `status`, `story`, `return`, `up`, `down`, `fork`, `archive`, `recover`, `backup`, `load`, `doctor` (capture/list share implementations).
- **Claimed but unavailable (9; release blockers):** `pick`, `freeze`, `back`, `forward`, `undo`, `redo`, `handoff`, `validate`, `completion`. They fall through `runtime::dispatch_native` to `RuntimeError::Unavailable`, so stable help currently advertises placeholders, violating `CONTRACTS.md:79`.
- **Passthrough invariant proof assets:** `src/cli/route.rs::tests::{final_registry_claims_only_stable_commands,explicit_git_escape_strips_exactly_two_tokens,globals_find_owned_command_without_rewriting_argv,status_unknown_and_machine_options_passthrough,future_globals_and_non_unicode_pass_through}` and `tests/git_routing.rs::{future_git_verb_preserves_entire_argv,supervised_passthrough_preserves_exit_code}`. Full C-GIT-002 remains open.

## Release blockers

1. Implement and black-box prove the nine registry/runtime gaps above, or remove them from stable registry/help; stable scope requires all except any explicitly reclassified with a contract change.
2. Complete setup import/reconcile, atomic pick, whole-control undo/redo, freeze/handoff/validation, legacy migration, crash/concurrency recovery, external-Git reconciliation, and optional-JJ workflow parity.
3. Add full Git transparency differential proof, status/accessibility goldens, graph/property/cross-surface proof, security corpus, UX protocol, release benchmarks, and clean-machine install/uninstall matrix.
4. Record release-artifact evidence; source types and architecture documents are not proof.

## Status totals

| Status | Count |
|---|---:|
| proven | 1 |
| implemented | 18 |
| blocked | 33 |
| experimental | 3 |
| **Total requirements** | **55** |

Counts cover F-01..09, L-01..16, and all 30 C-* contract rows. Rows with partial proof remain **implemented** or **blocked** according to whether the stable claim itself is present; no row is upgraded merely because adjacent behavior works.

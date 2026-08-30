# JJK v0.1 Requirements Matrix

Disk-truth audit of `VISION.md`, `origins.md`, `CONTRACTS.md`, and material requirements recovered in `legacy/jjk_v1/vision_overhaul.md`. `VISION.md:1` and `origins.md:6` contain the same canonical sentence; `F-*` rows split its independently testable clauses without counting the duplicate twice.

**Status vocabulary:** **proven** = an executable proof asset on disk exercises the compiled binary or named behavior; **implemented** = a concrete implementation symbol exists but the full required proof does not; **blocked** = stable/release-required behavior or proof is absent/incomplete; **experimental** = explicitly outside stable v0.1. Architecture prose and type declarations are never counted as proof.

## Founding sentence

| ID | Sentence-level requirement (source) | Exact implementation | Exact proof / blocker | Status |
|---|---|---|---|---|
| F-01 | State-of-the-art, forward-looking, cleanly organized architecture and file/folder structure (`VISION.md:1`; `origins.md:6`) | Layered modules `src/{domain,app,ports,adapters,cli,render}`; public boundary `src/lib.rs` | No behavioral acceptance test can prove a qualitative “state of the art/perfect” claim; release remains gated by rows C-* | blocked |
| F-02 | Branching, graph connections, and special operations are first-class (`VISION.md:1`; `origins.md:6`) | `domain::graph::{StateGraph,GraphEdge,EdgeKind}`; `domain::attempt::Attempt`; runtime fork/navigation/pick/archive/history operations | `graph_properties::{generated_logical_parent_graphs_are_acyclic,state_order_and_graph_traversal_are_deterministic}`; `snake_workflow::val_core_003_004_canonical_snake_preserves_futures_and_picks_only_the_atomic_delta`; `recovery_workflow::val_core_005_archive_recover_restores_exact_topology_and_every_other_surface` | proven |
| F-03 | Wargame unknowns and execute a complete development/refinement loop (`VISION.md:1`; `origins.md:6`) | Planning artifacts `docs/wargame.md`, `docs/unknowns.md`, `ROADMAP.md` and executable acceptance suites | Process artifacts are complete; product results are independently traced in C-* | proven |
| F-04 | Produce a stable usable JJK v0.1 rewrite honoring the vision and all human instructions (`VISION.md:1`; `origins.md:6`) | Package version `Cargo.toml:3`; binary/library targets `Cargo.toml:27-33`; complete stable runtime dispatch | Local release gates are executable; publication and clean published-artifact install evidence remain C-RELEASE-001 blockers | implemented |
| F-05 | Better engineered, “faster than instant,” and powerful (`VISION.md:1`; `origins.md:6`) | Release profile `Cargo.toml:64-68`; bounded orientation design in `runtime::{current,status,list}` | Explicit quantitative C-PERF-001..004 gates pass in retained release-binary and bounded-read evidence; the unbounded phrase itself is represented by those budgets | implemented |
| F-06 | JJK-native command names deliberately differ from Git unless deliberately enhancing Git (`VISION.md:1`; `origins.md:6`) | Versioned registry `cli::route::{REGISTRY_VERSION,NATIVE,ENHANCED,claimed_commands,classify}` | `cli::route::tests::final_registry_claims_only_stable_commands`; `distribution_smoke::stable_help_is_registry_exact_and_every_claim_reaches_an_implementation` | proven |
| F-07 | `jjk status` is a deliberate Git-like enhancement (`VISION.md:1`; `origins.md:6`) | Closed status grammar in `cli::output`; `runtime::status` combines Git porcelain and JJK orientation | `git_passthrough_conformance::unowned_status_forms_are_not_stolen_by_enhanced_status`; `human_workflow::{human_output_is_plain_semantic_and_bounded_at_supported_widths,piped_reads_are_deterministic_noninteractive_and_machine_json_marks_current}` | proven |
| F-08 | `rebase`, `clone`, and every unenhanced/future Git verb auto-forward to real Git with full compatibility (`VISION.md:1`; `origins.md:6`) | `cli::route::{dispatch,route}` default `Passthrough`; Unix `exec` path in `main::delegate_git`; supervised adapter elsewhere | `git_passthrough_conformance::{passthrough_preserves_native_argv_cwd_environment_output_and_exit_without_bootstrap,non_utf8_verb_and_arguments_reach_git_byte_for_byte,representative_real_git_commands_have_identical_observable_side_effects}` | proven |
| F-09 | Design and execution must be “unimaginably perfect” (`VISION.md:1`; `origins.md:6`) | No finite implementation symbol can satisfy an unbounded superlative | Mechanically replaced by all explicit C-* release gates; not independently provable | blocked |

## Recovered material requirements

| ID | Recovered requirement (`legacy/jjk_v1/vision_overhaul.md`) | Exact implementation | Exact proof / blocker | Status |
|---|---|---|---|---|
| L-01 | Semantic state is the user primitive with stable JJK/Git identity, intent, metadata, parentage, provenance, and evidence (:80-98) | `domain::state::{State,StateKind}`; `domain::provenance::Provenance`; `domain::evidence::ValidationRecord`; runtime `capture` | Binary graph/capture smoke: `tests/state_runtime.rs::canonical_state_engine_captures_graph_and_restores_content`; complete field/replay equality absent | implemented |
| L-02 | Git stores interoperable objects; JJ is optional; JJK owns semantics and cannot trap the repository (:100-106) | `ports::{git,jj,repository}`; `adapters::{git,jj}`; `domain::capability::CapabilityReport` | `tests/git_routing.rs::broken_jj_degrades_without_affecting_git`; uninstall/fsck and parity matrices absent | implemented |
| L-03 | Existing Git history/refs import and external Git changes reconcile idempotently without disturbing repository truth; ambiguity stops (:108-123) | Git discovery/observation and runtime setup reconciliation | Setup import is proven by `repository_shapes::{setup_imports_existing_sha1_history_once_without_mutating_git_facts,setup_imports_detached_head_without_attaching_or_moving_it}`; executable post-setup external-mutation and ambiguity corpus required by C-GIT-004 is absent | implemented |
| L-04 | Historical return restores exact content, preserves prior futures, and branches only on real divergence (:125-136) | `runtime::{restore,activate_state,capture}`; attempt history | `snake_workflow::val_core_003_004_canonical_snake_preserves_futures_and_picks_only_the_atomic_delta` | proven |
| L-05 | Attempts are the high-level concept; worktrees enable honest concurrent isolation; never claim subprocess cwd mutation (:138-149) | attempt/workspace models and runtime fork | `agent_workflow::val_agent_001_002_isolated_handoff_validation_and_explicit_pick`; `concurrency_recovery::linked_worktree_captures_share_one_store_and_preserve_both_states` | proven |
| L-06 | Atomic pick applies only logical-parent→state delta with full provenance; semantic composition preserves multiple attempts (:151-158) | `runtime::pick`; `DeltaApplied` event and typed graph relations | `snake_workflow::val_core_003_004_canonical_snake_preserves_futures_and_picks_only_the_atomic_delta`; both conflict-preimage tests | proven |
| L-07 | Canonical promotion is explicit, evidence-gated, atomic, reversible (:160-173) | Promotion event/policy/graph vocabulary exists; no runtime promote operation | Explicitly outside stable v0.1 research scope; no policy/ref-update/rollback executable proof, and atomic pick is not canonical promotion | experimental |
| L-08 | Graph is the primary deterministic explanation surface across terminal/GUI, exposing identities, topology, dirty/incomplete truth (:175-194) | shared domain graph/query and terminal/JSON renderers | `graph_properties::{state_order_and_graph_traversal_are_deterministic,compiled_cli_command_sequences_reopen_to_the_same_projection}` prove current graph/CLI behavior; GUI/TUI remain explicitly experimental and have no cross-surface proof | implemented |
| L-09 | Navigation supports exact/fuzzy return, toggle, visit history, ancestry, orientation, show/diff/story; ambiguity is never silently chosen (:196-209) | runtime exact return/back/forward/up/down/current/story; fuzzy return, return-toggle, and JJK-native show/diff are absent | `recovery_workflow::val_core_005_back_forward_truncates_only_navigation_future_not_saved_future` and `graph_properties::down_refuses_ambiguous_siblings_and_preserves_every_candidate` prove the implemented subset; required fuzzy/toggle/show/diff behavior is absent | blocked |
| L-10 | Delete/archive is reversible hiding; undo/redo restores whole JJK+Git control state; backup/load differs from portable freeze (:211-221) | runtime archive/recover/history/backup/load/freeze | `recovery_workflow::{val_core_005_archive_recover_restores_exact_topology_and_every_other_surface,val_core_005_undo_redo_round_trips_refs_index_worktree_and_current_projection,val_backup_001_freeze_create_verify_and_tamper_rejection_are_non_mutating,val_backup_001_backup_load_recovers_after_metadata_and_ref_loss_with_exact_scope}` | proven |
| L-11 | Agents use an explicit safe protocol, isolated worktrees, typed factual handoff, validation evidence, resume/reject/promote context (:223-248) | runtime fork/handoff/validate/pick and typed workspace models | `agent_workflow::val_agent_001_002_isolated_handoff_validation_and_explicit_pick` | proven |
| L-12 | Fork/PR projections separate rich exploration from continuously refreshed upstream-ready submissions; PR Radar/Feature Harvest sandbox candidates (:250-261) | Graph/event vocabulary for derived/external candidates only | Explicitly experimental in `CONTRACTS.md:79`; no runtime/proof | experimental |
| L-13 | Functional history is an immutable, provenance-linked, human-correctable derived view; AI proposes, deterministic core materializes (:263-274) | General provenance/graph types only | Explicitly experimental in `CONTRACTS.md:79`; no functional projection runtime/proof | experimental |
| L-14 | Timeshift is componentized, secret-excluding, previewable, partial, and honest about adapter limits (:276-289) | Event vocabulary includes Timeshift capture/restore; no stable command claim | Explicitly experimental in `CONTRACTS.md:79`; no adapters/runtime/proof | experimental |
| L-15 | New user trusts six-verb loop in <5 min; expert can inspect effects; agents are noninteractive; surfaces agree; capabilities distinguish stability (:604-619) | progressive help, human/JSON renderers, explicit capabilities and agent protocol | `human_workflow::{scripted_fresh_user_can_orient_capture_inspect_return_and_diagnose,piped_reads_are_deterministic_noninteractive_and_machine_json_marks_current,help_is_progressive_and_never_advertises_outside_the_stable_registry}`; elapsed fresh-human n≥3 evidence remains absent | implemented |
| L-16 | Removal leaves a valid understandable Git repository (:620) | Git-backed states/refs and standalone uninstall script | Git passthrough/repository-shape suites prove continued native Git operation; published-artifact uninstall drill remains in C-RELEASE-001 | implemented |

## Acceptance contracts and stable scope

Every hard gate in `CONTRACTS.md:23-50` is represented once. A model/type alone is implementation, never proof.

| ID | Stable claim | Exact implementation symbol(s) | Exact proof asset or honest release blocker | Status |
|---|---|---|---|---|
| C-CORE-001 | Setup imports commits/refs idempotently without changing repository/user config | runtime setup/reconciliation | `repository_shapes::{setup_preserves_an_unborn_repository_and_is_idempotent,setup_imports_existing_sha1_history_once_without_mutating_git_facts,setup_imports_detached_head_without_attaching_or_moving_it}` | proven |
| C-CORE-002 | Capture creates complete stable semantic identity backed by reachable Git | runtime capture, SQLite projection, state/event models | `state_runtime::canonical_state_engine_captures_graph_and_restores_content` and `graph_properties::compiled_cli_command_sequences_reopen_to_the_same_projection` prove binary capture/reopen behavior; complete required-field CLI/database/event replay equality remains absent | implemented |
| C-CORE-003 | Exact return preserves descendants; sibling attempt only on divergence | runtime restore/activate/capture | `snake_workflow::val_core_003_004_canonical_snake_preserves_futures_and_picks_only_the_atomic_delta` | proven |
| C-CORE-004 | Pick applies exactly one atomic delta with provenance | runtime pick and `DeltaApplied` event | canonical snake proof plus symbolic/detached conflict-preimage tests in `snake_workflow.rs` | proven |
| C-CORE-005 | Archive/recover and whole-control undo/redo | runtime visibility and control history | all three `val_core_005_*` recovery tests | proven |
| C-TXN-001 | Crash recovery at every durable boundary | transaction coordinator, failpoints, durable receipts | `concurrency_recovery::{capture_failpoints_preserve_exact_pre_or_discoverable_repair_state,reachable_capture_crash_is_durably_visible_to_doctor_after_reopen}` | proven |
| C-TXN-002 | Recovery preserves externally changed bytes/refs | preconditions/fingerprints and recovery-required status | crash/failpoint corpus and dirty refusal proof preserve exact preimages | proven |
| C-TXN-003 | Concurrent readers; serialized/typed-conflict writers; no lost update | SQLite writer coordination and read projections | all concurrency/recovery multiprocess tests | proven |
| C-GIT-001 | Git remains valid/useful before/during/after; metadata removal is safe | Git-backed captures and transparent passthrough | repository-shape and real-Git side-effect suites; published uninstall drill remains C-RELEASE-001 | implemented |
| C-GIT-002 | Unenhanced Git is transparent | closed routing plus direct exec/supervised passthrough | complete `git_passthrough_conformance.rs` differential suite | proven |
| C-GIT-003 | Enhanced status combines unsuppressed Git truth and JJK orientation/recovery | runtime enhanced status | width/human/JSON workflow proofs and explicit capability status proof | proven |
| C-GIT-004 | External Git mutations reconcile as immutable facts or ambiguity, idempotently | runtime observation/reconciliation | Setup import and reopened JJK projection behavior are proven; the required differential external-commit/branch/worktree/fetch/rebase/merge mutation and ambiguity corpus is absent | implemented |
| C-JJ-001 | Git-only completeness; optional JJ explicit/parity-tested; broken JJ degrades before mutation | JJ adapter/capability report | both black-box tests in `jj_parity.rs` plus stable capability contract | proven |
| C-GRAPH-001 | Acyclic logical graph; explicit composition; separated identities; deterministic traversals | typed graph domain and projections | complete property suite in `graph_properties.rs` plus canonical snake proof | proven |
| C-GRAPH-002 | CLI/JSON/TUI/GUI share one query/action graph model | domain graph/query and current CLI/JSON adapters | CLI/JSON agree; TUI/GUI remain explicitly experimental and therefore outside stable-v0.1 release proof | implemented |
| C-AGENT-001 | Concurrent agents have isolated owned worktrees and declared integration boundary | runtime fork/workspaces/leases | `agent_workflow` and linked-worktree concurrency proof | proven |
| C-AGENT-002 | Typed handoff includes all required facts and exact resume | runtime handoff and typed resume command | `agent_workflow::val_agent_001_002_isolated_handoff_validation_and_explicit_pick` | proven |
| C-MIG-001 | Import legacy v1 metadata once with provenance and rollback | legacy adapter and runtime migration controls | `stable_command_contracts::setup_legacy_migration_is_previewed_preserved_idempotent_and_rollback_safe` | proven |
| C-BACKUP-001 | Backup/load/freeze distinct, checksummed, previewable, exact; pre-load recovery point | runtime backup/load/freeze and verified artifacts | Recovery-workflow disaster and freeze tests plus security refusal/corruption tests prove create/verify/exact restore/refusal; executable preview and mandatory pre-load recovery-point proof remain absent | implemented |
| C-UX-001 | Fresh user completes and explains loop in five minutes | progressive help and six-verb runtime loop | scripted fresh-user proof exists; real-user n≥3 elapsed protocol evidence remains absent | implemented |
| C-UX-002 | Accessible 40/80/120, NO_COLOR, non-TTY, JSON; color not sole signal | bounded terminal and deterministic JSON renderers | `human_workflow::{human_output_is_plain_semantic_and_bounded_at_supported_widths,piped_reads_are_deterministic_noninteractive_and_machine_json_marks_current}` | proven |
| C-PERF-001 | Warm current/status p95 <50 ms | bounded runtime orientation paths | Retained `evidence/performance/release.json`: 100 warm samples on the stated host; current 26.157 ms and status 43.018 ms p95 | proven |
| C-PERF-002 | Planning/graph first paint p95 <100 ms at 1,000 states | runtime fork/list | Retained release-binary 1,000-state evidence: fork 82.135 ms and graph 27.830 ms p95 | proven |
| C-PERF-003 | Passthrough p95 overhead <5 ms | Unix direct `exec`; supervised process adapter elsewhere | Retained paired/interleaved evidence: 4.233 ms p95 overhead; deterministic-bootstrap mean 95% CI 2.054–2.518 ms | proven |
| C-PERF-004 | Hot orientation has bounded metadata reads/no full scan | projection-first runtime current/status | `adapters::sqlite::tests::current_orientation_reads_are_bounded_with_large_history` asserts fewer than 200 SQLite VM steps across 1,000 states; retained small/large timing evidence | proven |
| C-SEC-001 | Roots/config/hooks/remotes/env/backups/timeshift cannot escape or leak secrets | safe-path/domain redaction and verified backups | complete compiled-binary adversarial corpus in `security_invariants.rs` | proven |
| C-RELEASE-001 | Single versioned binary/library, deterministic migrations, completions, source build, verified install/uninstall | Cargo package, release workflow, installers, registry completion | Local distribution contract is proven; clean published macOS/Linux/Windows artifact install/uninstall matrix is pending | blocked |
| C-SOURCE-001 | Every founding requirement maps to implementation and proof | This file | Current matrix maps all 53 rows; final independent audit remains coupled to open performance/release evidence | implemented |

### Stable-command availability invariant

`src/cli/route.rs::{NATIVE,ENHANCED}` is the sole versioned command-ownership registry (`REGISTRY_VERSION = 1`). Unowned, unknown, future-global, non-UTF-8, malformed, and unowned-status invocations route to real Git without repository access or argv rewriting (`dispatch`, `route`, `main::delegate_git`). **Release invariant:** every name advertised as native/enhanced must have a reachable `runtime::dispatch_native` implementation and compiled-binary proof; every other verb must remain byte-transparent Git.

Current registry/runtime audit:

- **Available runtime (26):** every stable advertised command is dispatched by `runtime::dispatch_native`: `setup`, `save`, `step`, `nice`, `see`, `return`, `pick`, `fork`, `freeze`, `current`, `story`, `back`, `forward`, `up`, `down`, `archive`, `recover`, `undo`, `redo`, `backup`, `load`, `handoff`, `validate`, `doctor`, `completion`, and enhanced `status`.
- **Claimed but unavailable: 0.** `distribution_smoke::stable_help_is_registry_exact_and_every_claim_reaches_an_implementation` black-box probes every claimed command.
- **Passthrough invariant:** unknown, future, malformed, non-UTF-8, and deliberately unowned Git forms reach Git without repository bootstrap or argument rewriting; the complete differential corpus is `tests/git_passthrough_conformance.rs`.

## Release blockers

1. Re-run and retain release-binary performance evidence on the exact release commit.
2. Obtain green cross-platform CI/release runs from the exact release commit.
3. Publish immutable artifacts, then verify checksum/provenance and clean install/uninstall on advertised targets.
4. Record the independent final source/contract audit against those artifacts.

## Status totals

| Status | Count |
|---|---:|
| proven | 30 |
| implemented | 15 |
| blocked | 4 |
| experimental | 4 |
| **Total requirements** | **53** |

Counts cover F-01..09, L-01..16, and all 28 C-* contract rows. Rows with partial proof remain **implemented** or **blocked** according to whether the stable claim itself is present; no row is upgraded merely because adjacent behavior works.

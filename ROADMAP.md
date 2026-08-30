# JJK v0.1 Rewrite Execution Roadmap

The development loop is contract-first and vertical. Each stage ends in a usable behavior, adversarial proof, and a saved release-state candidate; no layer is declared complete because its types compile.

## Stage 0 — Freeze truth

1. Preserve sacred founding input and checksum.
2. Transfer prior implementation, tests, docs, and recovered vision into `legacy/` as read-only evidence.
3. Define stable-v0.1 scope and VAL contracts.
4. Complete architecture, wartable, unknowns, and requirements traceability.
5. Review for contradictions; change architecture before source exists.

Exit: every stable claim has an observable contract and an owner module.

## Stage 1 — Walking skeleton

1. Rust library + `jjk` binary with typed errors and response envelope.
2. Root/capability discovery and byte-transparent Git passthrough.
3. SQLite migration runner, event append, projection replay, operation records.
4. `jjk setup`, `jjk status`, `jjk see --format json` through the real layers.
5. Fixture harness invokes the compiled binary in real temporary repositories.

Exit: setup/status/passthrough work end to end; a deleted projection rebuilds exactly.

## Stage 2 — Indestructible state loop

1. Capture Git/index/worktree facts without dropping staged, unstaged, untracked, ignored, executable, symlink, or rename semantics.
2. Implement explicit capture through `save`, `step`, and `nice` annotation policy.
3. Implement deterministic graph plus `current`/`see`/`story` queries; native Git retains `show` and `diff`.
4. Implement exact return and navigation with sibling-future preservation.
5. Fault inject every durable boundary; implement startup recovery.

Exit: green→purple / green→orange fixture passes across clean/dirty variants and crash matrix.

## Stage 3 — Composition and attempts

1. Attempt/branch/workspace records and worktree ownership.
2. `fork` and shell path handoff; concurrent-agent leases and typed handoffs.
3. Exact atomic `pick` from logical parent→state delta with provenance.
4. Archive/recover and evidence-gated canonical promotion.
5. Conflict plan/continue/abort protocol.

Exit: orange receives fast mode without purple; parallel workers cannot collide; conflicts preserve both sides.

## Stage 4 — Complete recovery surface

1. Whole-control-plane undo/redo.
2. Backup/load with pre-load recovery point.
3. Freeze manifest/bundle and integrity verification.
4. Migration from legacy `repo.json` v1 and rollback to prior installation.
5. Metadata export/remove while Git remains valid.

Exit: destructive disaster drills restore exact declared state and native Git remains usable after removal.

## Stage 5 — Git/JJ compatibility closure

1. Differential passthrough corpus for all installed Git commands, aliases, helpers, flags, TTY, signals, and exit status.
2. External Git mutation/reconciliation corpus.
3. Optional colocated-JJ adapter, operation-log capability use, parity suite, explicit downgrade.
4. Remote/fork/upstream simulation and state-ref transport policy.
5. SHA-1/SHA-256, worktree, submodule, monorepo, bare inspection, and platform fixtures.

Exit: compatibility contracts pass in Git-only and JJ modes; no unadvertised semantic difference.

## Stage 6 — UX and performance

1. Six-verb progressive help and exact ambiguity UX.
2. Width-aware/color-redundant terminal graph, `NO_COLOR`, non-TTY, stable JSON.
3. Prompt integration and completions without startup shell work.
4. Profile hot paths; gate reconciliation and bound queries.
5. Release-binary benchmarks on ordinary, 1k-state, large-history, many-worktree, monorepo, and network-filesystem fixtures.

Exit: every performance contract passes with raw samples; fresh-user n≥3 succeeds 3/3 in under five minutes.

## Stage 7 — Release closure

1. Requirements audit against `VISION.md`, `origins.md`, legacy corpus, and CONTRACTS.
2. Security/privacy review and secret-canary bundle/doctor tests.
3. Targeted tests, full conformance matrix, actual CLI smoke flows, clean install/uninstall.
4. README, command reference, skill, changelog, license, architecture and migration docs reflect only proven behavior.
5. Build reproducible artifacts; verify package channels before advertising them.
6. Save/register through `/sas`; publish only after the user's release acknowledgement where required.

Exit: stable `v0.1_rewrite_sota_fable` has complete evidence, no placeholders, no stale claims, and a one-command verified return path.

## Continuous loop within every stage

`observe source truth → choose smallest complete vertical contract → implement → run actual surface → adversarially falsify → profile → simplify → record evidence → re-check founding intent`

Rules:

- Fix the model/invariant, never suppress a symptom.
- One shared semantic API; no renderer-owned behavior.
- One transaction/recovery path; no command-specific safety shortcuts.
- Every new command is classified native, enhanced, or passthrough before code.
- Every dependency requires a measured benefit and maintained fallback/removal story.
- Failed experiments are removed or documented in the rung graveyard; no dormant production path.
- Performance work begins from profiles and paired baselines, not intuition.
- Release scope may move only by explicit contract change, never by silently skipping a hard fixture.

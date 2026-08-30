# Changelog

All notable changes to the Rust implementation of JJK are recorded here.

This project follows semantic versioning.

## [Unreleased]

## [0.3.0] - 2026-08-29

### Changed

- Control snapshots now record only Git-visible paths (index entries plus untracked, non-ignored files) instead of walking the entire checkout. Ignored content such as `target/`, `node_modules/`, and `.worktrees/` is never stored, restored, or deleted; a repository with a 20 MB ignored artifact no longer grows `state.sqlite3` by ~144 MB per capture.
- Snapshot byte fields are stored as base64 text; the previous JSON array form still loads, so existing control histories, conflict preimages, and backups remain readable.

### Fixed

- `return`, `up`, `down`, `undo`, `redo`, and conflict abort no longer delete uncaptured files. Restores remove only paths tracked by the index, owned by the target snapshot, or captured by the state being left; untracked extras created after a capture survive, and verification checks the restored projection rather than demanding a byte-identical checkout.
- `return`, `up`, `down`, `undo`, and `redo` no longer refuse in a repository whose files were captured but never `git add`ed: when the user's index differs from the state tree, the workspace match now also accepts worktree content that equals the state tree, measured through a private index seeded from the state (`read-tree` + `add -u`). Staged-versus-worktree divergence still refuses.
- Navigation is no longer refused right after a restore because the index stat cache is stale: the staged-versus-worktree check now compares content through a private refreshed index copy instead of trusting `git diff-files` stat data (previously an immediate `undo` after `return` failed until `git status` ran).
- State queries accept the message that produced a label (`pick fast_purple` and `pick "fast purple"` both resolve `fast-purple`) and exact messages, not only the slugified label.
- `jjk <command> --help` prints the exact argument grammar the runtime parses instead of a placeholder; every claimed command must document its grammar (registry test).
- README and the `jjk` skill documented a `fork` grammar the binary rejected; both now show `jjk fork --worktree --json -- "<agent-name>"`.

## [0.2.1] - 2026-08-29

### Fixed

- Preserved the typed writer-conflict exit on Windows when the locked file also prevents reading its owner receipt.

## [0.2.0] - 2026-08-29

### Added

- Added native `star [state]` and `unstar [state]` annotations, including durable JSON state, `current`/`see`/`story` visibility, and idempotent receipts without creating duplicate snapshots.



## [0.1.2] - 2026-08-29

### Fixed

- Eliminated the Linux executable-publication race in the Git passthrough conformance fixture.
- Corrected SQLite current-state hydration and backup-load verification at the restored target.
- Aligned the declared MSRV, CI, and release toolchains on Rust 1.85.0.
- Hardened installer version parsing and corrected archive-root handling in the Homebrew formula.


## [0.1.1] - 2026-08-29

### Fixed

- Closed `git patch-id` stdin before waiting so the canonical snake workflow cannot deadlock on Windows.
- Stabilized linked-worktree identity, recovery publication, routing fixtures, and line-ending isolation across Windows runners.

## [0.1.0] - 2026-08-29

### Added

- Rust 2024 binary and library package for the state-first Git workflow.
- Versioned routing registry separating JJK-native commands, enhanced status, and transparent Git passthrough.
- Semantic state, attempt, graph, workspace, operation, evidence, and handoff domain models.
- Git adapter, optional Jujutsu capability probing, SQLite persistence, transaction recovery, and legacy import work.
- Black-box stable-surface, security, state-runtime, and Git passthrough conformance assets.
- Cross-platform CI and release scaffolding with checksums, SBOM generation, provenance attestations, installers, and a Homebrew formula template.

### Release status

- Stable v0.1.0 is the first release of the Rust rewrite.
- Historical `v0.1.1-stable` GitHub assets belong to the legacy implementation and are not evidence for this rewrite.

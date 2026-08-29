# Changelog

All notable changes to the Rust implementation of JJK are recorded here.

This project follows semantic versioning.

## [Unreleased]

## [0.2.0] - 2026-08-29

### Added

- Added native `star [state]` and `unstar [state]` annotations, including durable JSON state, `current`/`see`/`story` visibility, and idempotent receipts without creating duplicate snapshots.

### Fixed

- Preserved the typed writer-conflict exit on Windows when the locked file also prevents reading its owner receipt.

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

# Legacy baseline

The complete implementation/reference corpus copied into `legacy/jjk_v1/` came from `/Users/magic/wholesomegarden/Codex/jjk_v1` on 2026-08-28. It is read-only evidence for the rewrite, not a build dependency.

Observed source baseline before copying:

- package: `@fire17/jjk` 0.1.1, Bun/TypeScript
- source modules: command routing, Git bridge, JSON store, renderers, watch/repl, typed records
- tests: 23 files
- recovered vision: `vision_overhaul.md`, 2,946 lines
- Git 2.39.3, Jujutsu 0.39.0
- prior full test invocation did **not** pass: it timed out after 120 seconds, and `pick applies only the delta held by the chosen state after multiple returns` exceeded its 5-second test timeout at about 6.0 seconds. This is a rewrite regression/performance fixture, not a green baseline.

No file in `legacy/` may be imported by the release binary. Requirements and fixtures are re-expressed in the Rust implementation and conformance suite.

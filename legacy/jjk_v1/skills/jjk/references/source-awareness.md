# jjk source awareness

Use this reference when the user wants to know how `jjk` works internally, why a behavior exists, or where in the code a feature lives.

## Explain from live source when possible

If the current workspace contains the `jjk` implementation, prefer the live code and tests over prose.

Primary files:

- `src/commands.ts`
  - CLI parsing
  - command dispatch
  - help text
  - orchestration across store, git, and render layers
- `src/store.ts`
  - state graph persistence
  - lane / branch metadata
  - save / return / update behavior
  - undo / redo / backup snapshot logic
- `src/git.ts`
  - Git substrate operations
  - branch switching
  - worktree creation
  - patch / replay / revert helpers
- `src/render.ts`
  - `jjk see`
  - graph output
  - state tables
  - filtered and log-style views
- `src/types.ts`
  - repo and state schema
  - metadata fields and tag shapes

Tests:

- `tests/navigation.test.ts`
- `tests/inspection.test.ts`
- `tests/branch-shaping.test.ts`
- `tests/utility-commands.test.ts`
- `tests/patch-backup.test.ts`
- plus older tests for return, save, pick, update, render, undo, and backup behavior

## How to explain high-level vs low-level

### High-level explanation

Focus on:

- `jjk` as a state graph over Git commits
- branches as active lines of work
- states as named checkpoints with metadata
- recovery and parallel work as first-class workflows

### Low-level explanation

Focus on:

- which command case in `src/commands.ts` handles the behavior
- which store function mutates repo metadata
- which Git helper performs the substrate action
- which tests define the expected behavior

When possible, explain in this form:

1. entrypoint command
2. state/store mutation
3. git substrate action
4. render / observable output
5. test coverage

## Implementation map by feature family

### State creation and progression

- start in `src/commands.ts`
- save semantics in `src/store.ts`
- commit creation and worktree restoration in `src/git.ts`

### Recovery and navigation

- `return`, `continue`, `next`, `prev`, `root`, `trail`, `children`, `parents`
- command routing in `src/commands.ts`
- current-state and branch-state resolution in `src/store.ts`

### Branch shaping

- `move`, `split`, `branch-from`, `rename-state`, `rename-branch`
- primary logic in `src/store.ts`

### Inspection

- `see`, `graph`, `inspect`, `search`, `timeline`, `favorites`
- command routing in `src/commands.ts`
- formatting in `src/render.ts`

### Patch and replay operations

- `show`, `patch`, `files`, `touched`, `replay`, `merge-state`, `revert-state`, `amend`
- command routing in `src/commands.ts`
- state updates in `src/store.ts`
- diff / patch substrate in `src/git.ts`

### Snapshot and backup layer

- `undo`, `redo`, `backup`, `backups`, `snapshot-log`, `load`, `restore`
- snapshot storage and restore in `src/store.ts`
- CLI surface in `src/commands.ts`

## Recommended explanation pattern

When the user asks “how does this work?”:

1. explain the intent in one or two sentences
2. name the main command and data flow
3. point to the actual source files
4. mention the relevant tests

This keeps explanations grounded and trustworthy.

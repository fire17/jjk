![jjk cover](cover.png)

# jjk

`jjk` turns a project into a safe space.

It gives humans and agents a higher-level workflow on top of Git and Jujutsu: save meaningful states, branch without fear, return to good places fast, and keep a readable trail of experiments instead of a pile of fragile commits.

## What Exists In This Repo

- A Bun/TypeScript CLI with safe-space setup, state saves, graph rendering, fuzzy return, current-state navigation history, watch mode, lane creation, story view, freeze bundles, doctor checks, push/pull helpers, and experimental timeshift records.
- A product layer with a README, operating docs, a product site, a Hacker News launch post, and a Codex skill bundle.
- A storage model in `.jjk/` that tracks states and lanes without forcing normal Git branches to become the primary UX.

## Quick Start

```bash
bun run src/cli.ts init
bun run src/cli.ts "baseline before parser rewrite"
bun run src/cli.ts step "extracted state store"
bun run src/cli.ts nice "green tests after cleanup"
bun run src/cli.ts see
```

If you want the local launcher:

```bash
chmod +x ./bin/jjk
./bin/jjk -v
./bin/jjk init
./bin/jjk "safe baseline"
```

## Command Model

### Safe spaces

- `jjk init`
  - Initialize Git if needed.
  - Initialize colocated Jujutsu when `jj` is available.
  - Create `.jjk/repo.json`, local exclude rules, and the initial `main` state.

- `jjk status`
  - Show the current branch, lane, head, worktree counts, latest saved state, and upstream position.

- `jjk current`
  - Show the current saved state that matches your workspace, including its lane, branch, parent, commit, and navigation-history position.

- `jjk map`
  - Scan downward from the current directory for project markers such as `.git`, `.jj`, `.jjk`, and `package.json`.

- `jjk doctor`
  - Show the current branch, lane, state count, `jj` availability, and remote status.

### States

- `jjk <description>`
  - Save the current working tree as a real Git commit, remember it as a `save` state, and maintain a stable continuation branch such as `jjk/green` when helpful. Saves branch away from `main` by default so `main` stays anchored until you explicitly `jjk return main`.
  - If you write `jjk <label>, <desc>`, `jjk` treats the text before the first comma as the state label and saves the text after the comma as a state message in metadata. `jjk see` shows that message alongside the state.

- `jjk step [description]`
  - Save a small meaningful checkpoint.

- `jjk nice [description]`
  - Save a known-good or improving state.

- `jjk star [description]`
  - Save a memorable anchor state.

- `jjk see`
  - Render the logical state graph plus a table view.

- `jjk story`
  - Show only `nice` and `star` states as the memorable narrative of the work.

- `jjk diff [state] [state]`
  - Show a `git diff --stat` view against the latest saved state, one chosen state, or between two saved states.

- `jjk pick <state>`
  - Apply the delta represented by a saved state onto the current branch context and remember the result as a new `step`.

- `jjk promote <state> <nice|star>`
  - Promote an existing saved state to `nice` or `star` without taking another snapshot.

- `jjk return <query>`
  - Resolve a state by id, label, or fuzzy match, then resume its stable continuation branch when it is the tip of that line; otherwise detach at the snapshot so the next save can start a sibling branch.

- `jjk return -`
  - Jump back to the previously visited state, like `cd -`, and keep toggling if you repeat it.

- `jjk back`
  - Step backward through visited current-state history.

- `jjk forward`
  - Step forward through visited current-state history.

- `jjk up`
  - Move to the parent state of the current state.

- `jjk down`
  - Move to a child state of the current state. If there is more than one child, `jjk` will prefer your forward-history path or prompt you interactively.

### Flow

- `jjk lane <name>`
  - Create or switch to a named lane on a dedicated Git branch under `jjk/lane/*`.

- `jjk lane`
  - List known lanes and mark the current one.

- `jjk watch`
  - Watch the filesystem and create grouped `auto` states after a debounce window.

- `jjk push`
  - Push the current branch and all `refs/jjk/states/*` refs to `origin`.

- `jjk pull`
  - Fetch remote `refs/jjk/states/*` and fast-forward pull when possible.

- `jjk freeze [state]`
  - Create a portable Git bundle and JSON manifest for a chosen state.

- `jjk timeshift save [label]`
  - Save the current branch, lane, state, cwd, and shell-facing metadata.

- `jjk timeshift restore <query>`
  - Restore the saved branch and state target. This is an experimental first step toward fuller environment restore.

- `jjk`
  - Start an interactive shell.

## Operating Model

`jjk` uses Git and Jujutsu as substrate layers rather than as the primary mental model:

- Git stores real objects and compatibility refs.
- Jujutsu is enabled when available for graph/recovery integration.
- `.jjk/repo.json` stores the meaning layer: labels, descriptions, kinds, lanes, timeshifts, and freezes.
- State metadata can also hold an optional state message when you save with `jjk <label>, <desc>`.

The current implementation snapshots the working tree into hidden Git commits under `refs/jjk/states/*`. This gives stateful recall without forcing every save onto the visible branch history.

## Product Assets

- Vision: [docs/vision.md](/Users/magic/wholesomegarden/Codex/jjk_v1/docs/vision.md)
- Operating model: [docs/operating-model.md](/Users/magic/wholesomegarden/Codex/jjk_v1/docs/operating-model.md)
- Hacker News post: [marketing/hacker-news-post.md](/Users/magic/wholesomegarden/Codex/jjk_v1/marketing/hacker-news-post.md)
- Skill: [skills/jjk/SKILL.md](/Users/magic/wholesomegarden/Codex/jjk_v1/skills/jjk/SKILL.md)
- Site entry: [site/index.html](/Users/magic/wholesomegarden/Codex/jjk_v1/site/index.html)

Run the product site locally with:

```bash
bun run site:serve
```

## Current Boundaries

- State saves, returns, lanes, watch mode, and freeze bundles are implemented.
- `timeshift` is experimental. It currently restores branch/state context and records shell-facing metadata; it does not recreate a full terminal session.
- PR Radar, Feature Harvest, promotion flows, and richer merge/pick orchestration are documented in the vision, but not yet implemented as first-class commands in this repo.

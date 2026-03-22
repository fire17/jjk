![jjk cover](cover.png)

# jjk

`jjk` turns a project into a safe space.

It gives humans and agents a higher-level workflow on top of Git and Jujutsu: save meaningful states, branch without fear, return to good places fast, and keep a readable trail of experiments instead of a pile of fragile commits.

## Current Image

![jjk current image](cover.png)

## Bombastic JJK Header And Hook

Stop thinking in fragile commits and start thinking in safe, named states. `jjk` gives you a practical working memory for software development: save where you are, fork without fear, return to a known-good place, inspect the real story of the work, and let both humans and agents move fast without losing context.

`jjk` is better than plain Git when the real problem is not object storage but human workflow. Git is excellent at storing commits, refs, and diffs, but it pushes users toward branch anxiety, accidental history coupling, and low-signal commit spam. Jujutsu improves graph flexibility, but it is still fundamentally a version-control substrate, not a state-first UX for humans and agents. `jjk` sits above both: it gives named states, semantic commands, recovery workflows, branch-aware safe spaces, agent-friendly worktrees, and a more teachable mental model. You use `jjk` when you want “save this idea, branch here, return there, replay that change, show me the current story” to be the default way of working instead of raw commit surgery.

## Best Example And Pain Points Solved

Best example:

```bash
jjk init
jjk save "main baseline before parser rewrite"
jjk green
jjk step "extract parser state machine"
jjk step "wire parser into CLI path"
jjk nice "green tests after parser integration"
jjk return "main baseline"
jjk orange
jjk pick green
jjk save "try parser with alternate config flow"
jjk see
```

What this solves:

- “I’m afraid to branch because I’ll make a mess.”
  `jjk` makes branching and returning part of the normal flow.
- “My commit history is full of low-signal checkpoints.”
  `jjk step`, `save`, `nice`, and markers let you separate working memory from polished Git history.
- “I can’t remember where the good state was.”
  `jjk see`, `graph`, `search`, `inspect`, and `return` make recovery first-class.
- “Agents and humans step on each other in the same checkout.”
  `jjk fork <agent> --worktree` gives parallel isolated workspaces.
- “Cherry-picking the right delta is annoying.”
  `jjk pick`, `replay`, `merge-state`, and `revert-state` work in saved-state terms.
- “Git and Jujutsu know the storage graph, but not the meaning of my work.”
  `jjk` stores the meaning layer: labels, descriptions, messages, notes, markers, branch intent, and state relationships.

Cool features:

- branch-aware state graph with `jjk see` and `jjk graph`
- semantic state kinds like `step`, `save`, `nice`, `cherry`, `stash`, and `git`
- true Git-compatible commits under the hood
- agent-friendly worktree creation
- snapshot-based undo / redo / backup / restore
- state metadata like notes, stars, pins, statuses, handoff notes, and aliases

## Give Your Agent This Prompt

Use this when you want another agent to install `jjk`, install the `jjk` skill, and then actively use it while working:

```text
Use the jjk workflow for this project.

1. Install jjk if it is missing.
   Preferred:
   - `brew install fire17/homebrew-jjk/jjk`
   Fallback:
   - `bun add -g @fire17/jjk`

2. If working from source, set it up from the repository:
   - `bun install`
   - `chmod +x ./bin/jjk`
   - use `./bin/jjk` as the local launcher

3. Install or refresh the jjk Codex skill from `skills/jjk/SKILL.md` so you can explain and operate jjk accurately.

4. Use jjk in agentic mode for this task:
   - `jjk init` if needed
   - if parallel work is needed, use `jjk fork <agent-name> --worktree`
   - after each meaningful feature chunk, run `jjk step "<extensive factual message>"`
   - when a coherent milestone is reached, run `jjk save "<clear milestone message>"`
   - if a clearly good state is reached, use `jjk nice "<clear good-state message>"`

5. Teach me what you are doing as you go:
   - explain jjk at a high level
   - explain the exact commands you are using
   - if I ask how something works internally, ground the answer in the current source code and tests

6. Before finishing, tell me:
   - what jjk states you created
   - which branch/worktree you used
   - how to return to the best state if I want to continue later
```

## Easy Install From Bun Or Homebrew

### Homebrew

```bash
brew install fire17/homebrew-jjk/jjk
```

### Bun

```bash
bun add -g @fire17/jjk
```

## Install From Source

Requirements:

- `git` is required
- `jj` is optional, but recommended if you want Jujutsu integration
- `bun` is required when running from source

Source setup:

```bash
git clone <your-jjk-repo-url>
cd jjk_v1
bun install
chmod +x ./bin/jjk
./bin/jjk -v
```

Local source usage:

```bash
./bin/jjk init
./bin/jjk "safe baseline"
./bin/jjk see
```

Or during development:

```bash
bun run src/cli.ts init
bun run src/cli.ts "baseline before parser rewrite"
bun run src/cli.ts step "extracted state store"
bun run src/cli.ts nice "green tests after cleanup"
bun run src/cli.ts see
```

## How To Use: Basic To Advanced

### Basic

Start a safe space and save meaningful points:

```bash
jjk init
jjk save "main baseline"
jjk step "extract parser service"
jjk nice "green tests after cleanup"
jjk see
```

### Branching And Return

Create a line of work, go back, and create a sibling line:

```bash
jjk green
jjk return main
jjk orange
jjk see
```

### Replay And Merge

Take the changes from one state and apply them into another branch:

```bash
jjk return orange
jjk pick fast_purple
jjk save "orange with fast purple delta"
```

### Agentic Parallel Work

Give each agent a separate worktree:

```bash
jjk fork parser_agent --worktree
jjk fork ui_agent --worktree
```

Then inside each worktree:

```bash
jjk step "implement parser validation and add focused tests"
jjk save "parser feature milestone"
```

## User Stories

- “I want to try a risky refactor without losing the good version.”
  Use `jjk save`, do the refactor, and `jjk return` if it goes wrong.

- “I want two ideas in parallel without branch chaos.”
  Use `jjk fork <name> --worktree` for isolated worktrees.

- “I want to remember the good point after tests went green.”
  Use `jjk nice "green tests after config cleanup"`.

- “I want agents to work safely and leave a readable trail.”
  Ask them to use `jjk step` after each meaningful feature chunk and `jjk save` at milestones.

- “I forgot what changed on this branch.”
  Use `jjk graph`, `jjk inspect`, `jjk files`, `jjk touched`, and `jjk snapshot-log`.

## What Exists In This Repo

- A Bun/TypeScript CLI with safe-space setup, state saves, graph rendering, fuzzy return, current-state navigation history, watch mode, lane creation, story view, freeze bundles, doctor checks, push/pull helpers, and experimental timeshift records.
- A product layer with a README, operating docs, a product site, a Hacker News launch post, and a Codex skill bundle.
- A storage model in `.jjk/` that tracks states and lanes without forcing normal Git branches to become the primary UX.

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

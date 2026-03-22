# jjk commands

## Implemented

- `jjk init`
  - Turn the current directory into a safe space and save the initial `main` state.

- `jjk status`
  - Show the current branch, lane, head, worktree counts, latest state, and upstream position.

- `jjk current`
  - Show the current saved state that best matches the workspace, including branch, lane, parent, commit, and history position.

- `jjk <free form description>`
  - Save the current state as a `save`, usually on a stable continuation branch rather than advancing `main`.
  - `jjk <label>, <desc>` splits on the first comma: the label becomes the state label and the trailing text is saved as a state message in metadata.

- `jjk step [description]`
  - Save a small meaningful checkpoint.

- `jjk nice [description]`
  - Save a good or improving state.

- `jjk star [description]`
  - Save a memorable anchor state.

- `jjk see`
  - Show the logical state graph and table.
  - When a state has a saved message from `jjk <label>, <desc>`, show it inline in the graph and table.

- `jjk story`
  - Show the memorable narrative of `nice` and `star` states.

- `jjk diff [state] [state]`
  - Show a diff summary against a chosen state or between two states.

- `jjk pick <state>`
  - Apply the changes represented by a saved state onto the current branch context.

- `jjk promote <state> <nice|star>`
  - Promote an existing state to a memorable or approved state without taking a new snapshot.

- `jjk return <query>`
  - Fuzzy-match a state and resume its stable continuation branch when possible, otherwise detach at that snapshot so the next save can start a new branch.

- `jjk return -`
  - Jump back to the previously visited state, like `cd -`.

- `jjk back`
  - Walk backward through current-state history.

- `jjk forward`
  - Walk forward through current-state history.

- `jjk up`
  - Move to the parent state of the current state.

- `jjk down`
  - Move to a child state of the current state.

- `jjk lane`
- `jjk lane <name>`
  - List lanes or create/switch to a named lane branch.

- `jjk map`
  - Scan for nearby project markers such as `.git`, `.jj`, `.jjk`, and `package.json`.

- `jjk watch`
  - Watch the filesystem and create grouped `auto` states on change.

- `jjk push`
  - Push the current branch and hidden `refs/jjk/states/*` refs.

- `jjk pull`
  - Fetch hidden `refs/jjk/states/*` refs and fast-forward pull when possible.

- `jjk doctor`
  - Report safe-space health and current context.

- `jjk freeze [state]`
  - Export a bundle and manifest for a chosen state.

- `jjk timeshift save [label]`
- `jjk timeshift restore <query>`
  - Experimental repo-context timeshift support.

- `jjk`
  - Open the interactive shell.

## Product Direction

- deeper timeshift across fuller terminal context
- promotion flows
- PR Radar
- Feature Harvest
- richer merge and pick orchestration

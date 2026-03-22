# Operating Model

## Layers

`jjk` is a control plane over three layers:

1. Git
   - compatibility layer
   - real object storage
   - hidden refs for states
   - push/pull interoperability
2. Jujutsu
   - optional graph and recovery layer
   - colocated setup when available
   - import/export bridge for shared history visibility
3. `.jjk`
   - meaning layer
   - labels, descriptions, optional state messages, lanes, timeshifts, freezes

## Persistence

Repo metadata lives in:

- `.jjk/repo.json`

Freeze bundles live in:

- `.jjk/freezes/*.bundle`
- `.jjk/freezes/*.json`

Saved state commits are published under:

- `refs/jjk/states/<id>`

This means:

- the visible branch history stays readable
- the state graph still has real Git objects behind it
- state refs can be pushed and fetched without turning them into normal branch names

## Save Semantics

Saving a state currently works like this:

1. `jjk init` anchors the starting project snapshot as the initial `main` state.
2. Most saves stage tracked and untracked changes and create a real Git commit for the state's active line.
3. Saves that originate from `main` branch away to stable refs like `jjk/green` so the `main` branch stays anchored.
4. An explicit `jjk return main` arms the next save to land on `main` again.
5. Update `refs/jjk/states/<id>`.
6. Record the higher-level meaning in `.jjk/repo.json`.

If the save input uses `jjk <label>, <desc>`, the first comma separates the state label from an optional state message. That message is stored in state metadata and surfaced in `jjk see`.

This keeps each saved state aligned with a concrete Git commit instead of a hidden snapshot only.

## Return Semantics

`jjk return <query>`:

1. resolves a state by id, label, description, or fuzzy match
2. resumes the state's stable continuation branch when that state is the tip of its current line
3. otherwise detaches at that state commit so the next explicit save can begin a new sibling branch
4. keeps the lane association in `.jjk`

This makes rollback and experimentation cheap.

## Lanes

The current implementation maps each lane to a dedicated Git branch:

- `jjk/lane/<slug>`

That is intentionally conservative. It keeps the model inspectable in plain Git while allowing `jjk` to add higher-level meaning above it.

## State Navigation

`jjk return -` jumps back to the previously visited state and can keep toggling between two recent places.

`jjk back` and `jjk forward` walk the remembered current-state history.

`jjk up` moves to the parent state of the current state.

`jjk down` moves to a child state of the current state, preferring the forward-history path when there is one.

## Push / Pull

`jjk push` pushes:

- the current branch
- all `refs/jjk/states/*`

`jjk pull` fetches:

- remote `refs/jjk/states/*`
- then performs a fast-forward pull if the current branch has an upstream

## Watch Mode

Watch mode groups changes into `auto` states after a debounce period. It is designed to remember work without spamming a new state for every file event.

## Experimental Timeshift

The current `timeshift` implementation stores:

- branch
- lane
- current state id
- relative cwd
- shell-facing environment fields

This is enough to restore repo context but not a full terminal state. Full terminal timeshift remains a product direction, not a finished capability.

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
   - labels, descriptions, lanes, timeshifts, freezes

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

1. Use a temporary Git index so the working tree and real index remain untouched.
2. Stage the project snapshot into that temporary index.
3. Write a tree object.
4. Stage tracked and untracked changes, then create a real Git commit on the current branch.
5. Update `refs/jjk/states/<id>`.
6. Record the higher-level meaning in `.jjk/repo.json`.

This keeps each saved state aligned with a concrete Git commit instead of a hidden snapshot only.

## Return Semantics

`jjk return <query>`:

1. resolves a state by id, label, description, or fuzzy match
2. creates or switches to a branch rooted at that state commit
3. keeps the lane association in `.jjk`

This makes rollback and experimentation cheap.

## Lanes

The current implementation maps each lane to a dedicated Git branch:

- `jjk/lane/<slug>`

That is intentionally conservative. It keeps the model inspectable in plain Git while allowing `jjk` to add higher-level meaning above it.

## Up / Down

`jjk up` pushes:

- the current branch
- all `refs/jjk/states/*`

`jjk down` / `jjk pull` fetches:

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

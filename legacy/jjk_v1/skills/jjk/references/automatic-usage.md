# automatic jjk usage

Use this protocol only when the user explicitly asked for operational `jjk` usage.

Do not apply it just because the current topic is `jjk`.

## Decision rule

The user must ask for one of these scopes:

- use `jjk` for this task
- use `jjk` for this conversation
- use `jjk` by default / always / until turned off

If they only asked for explanation, stay in explanation mode.

## Operational protocol

1. Run `jjk init` if the project is not already a safe space.
2. Before risky work, save a state if there is no recent meaningful state.
3. Do the requested work.
4. During multi-step implementation, leave meaningful `jjk step ...` states instead of waiting until the very end.
5. Before finishing, leave at least one coherent state if code changed.
6. Use `jjk save ...` for a milestone or completed series of steps.
7. If the result is clearly good, suggest or apply `jjk nice ...`.
8. If the user asks to revert, use `jjk return`.

## Agent and subagent behavior

If multiple agents will work in parallel:

1. Create a dedicated worktree per agent:
   - `jjk fork <agent-name> --worktree`
2. Keep each agent isolated in its own branch/worktree.
3. Each agent should leave at least one `jjk` state before finishing a coding turn.
4. Prefer `step` during active work and `save` or `nice` at milestones.

## Description style

Descriptions should be factual, specific, and future-friendly.

Good:

- `extract parser service and wire callers`
- `step add branch shaping commands and focused tests`
- `save merged command batch on main`

Avoid:

- `stuff`
- `changes`
- `working on it`

## Grouping guidance

Do not create a new state for every file touch.

Prefer:

- one state before a risky action
- one state after each meaningful feature chunk
- one milestone `save`
- one `nice` when a clearly good waypoint exists

## Recovery guidance

If the target state is unclear:

1. `jjk see`
2. `jjk graph`
3. `jjk search <query>`
4. `jjk inspect <state>`

Then act with:

- `jjk return <query>`
- or the relevant replay / revert command

# jjk usage modes

Use this reference when the user wants the agent to actively use `jjk`, not just explain it.

## Modes

There are three supported intent levels.

### 1. One-shot use

Use `jjk` only for the current requested task or turn.

Examples:

- “Use the jjk skill for this task”
- “Before changing this code, save a jjk state”
- “Return me to the previous jjk state and then make the fix”

Expected behavior:

1. Ensure the safe space exists.
2. Use `jjk` only for the requested operation.
3. Do not assume future turns should keep using `jjk`.

### 2. Conversation-scoped use

Use `jjk` on every relevant coding turn in the current conversation.

Examples:

- “Use jjk in this conversation”
- “For the rest of this thread, use jjk for development”
- “Keep saving jjk states while we work on this feature”

Expected behavior:

1. Treat `jjk` as the default development protocol for the rest of the conversation.
2. Save meaningful states before finishing coding turns that changed code.
3. Keep state cadence sensible; do not create noise.

### 3. Mandatory or always-on use

Use `jjk` on every relevant coding turn until the user turns it off.

Examples:

- “Always use jjk until I say stop”
- “Toggle mandatory jjk use on”
- “For all coding work, make jjk mandatory”

Expected behavior:

1. Treat `jjk` as required on future coding work.
2. Keep using it until the user clearly disables it.
3. If the environment cannot truly persist that preference beyond the current context, say so plainly and still honor it for the current session/thread.

## Agent cadence

When operating with `jjk`, agents should use a state cadence that is informative and not noisy.

### Minimum expectation

If an agent or subagent changed code during a turn, it should normally leave at least one `jjk` state before finishing.

### Preferred commands

- `jjk step <extensive factual message>`
  - use after a meaningful feature step or grouped code change
  - this is the default command for active implementation turns
- `jjk save <message>`
  - use when a coherent series of related steps is done
  - use for milestone checkpoints
- `jjk nice <message>`
  - use when the resulting state is clearly a good waypoint or approval candidate
- `jjk star <state>`
  - use to mark especially memorable states without creating a new state

### Message style

Messages should be factual, specific, and future-friendly.

Good:

- `extract parser validation and update callers`
- `wire branch-shaping commands and add focused tests`
- `stabilize replay timeout and validate merged command batch`

Avoid:

- `changes`
- `working on it`
- `stuff`

## Parallel work with subagents

When multiple agents will work concurrently:

1. Use a separate worktree per agent:
   - `jjk fork <agent-name> --worktree`
2. Keep each agent on its own branch/worktree.
3. Give each agent a clear owned scope.
4. Require each agent to leave `jjk` states in its own branch before handoff.
5. Merge or cherry-pick those branches only after focused validation.

## Practical protocol

For conversation-scoped or mandatory `jjk` usage:

1. `jjk init` if needed.
2. Before risky work, save or confirm there is a recent meaningful state.
3. During implementation:
   - `jjk step ...` after each meaningful feature chunk
4. At a milestone:
   - `jjk save ...`
5. If the state is especially good:
   - `jjk nice ...`
6. If the user asks to recover:
   - inspect with `jjk see`, `jjk graph`, `jjk search`, or `jjk inspect`
   - then use `jjk return <query>`

## Recovery guidance

If the target state is unclear:

1. `jjk see`
2. `jjk graph`
3. `jjk search <query>`
4. `jjk inspect <state>`

Only then act with:

- `jjk return <query>`
- `jjk replay ...`
- `jjk merge-state ...`
- `jjk revert-state ...`

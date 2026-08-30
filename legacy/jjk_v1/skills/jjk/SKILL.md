---
name: jjk
description: Explain, teach, and operate the `jjk` state-first workflow. Use when a user asks what `jjk` is, wants command help or examples, wants product or implementation details, wants an agent to use `jjk` once or continuously, wants parallel `jjk` worktrees for agents, or wants the current code and tests used as the source of truth for how `jjk` works.
---

# jjk

Use this skill to explain `jjk`, teach its usage, and operate it as an agent protocol.

`jjk` is a state-first layer over Git and optionally Jujutsu. The skill should help in four modes:

- explanation mode: what `jjk` is, why it exists, how commands fit together
- teaching mode: command examples, recommended practice, helper workflows, recovery patterns
- implementation mode: explain the current code, data model, and command wiring from source
- operation mode: actually use `jjk` in the current project when the user asked for that behavior

## Build Context First

Read only what is needed for the user request.

- For command and workflow questions:
  - [references/commands.md](./references/commands.md)
- For automatic or ongoing agent usage:
  - [references/automatic-usage.md](./references/automatic-usage.md)
  - [references/usage-modes.md](./references/usage-modes.md)
- For low-level implementation, debugging, or “how is this built?” questions:
  - [references/source-awareness.md](./references/source-awareness.md)

If the current workspace is the `jjk` implementation itself, prefer the live local source as the source of truth over any stale prose:

- `src/commands.ts`
- `src/store.ts`
- `src/git.ts`
- `src/render.ts`
- `src/types.ts`
- relevant tests in `tests/`

If a project-local launcher exists, prefer it first:

- `./bin/jjk`

Otherwise use the installed `jjk` only when appropriate.

## Explanation Rules

When explaining `jjk`:

- lead with the state model, not raw Git mechanics
- explain Git and Jujutsu as substrate layers
- distinguish clearly between:
  - implemented now
  - partial / heuristic behavior
  - planned ideas
- give examples, not just definitions
- if asked for low-level detail, ground the answer in the actual code and tests, not memory

When teaching:

- prefer state-first language such as:
  - “save this”
  - “return to that point”
  - “branch from here”
  - “cherry this state onto that branch”
- give recommended practice and anti-patterns
- show a basic example first, then the more advanced version

## Operation Rules

Use `jjk` operationally only at the scope the user asked for:

- one-shot:
  - use `jjk` only for the explicitly requested task or turn
- conversation-scoped:
  - use `jjk` for every relevant coding turn in the current conversation
- mandatory / always-on:
  - use `jjk` on every relevant coding turn until the user explicitly turns it off

Do not silently escalate from one-shot to conversation-wide or mandatory mode.

If the user asks for ongoing usage, follow the mode and cadence in [references/usage-modes.md](./references/usage-modes.md).

## Agent And Subagent Protocol

When the user wants agentic or parallel work with `jjk`:

1. Ensure the project is a safe space.
2. For parallel work, create isolated worktrees with:
   - `jjk fork <agent-name> --worktree`
3. Each agent or subagent should work in its own branch/worktree.
4. Before finishing a turn that changed code, each agent should record at least one meaningful `jjk` state.
5. Prefer:
   - `jjk step <extensive factual message>` for a meaningful feature step or grouped change
   - `jjk save <message>` when a milestone or coherent series of steps is complete
   - `jjk nice <message>` when a clearly good milestone has been reached
6. If the result should be memorable or protected, consider `star`, `pin`, or both.

Do not create a state for every tiny edit. Group related work into coherent steps.

## Safety Rules

- Use `jjk init` only when needed.
- Prefer semantic `jjk` commands over raw Git when the user asked to work in `jjk` terms.
- If the user asked only for explanation, do not run `jjk` commands unless they also asked for operation.
- When unsure which saved state they mean, use `jjk see`, `jjk graph`, `jjk search`, or `jjk inspect` before acting.
- For recovery, prefer `jjk return <query>` over raw Git commands.

## Notes

- `jjk` is a UX layer and a working protocol, not just a command list.
- The skill should help both humans and agents:
  - understand the model
  - use the commands well
  - explain the implementation correctly
  - operate safely and consistently

## References

- [references/commands.md](./references/commands.md)
- [references/automatic-usage.md](./references/automatic-usage.md)
- [references/usage-modes.md](./references/usage-modes.md)
- [references/source-awareness.md](./references/source-awareness.md)

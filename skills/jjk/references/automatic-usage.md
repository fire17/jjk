# automatic jjk usage

Use this protocol only when the user explicitly asks the agent to use `jjk`.

## Protocol

1. Run `jjk init` if the repo is not already a safe space.
2. Before risky work, save a state if there is no recent meaningful state.
3. Do the requested work.
4. Save a coherent grouped state before finishing.
5. If the result is approved, suggest or apply `jjk nice`.
6. If the user asks to revert, use `jjk return`.

## Description style

Descriptions should be short, factual, and future-friendly.

Good:

- `baseline before auth rewrite`
- `step extracted parser service`
- `nice green tests after config cleanup`

Avoid:

- `stuff`
- `changes`
- `working on it`

## Notes

- Prefer meaningful grouped states over spammy micro-saves.
- Treat `timeshift` as experimental repo-context restore, not a full terminal replay system.

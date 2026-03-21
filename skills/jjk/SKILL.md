---
name: jjk
description: Operate and explain the `jjk` state-first workflow in this repository, including safe spaces, states, lanes, return flows, freeze bundles, and experimental timeshift support.
---

# jjk

## Overview

Use this skill when the user asks to:

- use `jjk` commands in a repo
- explain what `jjk` is
- ask an agent to use `jjk` automatically
- recover or inspect prior states
- understand the product model in human language

## Workflow

### 1. Build context from this repo

- Read [references/commands.md](./references/commands.md).
- Read [references/automatic-usage.md](./references/automatic-usage.md) if the user wants automatic agent behavior.
- If the user wants product framing, read:
  - `../../README.md`
  - `../../docs/vision.md`
  - `../../docs/operating-model.md`
  - `../../marketing/hacker-news-post.md`

### 2. Operate `jjk` safely

- Prefer the project-local launcher first:
  - `./bin/jjk`
- Use `jjk init` to turn a directory into a safe space if it is not already initialized.
- Use state-first language in explanations:
  - save this
  - this is a step
  - this is a good place
  - return to the last good place

### 3. Keep the distinction clear

- Explain implemented commands as implemented.
- Explain product direction separately from shipped behavior.
- Be explicit that `timeshift` is currently experimental and not a full terminal restore system.

### 4. Automatic agent usage

Apply automatic behavior only when the user explicitly asks for the agent to use `jjk`.

When asked:

1. Ensure a safe space exists.
2. Save a fresh state before risky work if there is no recent meaningful state.
3. Do the work.
4. Save a coherent grouped state before finishing.
5. If approved, suggest or apply `jjk nice`.
6. If rejected, use `jjk return`.

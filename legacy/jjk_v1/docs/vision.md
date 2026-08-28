# Vision

## Core Claim

`jjk` is not commit management. It is stateful development.

The goal is a workflow where a project remembers meaningful moments, experiments branch harmlessly, and both humans and agents can move quickly without treating history surgery as a daily skill requirement.

## Safe Spaces

A directory becomes a safe space when:

- Git exists or is initialized.
- Jujutsu is initialized when available.
- `jjk` metadata exists under `.jjk/`.
- the project can remember states, lanes, timeshifts, and freezes with human-meaningful labels.

The first emotional promise of `jjk` is simple:

You should be able to try things without fear.

## States

States are the primary unit of memory.

Each state has:

- a short id
- a kind
- a label
- a description
- a timestamp
- a lane
- a branch
- a hidden Git snapshot commit

Kinds:

- `save`
- `step`
- `nice`
- `star`
- `auto`

This allows a project to distinguish routine progress, known-good places, and memorable anchors without forcing the user to think in commit-graph trivia.

## Humans And Agents

`jjk` should work for:

- a human working alone
- an agent working alone
- a human and an agent working side by side
- several agents exploring competing attempts

When an agent is asked to use `jjk`, the default behavior should be:

1. Ensure the project is a safe space.
2. Save a fresh state before risky work if there is no recent meaningful state.
3. Do the requested work.
4. Save a coherent grouped state before finishing.
5. If the result is approved, promote it with `nice`.
6. If the result is rejected, return to a prior good state.

## Lanes

The direction for lanes is:

- a lane is a named stream of work
- lanes can align with Git branches and worktrees
- a lane holds the narrative of an attempt
- returning to a state inside a lane should be trivial

The current repo implements named lanes on `jjk/lane/*` Git branches. The longer-term model is broader: lanes should also support multi-attempt work, side-by-side experiments, and promotion into canonical branches.

## Story, Freeze, Timeshift

Three higher-level ideas matter:

- `story`
  - highlight the memorable path instead of every movement
- `freeze`
  - export a state cleanly for review, backup, or handoff
- `timeshift`
  - eventually restore more than files: branch, lane, cwd, terminal context, and collaboration state

This repo implements story and freeze directly. It implements the first experimental layer of timeshift metadata rather than a full terminal restore system.

## What Comes Next

The broader planned surface includes:

- promotion flows for accepted attempts
- better merge/pick orchestration
- PR Radar for discovering candidate futures
- Feature Harvest for extracting only the winning parts of upstream work
- richer worktree and parallel-agent coordination
- deeper timeshift that spans repo state and terminal context together

The important boundary is that these ideas remain aligned with the central vocabulary:

- safe space
- state
- lane
- nice
- star
- return
- freeze
- timeshift

If the vocabulary drifts, the product stops feeling coherent.

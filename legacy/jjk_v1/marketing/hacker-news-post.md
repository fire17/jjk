# Show HN: jjk - Turn projects into safe spaces

I built `jjk` because normal source-control UX still asks both humans and coding agents to think too much about low-level history management.

The core idea is simple:

Turn a directory into a safe space.

Then let people work in terms of meaningful states:

- save this
- this is a small step
- this is a good place
- star this version
- return to the state before the experiment

Under the hood, `jjk` uses Git and optionally Jujutsu. But the user-facing model is higher-level:

- states with ids, labels, descriptions, timestamps, branches, and lanes
- hidden Git snapshot refs so saves do not have to clutter visible branch history
- fast return to prior states
- lane-oriented work for parallel experiments
- watch mode for grouped auto-saves
- freeze bundles for portable handoff

The target user is both:

- a human who wants safer experimentation
- an agent that should leave behind a clean, reviewable trail of meaningful progress

The repo also includes the product docs and a small site because I wanted the model to be explicit, not just a CLI trick.

Current scope:

- `jjk init`
- `jjk status`
- `jjk <description>`
- `jjk step`
- `jjk nice`
- `jjk star`
- `jjk see`
- `jjk story`
- `jjk diff`
- `jjk pick`
- `jjk promote`
- `jjk return`
- `jjk lane`
- `jjk map`
- `jjk watch`
- `jjk up`
- `jjk down`
- `jjk freeze`
- experimental `jjk timeshift`

Still ahead:

- deeper timeshift
- richer worktree orchestration
- promotion flows
- PR Radar / Feature Harvest style workflows

If this direction resonates, I would especially value feedback on:

1. whether hidden snapshot refs feel like the right default substrate
2. where the lane model should stop being “just Git branches with better semantics”
3. what the best agent protocol should be when automatically saving and returning states

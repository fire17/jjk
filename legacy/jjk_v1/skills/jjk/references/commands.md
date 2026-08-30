# jjk commands

This file is a compact reference for command families.

When the current workspace contains the live `jjk` implementation, treat the local source and tests as the source of truth because the command surface moves quickly.

## Core state operations

- `jjk <description>`
- `jjk save [description]`
- `jjk step [description]`
- `jjk nice [description]`
- `jjk amend [description]`
- `jjk star [state]`
- `jjk unstar [state]`
- `jjk pin <state>`
- `jjk unpin <state>`
- `jjk thumbsup [state]`
- `jjk thumbsdown [state]`
- `jjk note <state>, <message>`

## Inspection and navigation

- `jjk see`
- `jjk graph`
- `jjk log <branch>`
- `jjk inspect <state>`
- `jjk search <query>`
- `jjk timeline`
- `jjk favorites`
- `jjk current`
- `jjk where`
- `jjk heads`
- `jjk root <state>`
- `jjk trail <state>`
- `jjk children <state>`
- `jjk parents <state>`
- `jjk next`
- `jjk prev`
- `jjk continue`

## Branch and shaping operations

- `jjk branch [name]`
- `jjk checkout <branch>`
- `jjk fork <name> [--worktree]`
- `jjk worktree [state]`
- `jjk move <state> <branch>`
- `jjk branch-from <state> <label>`
- `jjk split <state> <new-branch>`
- `jjk rename-state <state> <new-label>`
- `jjk rename-branch <old> <new>`
- `jjk update <branch> [state]`

## Recovery and snapshot operations

- `jjk return <query>`
- `jjk back`
- `jjk forward`
- `jjk undo`
- `jjk redo`
- `jjk backup`
- `jjk backups`
- `jjk load <backup>`
- `jjk restore <backup> [--preview]`
- `jjk snapshot-log`

## Patch and replay operations

- `jjk show [state]`
- `jjk show --atomic-chain <state>`
- `jjk patch <state>`
- `jjk files <state>`
- `jjk touched <branch>`
- `jjk diff [--atomic] ...`
- `jjk pick <state>`
- `jjk replay <state> onto <branch>`
- `jjk merge-state <state> into <branch>`
- `jjk revert-state <state>`

## Utility, collaboration, and safety

- `jjk archive <state>`
- `jjk quarantine <state>`
- `jjk mark <state> <status>`
- `jjk assign-note <state>, <person/note>`
- `jjk ready <state>`
- `jjk publish <state>`
- `jjk handoff <state>`
- `jjk copy-id <query>`
- `jjk recent [limit]`
- `jjk aliases`
- `jjk aliases add <name> <query>`
- `jjk default-branch <branch>`
- `jjk config`
- `jjk open <state>`
- `jjk checkpoint [description]`
- `jjk autosave now`
- `jjk lock <branch>`
- `jjk unlock <branch>`

## Project and transport operations

- `jjk init`
- `jjk map`
- `jjk watch [description]`
- `jjk git log`
- `jjk push`
- `jjk pull`
- `jjk lane`
- `jjk doctor`
- `jjk freeze`
- `jjk snapshots`
- `jjk timeshift`

## State semantics

- `new`
  - start or branch a new line of work
- `save`
  - plain-language checkpoint or milestone
- `step`
  - meaningful feature step
- `nice`
  - clearly good waypoint
- `star`
  - memorable anchor marker
- `cherry`
  - replayed or merged state delta
- `stash`
  - parked workspace changes
- `auto`
  - grouped automatic checkpoint
- `git`
  - imported raw Git commit

## Important caveat

Some commands may still be partial or evolving.

When the user needs exact behavior, verify against:

- the local `src/commands.ts`
- the relevant store / git / render modules
- the focused tests that cover that command family

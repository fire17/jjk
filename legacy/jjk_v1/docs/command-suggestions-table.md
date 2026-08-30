# JJK Command Suggestions Table

This file captures every command idea suggested so far, including a short status and progress note.

Status values:

| Status | Meaning |
| --- | --- |
| `done` | Implemented in `jjk_v1` already |
| `partial` | Related behavior exists, but the exact suggested command/UX does not |
| `planned` | Strong next candidate, not started |
| `idea` | Good idea worth keeping, but not yet prioritized |

| Category | Suggestion | Status | Progress Notes |
| --- | --- | --- | --- |
| Core workflow | `jjk fork <label>` | `planned` | Strong next step for making branch creation explicit instead of inferred. |
| Core workflow | `jjk redo` | `done` | Implemented via full workspace snapshot history. Restores the next exact `jjk` + Git state. |
| Core workflow | `jjk amend` | `planned` | High-value follow-up. Should update the current saved state instead of creating a new one. |
| Core workflow | `jjk move <state> <branch>` | `planned` | Similar in spirit to `update`, but intended as an explicit metadata move for a state. |
| Core workflow | `jjk heads` | `planned` | Would be very useful now that branch history is getting deeper. |
| Core workflow | `jjk log <branch>` | `planned` | Natural complement to `jjk see` once one-branch inspection matters more. |
| Core workflow | `jjk checkout <branch>` | `idea` | Could make Git branch switching and `jjk` current-state selection happen together. |
| Core workflow | `jjk where` | `idea` | Lightweight “what branch/state am I on?” command. |
| Core workflow | `jjk root <state>` | `idea` | Useful for understanding where a branch line started. |
| Core workflow | `jjk trail <state>` | `idea` | Focused lineage view from root to one chosen state. |
| Core workflow | `jjk next` / `jjk prev` | `idea` | Simpler branch-tree navigation aliases beyond visited-history movement. |
| Core workflow | `jjk continue` | `idea` | Resume the latest state on the current branch when detached or on an older state. |
| Branch shaping | `jjk note <state>, <message>` | `planned` | Would add human context without changing Git history. |
| Branch shaping | `jjk pin <state>` / `jjk unpin <state>` | `planned` | Good protection feature for important states before more destructive tools are added. |
| Branch shaping | `jjk split <state> <new-branch>` | `planned` | Fits the current non-leaf return model very well. |
| Branch shaping | `jjk restack <branch>` | `planned` | Useful as a repair/reconciliation command for branch metadata. |
| Branch shaping | `jjk rename-branch <old> <new>` | `idea` | Would need coordinated Git branch rename plus metadata rewrite. |
| Branch shaping | `jjk rename-state <state> <new-label>` | `idea` | Straightforward metadata-only rename. |
| Branch shaping | `jjk branch-from <state> <label>` | `idea` | Explicit branch creation from any chosen historical state. |
| Branch shaping | `jjk squash <stateA> <stateB>` | `idea` | Probably metadata-heavy; best attempted after amend/fork/log are stable. |
| Branch shaping | `jjk transplant <state> <new-parent>` | `idea` | Advanced graph surgery feature. |
| Branch shaping | `jjk prune-branch <branch>` | `idea` | Likely a branch-level variant of hide/archive. |
| Inspection | `jjk doctor` | `planned` | High-value consistency checker for Git refs, current state, deleted references, and branch tips. |
| Inspection | `jjk inspect <state>` | `planned` | Good candidate now that state metadata is richer. |
| Inspection | `jjk children <state>` | `idea` | Simple structural query, useful once graphs get larger. |
| Inspection | `jjk parents <state>` | `idea` | Complements `children` and `trail`. |
| Inspection | `jjk search <query>` | `idea` | Broader query interface across labels, ids, notes, tags, branches, and messages. |
| Inspection | `jjk timeline` | `idea` | Time-first view separate from the graph shape. |
| Inspection | `jjk graph --branch <branch>` | `partial` | `jjk see` exists, but branch-focused graph output does not yet. |
| Inspection | `jjk see --kind <kind>` | `planned` | Good low-risk filtering feature for crowded repos. |
| Inspection | `jjk see --tag <tag>` | `idea` | More useful now that explicit tags like `star` and `stash` exist. |
| Inspection | `jjk see --since <time>` | `idea` | Time-window filtering for recent work only. |
| Inspection | `jjk show --atomic-chain <state>` | `partial` | `jjk show` and atomic diff exist, but not the full atomic chain view. |
| Inspection | `jjk compare-branch <a> <b>` | `planned` | Strong candidate once `heads` exists. |
| Patch/change tools | `jjk patch <state>` | `idea` | Human-friendly patch summary on top of existing diff/show primitives. |
| Patch/change tools | `jjk files <state>` | `idea` | Focused file list view per state. |
| Patch/change tools | `jjk touched <branch>` | `idea` | Branch-level changed-file aggregation. |
| Patch/change tools | `jjk blame-state <file>` | `idea` | Could be very powerful, but likely more involved than it first looks. |
| Patch/change tools | `jjk replay <state> onto <branch>` | `idea` | Broader state replay idea beyond one cherry-picked delta. |
| Patch/change tools | `jjk merge-state <state> into <branch>` | `partial` | `jjk pick` already covers much of this, but the user-facing model could be clearer. |
| Patch/change tools | `jjk revert-state <state>` | `idea` | Symmetric inverse operation for a state’s patch. |
| Safety/recovery | `jjk archive <state>` | `partial` | `delete`/`recover` already behave like a hidden archive, but the UX is not branded that way. |
| Safety/recovery | `jjk quarantine <state>` | `idea` | Useful for flagging broken/problematic states without deleting them. |
| Safety/recovery | `jjk lock <branch>` / `jjk unlock <branch>` | `idea` | Prevent accidental writes on sensitive branches. |
| Safety/recovery | `jjk checkpoint` | `idea` | Would save a recoverable workspace snapshot without creating a normal visible state. |
| Safety/recovery | `jjk autosave now` | `idea` | Explicit on-demand safety snapshot. |
| Safety/recovery | `jjk clean` | `idea` | Light cleanup for stale metadata and refs. |
| Safety/recovery | `jjk gc` | `idea` | Heavier cleanup once archive/delete/history get more complex. |
| Backup/snapshot | `jjk backups` | `planned` | Natural follow-up now that `backup` and `load` exist. |
| Backup/snapshot | `jjk backup --latest` | `partial` | `jjk backup` already creates a timestamped default backup when no name/path is supplied. |
| Backup/snapshot | `jjk restore <backup> --preview` | `partial` | `jjk load` exists, but no preview/dry-run mode yet. |
| Backup/snapshot | `jjk snapshot-log` | `planned` | Would expose the new undo/redo workspace snapshot history. |
| Backup/snapshot | `jjk export <state> <file>` | `idea` | Portable export for one state or state subtree. |
| Backup/snapshot | `jjk import <file>` | `idea` | Companion to export. |
| Collaboration | `jjk mark <state> <status>` | `idea` | Could add lightweight workflow states like review/approved/blocked/wip. |
| Collaboration | `jjk assign-note <state>, <person/note>` | `idea` | Human coordination layer without changing commits. |
| Collaboration | `jjk ready <state>` | `idea` | Could be a more explicit review handoff marker than `nice`. |
| Collaboration | `jjk publish <state>` | `idea` | State-aware push/publication command. |
| Collaboration | `jjk handoff <state>` | `idea` | Generate a human/agent summary for a chosen state. |
| Usability | `jjk open <state>` | `idea` | Open the changed files from a state in the editor. |
| Usability | `jjk copy-id <query>` | `idea` | Resolve a fuzzy query and print only the chosen id. |
| Usability | `jjk recent` | `idea` | Quick view of recently visited or recently saved states. |
| Usability | `jjk favorites` | `partial` | Starred states exist, but there is no favorites-only view yet. |
| Usability | `jjk aliases` | `planned` | Good quality-of-life feature for frequent state names. |
| Usability | `jjk aliases add <name> <query>` | `idea` | Likely part of the broader aliases command family. |
| Usability | `jjk default-branch <branch>` | `idea` | Useful once more commands need a sensible branch default. |
| Usability | `jjk config` | `idea` | Unified settings view/edit command for `jjk`. |

# Product

## Register

product

## Users

Software developers, maintainers, and coding agents working alone or concurrently in real Git repositories. They need to try, compare, reject, combine, recover, and hand off work without losing files, corrupting history, or translating every intention into low-level version-control surgery.

## Product Purpose

JJK turns a directory into a safe space: a trustworthy working memory above Git and optionally Jujutsu. Success means users attempt more ideas with less fear, always know where they are, preserve sibling futures, take only the intended delta from another attempt, and leave a repository that remains valid and understandable to people and tools that have never installed JJK.

## Brand Personality

Calm, plainspoken, and exact. JJK should feel protective without being paternalistic, powerful without ceremony, and fast enough to disappear into the act of development.

## Anti-references

- Git porcelain copied under new names without a higher-level semantic purpose.
- A giant flat command catalog that exposes every advanced operation to beginners.
- Decorative graph output that hides identity, provenance, incompleteness, or recovery state.
- “Magic” automation that silently chooses an ambiguous target, rewrites canonical history, stages secrets, deletes worktrees, or claims to change a parent shell's directory.
- A Git replacement that traps repositories behind proprietary metadata or requires collaborators and CI to install JJK.
- Agent dashboards that optimize activity counts instead of preserving understandable, validated work.

## Design Principles

1. **Meaning above mechanics.** States, attempts, return paths, and exact composition are the user language; Git objects remain inspectable substrate.
2. **Safety creates freedom.** Every mutation is journaled, previewable when risky, recoverable, and explicit about indeterminate outcomes.
3. **Truth remains universal.** Native Git workflows continue to work; missing optional capabilities degrade explicitly rather than being simulated.
4. **Progressive power.** Orientation and six human verbs come first; graph surgery, collaboration, ecosystem harvesting, and Timeshift appear only at the altitude where they help.
5. **One graph, every surface.** CLI, structured output, TUI, GUI, IDE, and agents consume the same identities, topology, actions, and evidence.

## Accessibility & Inclusion

Target WCAG 2.2 AA where graphical surfaces exist. Terminal and machine interfaces must not encode state by color alone; honor `NO_COLOR`; remain legible at narrow widths and under common color-vision deficiencies; provide deterministic non-TTY and JSON output; keep keyboard operation complete; reduce or remove nonessential motion; use plain error language with an exact recovery action.

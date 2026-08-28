# JJK Design System

## Intent

A calm, dense developer tool whose visual language makes topology and safety obvious. Familiar terminal conventions first; delight comes from instant orientation, not decoration.

## Foundations

### Typography

- Terminal: inherit the user's monospace font; never depend on glyphs absent from common Unicode-capable terminals.
- Graphical adapters: system sans for controls and explanations; system monospace for IDs, refs, commands, patches, and graph rows.
- Stable fixed size hierarchy. Do not use fluid display typography in task surfaces.

### Color

Color is a redundant branch/attempt cue, never the only carrier of meaning.

- Current state: bold plus `*`; optional high-contrast accent.
- Attempt/branch identity: deterministic palette derived from stable attempt ID.
- Canonical/trusted: distinct symbol and restrained success color.
- Archived: dim plus explicit archived label.
- Warning/recovery required: symbol, wording, and amber/yellow where supported.
- Failure: symbol, wording, and red where supported.
- `NO_COLOR`, non-TTY, and JSON remove ANSI without losing any state distinction.
- ANSI palette choices must remain distinguishable under protanopia, deuteranopia, and tritanopia simulations; use line/symbol variation when colors converge.

### Spacing and density

- Terminal rows are one semantic unit per line wherever possible.
- Tables use stable columns only when width permits; collapse into labeled detail rows rather than truncating decisive fields.
- Blank lines separate conceptual groups, not every row.
- IDs default to the shortest unambiguous prefix; full IDs remain available in detail/JSON.

## Core components

### Orientation strip

The first block of `status` and TUI views:

`safe-space · workspace · attempt · state · Git branch/HEAD · clean/dirty · recovery`

It must answer “where am I and is my work safe?” before secondary counts.

### State row

Required semantic slots:

`current marker · topology glyph · trust/tag marker · state ID · kind · label · attempt/branch · validation · age/stats`

Long messages become a following detail line or an explicit expanded view; never break topology alignment unpredictably.

### Graph

- Roots lower/earlier and live leaves later/upward according to the surface's orientation convention; state the convention.
- Logical-parent edges are primary. Composition/provenance edges use distinct glyph/style and never masquerade as ancestry.
- Current, leaf, canonical, composed, archived, and incomplete/filter states have redundant symbols and labels.
- Layout is deterministic for the same graph and viewport.
- 40 columns: focused lineage plus “N hidden; use …”.
- 80 columns: normal graph and concise metadata.
- 120+ columns: graph plus evidence/stats columns.

### Choice list

Ambiguous fuzzy matches show stable numbering, label, kind, attempt, relative time, changed-file count, and confidence reason. Keyboard selection is complete; non-interactive mode returns a typed ambiguity error and candidate JSON rather than guessing.

### Operation plan

Risky actions render:

`intent → target → affected refs/files/workspaces → validations → recovery point → exact command to continue/abort`

Default answer is no when confirmation is required. `--json` exposes the same plan before execution.

### Result and recovery card

Every mutation ends with the new state/attempt, exact underlying Git/JJ effects, evidence, and one-line return/recovery command. Failure output begins with what happened, what remains safe, and the exact next action.

## Interaction

- Fast paths print useful feedback before 100 ms where contracted.
- No spinner for operations expected below 200 ms.
- Longer work prints phase transitions only when a TTY is present; JSON emits structured progress events if requested.
- Motion in graphical adapters is optional, under 100 ms for rung transitions where achievable, disabled with reduced motion, and never delays action.
- Destructive previews remain inline or in a dedicated plan view; modals are not the default.

## Structured surface

Every command supports a versioned JSON envelope where automation is meaningful:

```json
{
  "schema": "jjk.cli/v1",
  "command": "status",
  "ok": true,
  "data": {},
  "warnings": [],
  "recovery": null
}
```

Human and JSON surfaces are renderers over the same response types. No scraping human text for agent operation.

## Voice

Short factual sentences. Use JJK's user vocabulary: state, attempt, return, pick, safe space, recovery. Name Git/JJ mechanics only when the user asks for detail or the mechanics affect safety. Never say “success” without the exact resulting state; never say “nothing was lost” unless verified.

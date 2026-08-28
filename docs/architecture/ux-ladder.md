# UX Ladder Architecture

**Status:** decision-grade design for the JJK v0.1 rewrite  
**Scope:** CLI, TUI, library/API projections, action dispatch, accessible terminal rendering, and deterministic graph rendering  
**Authority:** `../../VISION.md`, then `../../../wholesomegarden/Codex/jjk_v1/vision_overhaul.md`, then observed prototype source/tests  
**Verification status:** architecture only; runtime checks are **NOT yet live-verified**.

## 1. Context

JJK is the semantic state layer above Git and, when available, Jujutsu. Git remains the universal durable substrate. JJ is an optional local-history/recovery accelerator. JJK owns user-facing meaning: states, attempts, curation, provenance, composition, collaboration, and reversible movement through development situations.

The interface must reconcile two needs: a new user should live inside six plainspoken verbs, while an expert, maintainer, or agent must inspect and act on exploration graphs, collaboration state, remote ecosystems, functional transformations, and whole development situations. Flattening these into one help screen buries “turn a directory into a safe space”; implementing separate models creates two truths. The decision is one seven-rung ladder over one graph/action model.

A rung is not a permission tier, edition, maturity label, or a smaller rendering of the previous rung. It introduces objects suited to one evidenced question and explicitly drops irrelevant information. Higher is not better. Movement preserves focus, and every omitted fact remains reachable.

### 1.1 Evidence ledger

| ID | Observed requirement | Locator |
|---|---|---|
| `E-UX-001` | A JJK state is a semantic primitive distinct from Git/JJ identities. | `vision_overhaul.md:80-106,400-411` |
| `E-UX-002` | The experience must be ambiently safe, plainspoken, visible, calm, reversible, and progressively disclosed. | `vision_overhaul.md:53-75` |
| `E-UX-003` | The graph is the primary explanation surface; current, tips, trust, topology, identities, provenance, dirty work, and incompleteness must be visible. | `vision_overhaul.md:175-194` |
| `E-UX-004` | Navigation follows memory, ancestry, and intent; ambiguous matches must be shown rather than silently chosen. | `vision_overhaul.md:196-209` |
| `E-UX-005` | The proposed ladder is ambient → six verbs → exploration → collaboration → ecosystem → transformation → situation control. | `vision_overhaul.md:443-453` |
| `E-UX-006` | `current`/`status` target <50 ms warm; graph first paint targets <100 ms for 1,000 states. | `vision_overhaul.md:510-519` |
| `E-UX-007` | Concurrent people/agents require attempts, worktrees, ownership, factual handoffs, validation, and canonical promotion. | `vision_overhaul.md:138-173,223-248` |
| `E-UX-008` | PR Radar, Feature Harvest, and fork projections make external futures locally explorable. | `vision_overhaul.md:250-274` |
| `E-UX-009` | Functional history is a provenance-preserving derived view; AI may propose, while materialization remains deterministic and auditable. | `vision_overhaul.md:263-274` |
| `E-UX-010` | Timeshift is componentized and must report adapter limits and secret exclusions honestly. | `vision_overhaul.md:276-289` |
| `E-UX-011` | Prototype evidence covers current/tip/star markers, stable branch colors, single-line truncation, width-bounded tables, and TTY-sensitive color. | `src/render.ts:16-22,61-79`; `tests/render.test.ts:76-82,156-256,663-829,1182-1313` |
| `E-UX-012` | Existing `current` and `status` answer distinct orientation questions. | `src/commands.ts:1182-1224` |
| `E-UX-013` | The canonical request requires branching, graph connections, special operations, and execution-ready architecture. | `/Users/magic/Creations/JJK/VISION.md:1` |

Prototype references are evidence of learned behavior, not a commitment to its TypeScript types or renderer.

## 2. Decisions

| ID | Decision and consequence |
|---|---|
| `UX-DEC-001` | Seven stable levels `0..6` derive from seven evidenced task families. New levels require a new question and schema-versioned addition; existing IDs never renumber. |
| `UX-DEC-002` | One semantic graph and one action catalog serve CLI, TUI, and API. Renderers may differ; topology, availability, risk, selection, and mutation semantics may not. |
| `UX-DEC-003` | `status` answers repository/workspace safety; `current` answers semantic location. They share level 0 but remain distinct fast commands. |
| `UX-DEC-004` | Beginner verbs are exactly `init`, `save`, `nice`, `see`, `return`, `pick`. `jjk <free-form text>` is parser sugar for `save`, not a seventh action. |
| `UX-DEC-005` | Every view carries available actions from the shared catalog, including level 0. TUI/API dispatch the same `ActionId` as CLI. |
| `UX-DEC-006` | Transitions carry focus by stable entity ID, never row number, screen coordinate, color, or fuzzy label. |
| `UX-DEC-007` | Omission is explicit data. Every projection carries hidden counts/categories/reasons and expansion links. |
| `UX-DEC-008` | Every mutation uses the cross-layer protocol and one plan/apply/undo contract. CLI, TUI, and API preview the same plan digest and resolved members. |
| `UX-DEC-009` | Terminal output is width-aware, color-optional, keyboard-complete, and legible without ANSI, Unicode line art, animation, mouse, or hyperlinks. |
| `UX-DEC-010` | Structured output is renderer-independent and schema-versioned. Width, color, locale, and TTY state cannot alter its meaning. |
| `UX-DEC-011` | Graph ordering/lane allocation are deterministic functions of the snapshot, never map iteration, arrival timing, width, or color. |
| `UX-DEC-012` | Help/action records classify commands as JJK-native, Git-enhanced, or transparent Git passthrough. |
| `UX-DEC-013` | The UX contract is storage-neutral. SQLite WAL journal plus materialized projections is accepted as the current default only if it provides snapshot-consistent revisions, atomic results, and measured budgets. |
| `UX-DEC-014` | AI-derived ecosystem/transformation values are sourced proposals with confidence, never graph truth or auto-applied actions. |

## 3. Invariants

- **`UX-INV-001 Same truth:`** equal `(repo fingerprint, graph revision, level, focus, filters, capabilities)` yields semantically equal CLI/TUI/API projections.
- **`UX-INV-002 Typed identity:`** Git OID, optional JJ IDs, state, attempt, actor, candidate, situation, and operation IDs are distinct; labels are not identities.
- **`UX-INV-003 Focus carry:`** transition returns `exact`, `contained_by`, `primary_member`, or `unavailable(reason)`; valid focus is never silently dropped.
- **`UX-INV-004 Honest omission:`** filters, folds, pages, archival exclusion, adapter absence, stale remote data, and redaction are represented in `omitted` and the terminal header.
- **`UX-INV-005 Action parity:`** the same `ActionId` at the same revision resolves identical targets, risk, preview, and undo semantics on every surface.
- **`UX-INV-006 Selection first:`** batch actions resolve to an exact count and stable ID list, printed before confirmation.
- **`UX-INV-007 Reversible sky actions:`** aggregate/multi-member actions require preview, explicit confirmation, and operation undo or a declared irreversible boundary.
- **`UX-INV-008 Git honesty:`** JJK never invents Git state, hides divergence, or reports success before refs/workspace and journal agree.
- **`UX-INV-009 No color-only meaning:`** current, tip, trusted, dirty, warning, edge kind, and selection have text/symbol markers.
- **`UX-INV-010 No pipeline interaction:`** non-TTY never opens a TUI/pager/prompt or silently resolves ambiguity.
- **`UX-INV-011 Clean streams:`** JSON success is one stdout document; JSON failure is one stderr error envelope with stdout empty.
- **`UX-INV-012 Determinism:`** canonical JSON order, topology order, selection membership, and plan digest are locale-, time-, and hash-seed-independent.
- **`UX-INV-013 Safe labels:`** labels/messages cannot inject controls, rows, bidi spoofing, hyperlinks, or JSON structure.
- **`UX-INV-014 Capability honesty:`** missing JJ/forge/shell/editor/terminal/agent adapters disable actions with reasons; they never simulate success.
- **`UX-INV-015 Source reachability:`** every derived field has source references/confidence; every aggregate expands to exact members at the same revision.
- **`UX-INV-016 Git passthrough fidelity:`** transparent passthrough preserves argv `OsString`s/bytes, cwd, inherited env, stdio, signals, and exit code.

## 4. Shared graph/action model

The core returns typed snapshots. Renderers never query Git, JJ, SQLite, a forge, or files directly.

```rust
struct ViewRequest {
    repo: RepoId,
    level: LevelId,
    focus: Option<EntityId>,
    filters: Vec<FilterExpr>,
    include: IncludePolicy,
    page: Option<PageCursor>,
    page_budget_bytes: u32,
    expected_revision: Option<GraphRevision>,
}

struct ViewSnapshot {
    schema: SchemaVersion,
    repo: RepoIdentity,
    graph_revision: GraphRevision,
    generated_at: Timestamp,
    level: LevelDescriptor,
    focus: FocusResolution,
    nodes: Vec<ViewNode>,
    edges: Vec<ViewEdge>,
    actions: Vec<ActionDescriptor>,
    omitted: OmissionSummary,
    capabilities: CapabilitySummary,
    warnings: Vec<Diagnostic>,
    next_page: Option<PageCursor>,
}

struct ViewNode {
    id: EntityId,
    kind: ObjectKind,
    label: String,
    markers: Vec<SemanticMarker>,
    fields: BTreeMap<FieldKey, FieldValue>,
    members: Vec<EntityId>,
    sources: Vec<SourceRef>,
    confidence: Confidence,
    available_actions: Vec<ActionId>,
}

struct ViewEdge { id: EdgeId, kind: EdgeKind, from: EntityId, to: EntityId, sources: Vec<SourceRef> }
enum EdgeKind {
    LogicalParent, AttemptContains, BranchProjects, WorktreeHosts,
    DeltaDerivedFrom, CompositionUses, Validates, Promotes, OwnedBy,
    HandsOffTo, MirrorsUpstream, Harvests, GroupsHunk, Materializes,
    CapturesComponent, RestoresFrom,
}

struct OmissionSummary {
    incomplete: bool,
    hidden_counts: BTreeMap<OmittedCategory, u64>,
    reasons: Vec<OmissionReason>,
    expand: Vec<ExpandLink>,
}
```

Aggregate `members` are exact at the snapshot revision. Wire encoding may cursor large member lists during reading, but a mutation plan materializes and prints all selected IDs.

```rust
enum CommandClass { JjkNative, GitEnhanced, TransparentGitPassthrough }
struct ActionDescriptor {
    id: ActionId, name: String, class: CommandClass,
    scopes: Vec<ActionScope>, input_schema: SchemaRef,
    enabled: bool, disabled_reason: Option<Diagnostic>, risk: RiskClass,
    preview: PreviewPolicy, confirmation: ConfirmationPolicy, undo: UndoPolicy,
}
struct ActionRequest {
    repo: RepoId, action: ActionId, selection: SelectionExpr, args: Value,
    expected_revision: GraphRevision, idempotency_key: IdempotencyKey,
}
struct ActionPlan {
    operation: OperationId, action: ActionId, based_on_revision: GraphRevision,
    resolved: Vec<EntityId>, resolved_count: u64,
    preconditions: Vec<Precondition>, effects: Vec<PlannedEffect>,
    diff: PlanDiff, risk: RiskClass, recovery_boundary: RecoveryBoundary,
    undo: UndoPlan, plan_digest: Digest,
}
struct ActionResult {
    operation: OperationId, disposition: OperationDisposition,
    before_revision: GraphRevision, after_revision: GraphRevision,
    verified_effects: Vec<VerifiedEffect>, recovery: Option<RecoveryInstruction>,
    next_focus: FocusResolution,
}
```

Every mutation follows:

> `discover → lock → reconcile → resolve → plan → durable prepare → mutate Git/JJ/files → append events+projections → verify → commit/repair`

`commit/repair` is transaction disposition, not necessarily a Git commit. Changed revision, capabilities, membership, or preconditions invalidate the plan; apply never silently replans.

### 4.1 Command classes

- **JJK-native:** `status`, `current`, six beginner verbs, attempts, story, handoff, promotion policy, harvesting, transformations, Timeshift.
- **Git-enhanced:** JJK plans semantic/evidence work around explicit Git effects, such as refreshing submission projections or promoting a verified state to a canonical ref. Plans print exact Git-facing effects.
- **Transparent Git passthrough:** `jjk git -- <argv...>` forwards platform-native argv, cwd/env/stdio/signals/exit code; it does not lock, rewrite flags, prompt, journal semantic success, or print JJK output. Unix should prefer `exec`. External changes reconcile on the next JJK command. Any command that parses/decorates Git is Git-enhanced, not transparent.

## 5. Rung specifications

All rungs expose “what is not shown” through CLI `jjk see --omitted`, TUI `o`, and API `omitted`.

### Level 0 — Ambient orientation (`ambient`)

- **User question:** “Am I safe to continue here, and what exactly is current?”
- **Objects/new information:** `SafeSpaceOrientation`, `WorkspaceHealth`, `CurrentPointer`, `AttemptBadge`, `CapabilitySummary`; safety state (`safe|dirty|diverged|recovering|unknown`), exact current state/attempt, branch/worktree binding, and degraded capabilities.
- **Actions:** reads `status` and `current`; shared catalog offers `save`, resume via `return`, and previewed `doctor repair`. These dispatch canonical action IDs.
- **Context carried:** repo/fingerprint/revision, cwd, Git HEAD/branch, current state/attempt/worktree IDs, dirty/index/untracked summary, last durable operation, adapter availability.
- **Information dropped:** ancestry, sibling attempts, files/hunks, old messages, ownership/validation, external candidates, functional groups, situation components; counts/drills remain in `omitted`.
- **Transition:** `see`/level 1 expands `CurrentPointer` to exact `StateId`; lateral movement is `current` ↔ `status`. No lower rung.
- **Evidence:** `E-UX-002`, `E-UX-006`, `E-UX-012` prove distinct orientation/safety tasks and latency needs.
- **Failure:** Git/JJK disagreement renders `[unknown: reconciliation required]`, disables capture/return, and exposes repair; display never chooses a convenient truth.

### Level 1 — Six-verb remembered work (`beginner`)

- **User question:** “What did I mean to preserve, where should I return, and which one idea should I carry forward?”
- **Objects/new information:** `SemanticState`, `MemorableWaypoint`, `StateChoice`, `AtomicDelta`, `RememberedPath`; saved meaning, known-good waypoint, remembered target, and a state’s parent-to-state delta.
- **Actions:** exactly `init`, `save`, `nice`, `see`, `return`, `pick`. Free-form `jjk <text>` is exact `save` sugar. `pick` applies only the source state’s logical-parent delta and records base/source/patch/conflict provenance.
- **Context carried:** level-0 facts plus state ID, label, kind, logical parent, Git OID, attempt membership, curation/trust markers, evidence status, dirty-work pseudo-node.
- **Information dropped:** raw patches/file lists, visited-history mechanics, full worktree topology, actor assignment, policy details, ecosystem, transformation, Timeshift components.
- **Transition:** ascent maps state to containing attempt; descent maps attempt to last-focused state or deterministic tip. `show` drills to content at level 2. Default help shows six verbs and one adjacent-level hint.
- **Evidence:** `E-UX-001..005`, especially the explicit six-verb ladder and `save → see → return → pick` demonstration.
- **Failure:** ambiguous fuzzy matches show aligned choices with ID/attempt/date/stats; non-TTY/API returns typed `ambiguous_target` candidates.

### Level 2 — Exploration map (`exploration`)

- **User question:** “Which attempt should I inspect, compare, or safely continue?”
- **Objects/new information:** `Attempt`, `WorktreeBinding`, `BranchBinding`, `StatePath`, `Comparison`, `StoryEntry`, `NavigationTrail`; competing futures, implementation bindings, atomic/full comparison, curated story vs visit history.
- **Actions:** `fork`, `worktree`, `show`, `diff`, `story`, `back`, `forward`, `up`, `down`; continue/archive attempts; compare selected tips. Fork/worktree/archive use plan/apply/undo.
- **Context carried:** state focus, attempt containment, logical-parent edges, branch/worktree bindings, tips/archive state, delta/file summaries, navigation/story membership.
- **Information dropped:** per-hunk content until drill, detailed handoffs/policies, unimported remote candidates, functional classifications, non-repository situation components.
- **Transition:** state ↔ containing attempt with last-focus restoration; ascent maps attempt to assignment/handoff/promotion object; peer movement switches attempts without changing level.
- **Evidence:** `E-UX-003`, `E-UX-004`, `E-UX-005`, `E-UX-007` prove attempts differ from branches/worktrees and require exploration/navigation actions.
- **Failure:** missing branch/worktree binding is a visible broken-binding node with repair action, never silently removed.

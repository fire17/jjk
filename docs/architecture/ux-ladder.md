# UX Ladder Architecture

**Status:** decision-grade design for the JJK v0.1 rewrite  
**Scope:** CLI, TUI, library/API projections, action dispatch, accessible terminal rendering, and deterministic graph rendering  
**Authority:** `/Users/magic/Creations/JJK/VISION.md`, then `/Users/magic/wholesomegarden/Codex/jjk_v1/vision_overhaul.md`, then observed prototype source/tests  
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

### Level 3 — Collaboration control (`collaboration`)

- **User question:** “Who owns each attempt, what evidence says it is ready, and what may become canonical?”
- **Objects/new information:** `Actor`, `Assignment`, `TypedHandoff`, `ValidationRecord`, `PromotionCandidate`, `CanonicalProjection`, `PolicyDecision`, `RemoteSyncState`; ownership, objective, evidence, risk, resume/reject instruction, policy gate, previous canonical tip, rollback path.
- **Actions:** claim/release attempt; write/accept handoff; request/record validation; compare candidates; dry-run/promote/rollback canonical projection; sync transport metadata. Promotion is selection → evidence/policy preview → confirm → atomic refs/events → undo.
- **Context carried:** attempt/state focus, tips/bindings, source attempt/state IDs, actor, validation provenance, canonical ref, upstream relation, revision.
- **Information dropped:** routine intermediate states, raw hunks, shell visit history, unselected remote catalog, functional proposals, editor/terminal capture.
- **Transition:** attempt maps to assignment/handoff/promotion candidate; state maps to nearest containing candidate or explicit `no collaboration object`. Ascent keeps canonical baseline focused for external comparison; descent restores attempt.
- **Evidence:** `E-UX-005`, `E-UX-007`, plus `vision_overhaul.md:160-173,223-248,554-559` explicitly require typed handoffs, evidence, ownership, promotion, and rollback.
- **Failure:** stale/missing validation disables promotion with exact missing gates. Concurrent claims show both and require reconciliation; never last-writer-wins.

### Level 4 — Ecosystem opportunity map (`ecosystem`)

- **User question:** “Which external future is valuable and safe to try locally against current upstream?”
- **Objects/new information:** `ExternalCandidate`, `UpstreamBaseline`, `SubmissionProjection`, `CompatibilityAssessment`, `RadarSignal`, `HarvestRun`, `SandboxResult`; candidate freshness/reviews/areas, isolated import, upstream replay, selectively harvested value.
- **Actions:** refresh PR Radar; filter/select candidates; import into isolated attempts/worktrees; sandbox validation; refresh submission projection; preview/harvest deltas. Batch actions print all resolved IDs/count and obey sandbox policy.
- **Context carried:** canonical baseline/policy, forge/repo identity, remote refs/OIDs, linked local attempt, validation, observation timestamp, source URLs.
- **Information dropped:** exhaustive local chronology, unrelated handoff discussion, raw remote discussions, raw hunks until drill, functional proposals, Timeshift state.
- **Transition:** canonical projection maps to candidates relative to it; imported candidate descends to local attempt. Ascent maps selected candidate deltas to functional units; unanalyzed candidates remain explicit.
- **Evidence:** `E-UX-005`, `E-UX-008` explicitly name PR Radar, Feature Harvest, fresh-upstream projection, sandboxing, and selective composition.
- **Failure:** offline/rate-limited/stale data shows `observed_at`, adapter error, staleness, and sources. Cached reading remains; freshness-dependent actions disable.

### Level 5 — Functional transformation (`transformation`)

- **User question:** “How should mixed history be re-expressed by function, and which valid composition should I materialize?”
- **Objects/new information:** `FunctionalUnit`, `HunkProvenance`, `FunctionalProjection`, `CompositionProposal`, `CompositionAlternative`, `MaterializationPlan`, `HumanCorrection`; behavioral units, exact source hunks, alternative syntheses, deterministic materialization.
- **Actions:** propose grouping; inspect sources; correct/split/join units; create/compare/validate alternative compositions; select by expression; dry-run/materialize; undo materialization while retaining source archive/proposals.
- **Context carried:** source state/delta/candidate IDs, parent/base, patch identity, validation, target, actor, classifier/version/confidence, immutable hunk locators.
- **Information dropped:** chronology as primary layout, routine worktree mechanics, remote discussion noise, unselected file content, ambient details, shell/editor/agent situation. Provenance still reaches chronology.
- **Transition:** delta/candidate maps to functional units; unit descends to exact hunks/states. Materialized state ascends to level-6 repository component; an unmaterialized proposal cannot masquerade as restorable state.
- **Evidence:** `E-UX-005`, `E-UX-009` require immutable provenance, correction, preview, deterministic materialization, and preservation of original history.
- **Failure:** low-confidence, overlapping, or untraceable groupings cannot materialize. Preserve alternatives instead of forcing one “best” merge.

### Level 6 — Timeshift situation control (`timeshift`)

- **User question:** “Which development situation should I restore, which components are safe to apply, and what cannot be recreated?”
- **Objects/new information:** `SituationSnapshot`, `ComponentCapture`, `AdapterCapability`, `RestoreSelection`, `RestorePlan`, `RestoreGap`, `SecretExclusion`, `AgentContextPointer`; component availability, portability, redaction, partial restore, honest gaps.
- **Actions:** capture situation; select components; preview restore; restore exact set; retry failed components; undo the whole boundary or safe subset. The top-rung batch action prints situation/component IDs, count, unavailable components, effects, and digest before confirmation.
- **Context carried:** repository/state/attempt, component versions/sources, target host capabilities, allowlisted env keys, redaction proof, recovery boundary.
- **Information dropped:** individual graph rows/hunks, candidate ranks, model reasoning, secrets, arbitrary env, unallowed raw shell history, process memory, unsupported internals. `not captured`, `redacted`, `unsupported`, and `failed` remain distinct.
- **Transition:** descent focuses repository state/attempt/materialization; non-JJK component returns `unavailable` rather than teleporting. Peer movement switches situations.
- **Evidence:** `E-UX-005`, `E-UX-010` establish componentized, adapter-specific, partial, secret-safe, honest Timeshift.
- **Failure:** preview separates applicable/unavailable/redacted/stale/conflicting components. Apply only the confirmed subset; mixed success enters repair with a durable recovery point.

## 6. Transition glue

### 6.1 Controls and focus

- **CLI:** global `--level 0..6`; `see` defaults 1, `status/current` 0. `help` shows level 1 and adjacent-level hints; `help --level N` shows only that rung. `-v` expands fields within a rung and never changes object vocabulary.
- **TUI:** `[` down, `]` up, `g` numbered chooser, `o` omissions, `Enter` drill, `Tab`/`Shift-Tab` peer navigation. Breadcrumb: `JJK › <number>: <name> › <focus>`.
- **API:** `level` in `ViewRequest`, echoed in response; HTTP `?level=N`. A documented default is returned with `defaulted: true`, never client-dependent inference.

`transition(from,to,focus,revision)` keeps exact identity if represented; ascending chooses nearest typed container using rung-pair edge priority; descending restores last-focused valid member or deterministic primary member. Filtered/unavailable focus becomes a ghost reference with reason and inclusion/capability action. CLI/TUI announce level, resolved focus, containment, and shown/total compression. API returns the same `FocusResolution`.

### 6.2 Act at altitude

All mutations show selection expression, exact resolved IDs/count, revision/preconditions, JJK/Git/JJ/file effects, unavailable members, recovery/undo, and plan digest. CLI applies with `--confirm-plan <digest>` or TTY confirmation of that exact digest. Generic `--yes` is allowed only when the action descriptor permits it and never resolves ambiguity. API requires idempotency key and digest. TUI confirmation is keyboard reachable and names action/count.

## 7. Surface contracts

### 7.1 CLI and TUI

Human CLI order is fixed: altitude header; focus/revision/completeness; primary representation; omission/warning line; bounded ordered next actions. `--quiet` never removes safety/incompleteness/ambiguity. `--verbose` stays within the rung. `--format text|json|jsonl`; `--json` aliases JSON.

The TUI consumes snapshots/events only; it never reads storage or runs Git directly. Revision change invalidates plans and refreshes with focus carry. It is keyboard complete. `jjk tui --accessible` uses durable line-oriented output with no alternate screen, cursor redraw, animation, mouse capture, hidden key-only controls, or transient-only reports. `--motion never` and `JJK_REDUCED_MOTION=1` disable animation.

### 7.2 Library/local HTTP API

The Rust library is canonical; an optional daemon exposes identical records:

```text
GET  /v1/repos/{repo}/views?level={0..6}&focus={id}&filter={expr}&cursor={cursor}
POST /v1/repos/{repo}/actions:plan
POST /v1/repos/{repo}/actions:apply
POST /v1/repos/{repo}/operations/{operation}:undo
GET  /v1/repos/{repo}/events?after_revision={revision}
```

Cursors bind fingerprint, revision, level, filters, and sort contract. Stale cursor returns `stale_cursor`. First page targets ≤50 KiB JSON with totals/cursor, never silent truncation.

### 7.3 Structured output

Success emits one `ViewSnapshot`, `ActionPlan`, or `ActionResult` on stdout. Failure emits one schema-versioned `ErrorEnvelope` on stderr, stdout empty.

```json
{"schema":"jjk.api/v1","kind":"view","level":{"id":2,"key":"exploration","name":"Exploration"},"repo":{"id":"r_…","fingerprint":"…"},"graph_revision":"gr_…","focus":{"requested":"s_…","resolved":"a_…","via":"contained_by"},"nodes":[],"edges":[],"actions":[],"omitted":{"incomplete":false,"hidden_counts":{},"reasons":[]},"capabilities":{},"warnings":[],"next_page":null}
```

JSON has no ANSI, hyperlinks, truncation, localized keys, or TTY fields; uses full IDs/RFC3339 UTC; canonical array order and schema-order serialization; distinguishes unavailable/redacted/absent/null; escapes controls. JSONL is only for explicit event streams. Human progress uses stderr and is suppressed in JSON mode unless requested as JSONL.

## 8. Accessible deterministic terminal rendering

### 8.1 Width

Resolve width: `--width N` → TUI pane → TTY columns → positive `COLUMNS` → 80. Measure terminal cells after stripping ANSI with one pinned Unicode-width table. Never split graphemes. Escape tabs, CR/LF, C0/C1, ESC, bidi controls, and OSC. Embedded newlines display as `↵` or `\n`.

| Width | Representation |
|---|---|
| `<40` | vertical compact records; connector/markers/short ID, then label, then parent/member count; no tables |
| `40..79` | compact graph: short IDs, kind, markers, ellipsized label |
| `80..119` | standard graph/table: label, attempt/branch, concise evidence/status |
| `>=120` | rich bounded graph: message, stats, actor, verification, provenance hint |

No line exceeds width. Membership/order never vary by width. Header and omissions remain.

### 8.2 Color, `NO_COLOR`, Unicode

`--color auto|always|never`, default auto. `never` disables; `auto` requires capable TTY and absent `NO_COLOR`; presence of `NO_COLOR` disables even `always`; `always` otherwise enables deliberate capture. Stable attempt color is a versioned accessible-palette index from a specified cryptographic digest of `(repo fingerprint, attempt ID)`, never process hash, row order, label, or width. Markers remain: `* current`, `^ tip`, `+ trusted`, `! dirty`, `> selected`, `? warning`; red/green never solely means fail/pass.

`--unicode auto|always|never`; auto requires UTF-capable locale and non-`dumb` terminal. ASCII and Unicode preserve topology/markers. OSC 8 is off for non-TTY/JSON and capability-gated otherwise; visible text contains destination identity.

### 8.3 Non-TTY

Non-TTY disables color, cursor addressing, alternate screen, animation, hyperlinks, pager, and selection prompts; uses `--width`, `COLUMNS`, or 80; returns typed ambiguity/confirmation errors; requires plan digest for risky apply; pages explicitly rather than invoking a pager; preserves exit codes. TTY enables decoration/interaction only after semantic ordering is fixed.

### 8.4 Deterministic graph algorithm

1. Build one-revision snapshot.
2. Require visible endpoints or typed boundary stubs.
3. Rank object/edge kinds from schema-versioned tables.
4. Order roots `(canonical priority, observed UTC created_at, EntityId)`.
5. Order children `(edge rank, attempt/canonical priority, observed UTC created_at, EntityId)`.
6. Topologically traverse ancestry; cycles become corruption diagnostics, never hangs.
7. Assign lowest free lane at divergence and retain through deterministic last use; ties use IDs.
8. Render secondary composition/provenance edges after ancestry in edge-kind/ID order.
9. Fold only when rung permits; fold records exact ordered members/endpoints.
10. Hash schema, revision, ordered nodes/edges, focus, filters, omissions before width/color rendering.

Terminal ancestry is root-to-leaf. A future canvas may invert orientation but consumes the same graph and declares orientation. No random/crossing heuristic may change canonical order.

## 9. Failure modes

| ID | Failure | Required behavior |
|---|---|---|
| `UX-F-001` | State changed after view/plan | `stale_revision`/`stale_plan`; reconcile and preview anew |
| `UX-F-002` | Ambiguous target | TTY choices; non-TTY/API candidates; never silent choice |
| `UX-F-003` | Filter/fold/page hides topology | boundary stubs plus incomplete/count/reason/expand |
| `UX-F-004` | Narrow/non-UTF/dumb/no-color terminal | deterministic compact ASCII/plain, all semantic markers |
| `UX-F-005` | Hostile label controls/bidi/OSC | sanitize display/measurement; escape structured output |
| `UX-F-006` | Cycle/dangling/duplicate/impossible parent | stop mutation, diagnose, preview repair; no best-effort rewrite |
| `UX-F-007` | Large graph/catalog | aggregates/page with exact totals and revision cursor; stable focus/order |
| `UX-F-008` | TUI revision changes during preview | preserve focus, invalidate digest, refresh, re-plan |
| `UX-F-009` | Unsourced/incorrect model field | mark source/confidence, allow correction, disable materialization |
| `UX-F-010` | Adapter absent/stale | explicit degradation and disabled actions; Git/repo-only path remains |
| `UX-F-011` | Partial Timeshift restore | per-component disposition, confirmed subset only, recovery point, mixed result |
| `UX-F-012` | Git mutation succeeds but journal append fails | durable prepare drives repair/reconciliation before command completion |
| `UX-F-013` | JSON serialization/write fails | no human fallback; typed error where possible and nonzero exit |
| `UX-F-014` | Transparent passthrough invoked | bypass JJK renderer/lock; preserve Git fidelity and exit status |

## 10. Acceptance checks

### 10.1 Rung/schema checks

| Check | Acceptance |
|---|---|
| `UX-ACC-001` | Each level `0..6` exposes ID/key/name/question, typed objects, ≥1 mutation, carried/dropped information, transition, evidence, omission control. |
| `UX-ACC-002` | Adjacent rungs introduce at least one new object type; no rung is merely fewer nodes/fields of the previous rung. |
| `UX-ACC-003` | Every derived field has source/confidence; every aggregate expands to exact revision-bound members. |
| `UX-ACC-004` | Level-6 batch restore demonstrates select → IDs/count → preview → confirm → apply → verify → undo. |

### 10.2 Cross-surface conformance

| Check | Acceptance |
|---|---|
| `UX-ACC-005` | Golden fixture queried at every level through library, CLI JSON, TUI model, and HTTP yields identical revision, focus, ordered node/edge IDs, actions, and omissions. |
| `UX-ACC-006` | Every action available in one surface is available or identically disabled with reason in the others. |
| `UX-ACC-007` | Revision change between plan/apply is rejected on all surfaces; no effects occur. |
| `UX-ACC-008` | Fresh user/integrator can name level from CLI/TUI header or API envelope unaided (3/3 within 5 s for CLI/TUI; 3/3 from envelope alone for API). |

### 10.3 Terminal/accessibility matrix

| Check | Acceptance |
|---|---|
| `UX-ACC-009` | Golden graphs at widths 20, 39, 40, 79, 80, 119, 120, 200 have no over-width line or split grapheme and preserve semantic membership/order. |
| `UX-ACC-010` | TTY/non-TTY × `NO_COLOR` present/absent × color auto/always/never meets §8.2; semantic marker extraction is identical. |
| `UX-ACC-011` | ASCII/Unicode renders decode to identical node/edge/marker facts. |
| `UX-ACC-012` | Hostile labels with multiline, tabs, ESC/OSC, combining marks, emoji ZWJ, wide glyphs, bidi controls cannot forge rows or exceed width. |
| `UX-ACC-013` | Every TUI action and transition works keyboard-only; accessible/reduced-motion modes have durable reports and no transient-only state. |
| `UX-ACC-014` | Non-TTY ambiguity and confirmation never block/read stdin; emit typed candidates/errors and nonzero exit. |

### 10.4 Determinism, structured output, performance

| Check | Acceptance |
|---|---|
| `UX-ACC-015` | Permuting event insertion/map iteration 1,000 times yields identical canonical graph digest, order, lanes, fold members, and plan selection. |
| `UX-ACC-016` | JSON is ANSI-free, full-ID, schema-valid, deterministic, untruncated, and semantically identical across widths/TTY/color/locale. |
| `UX-ACC-017` | Transparent Git passthrough conformance covers non-UTF argv/path where supported, cwd/env/stdin/stdout/stderr, SIGINT/SIGTERM, and exit codes 0/1/128. |
| `UX-ACC-018` | Warm `status/current` p95 <50 ms and 1,000-state first paint p95 <100 ms on declared small/monorepo/large-history/many-worktree/network-FS fixtures; API p95 <200 ms and first page ≤50 KiB. |
| `UX-ACC-019` | Projection reads and rung transitions allocate within measured bounded budgets; no whole-repo scan per view. Exact benchmark hardware/data/results must replace **NOT yet live-verified** before release. |

## 11. Explicit non-goals

- A GUI/canvas visual design; it may later consume the same model.
- Making JJ mandatory or exposing storage mechanics as beginner vocabulary.
- Persisting a user’s last level globally; entry-point defaults remain predictable.
- Treating verbosity, filtering, folding, or pagination as abstraction levels.
- A chat/blob as the primary representation.
- Auto-resolving fuzzy targets, auto-promoting candidates, auto-harvesting remote code, or auto-materializing model proposals.
- Pretending a subprocess changed its parent shell cwd; shell integration must return/consume an explicit path.
- Claiming complete terminal/process restoration; Timeshift reports component truth.
- Encoding unique meaning in color, glyphs, animation, mouse behavior, or hyperlinks.
- Letting renderers or adapters derive independent graph/action semantics.
- Tying the UX API to SQLite tables; SQLite WAL remains replaceable behind the revision/transaction contract.
- Changing Git behavior in transparent passthrough.

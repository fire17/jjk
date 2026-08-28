# State Graph Architecture

**Status:** normative JJK v0.1 rewrite design.  
**Scope:** state, attempt, branch, workspace, composition, promotion, navigation, provenance, verification, and archive graph.

## Context

JJK is the semantic state layer above Git and optional Jujutsu. Git remains the universal content, ancestry, ref, and collaboration substrate; JJ may accelerate local history and recovery. JJK must preserve what neither substrate does: what an actor attempted, which state they meant, which future they chose, and which exact contribution a composition introduced.

The current implementation proves the product shape but conflates concepts the rewrite must separate:

- a logical parent and a Git parent are not interchangeable;
- labels, branch names, JJK IDs, Git OIDs, and JJ IDs are different identity domains;
- atomic pick must preserve its source as provenance without claiming it as a Git parent;
- “best of A and B” may have several valid results;
- navigation may revisit states while semantic ancestry must remain acyclic;
- archive must hide without severing topology or reachability;
- branch leaves, topology leaves, and attempt tips are distinct facts.

The graph is a **typed, relation-specific multigraph projected from an append-only event journal**. Its structural spine is a forest of states with at most one logical parent per state. Git ancestry, composition, provenance, navigation, and mutable placements are separate relations.

All mutations use:

`discover → lock → reconcile → resolve → plan → durable prepare → mutate Git/JJ/files → append events+projections → verify → commit/repair`

JJK-native commands (`return`, `fork`, `pick`) operate on this graph. Git-enhanced commands may join it with Git facts. Transparent Git passthrough creates no guessed JJK edges; reconciliation imports its observable effects.

## Decisions

### SG-D01 — Relation-specific persistence

Each edge family has a table and typed API record. A tagged `GraphEdge` union may exist for rendering, but an untyped `(from, type, to, json)` table is not authoritative. This makes cardinality, foreign keys, indexes, archive behavior, and cycle checks enforceable.

### SG-D02 — Typed stable identities

Durable JJK IDs are UUIDv7 stored as 16-byte blobs. Public forms use Crockford Base32 and prefixes:

| Entity | Prefix | Entity | Prefix |
|---|---:|---|---:|
| safe space | `ss_` | state | `st_` |
| attempt | `at_` | branch | `br_` |
| workspace | `ws_` | composition | `cmp_` |
| candidate | `cand_` | delta | `dlt_` |
| promotion | `prm_` | navigation visit | `nav_` |
| provenance | `prov_` | verification | `ver_` |
| archive episode | `arc_` | operation/event | `op_` / `evt_` |

UUID order is never semantic ancestry.

```rust
struct GitOid { algorithm: GitHashAlgorithm, bytes: Vec<u8> } // Sha1=20, Sha256=32
struct JjCommitId(Vec<u8>);
struct JjChangeId(Vec<u8>);
struct RefNameBytes(Vec<u8>);
struct ContentDigest { algorithm: DigestAlgorithm, bytes: [u8; 32] }
```

No API accepts an untyped string where identity domains are ambiguous. StateId is not derived from GitOid; label, ref, or path changes never change identity.

### SG-D03 — Labels are mutable scoped aliases

A state has one current primary label and any number of aliases. Exact UTF-8 input is preserved; a separate normalized/case-folded key supports search. Labels may collide and be renamed. They are never foreign keys, provenance identities, delta identities, or verification subjects.

Exact StateId wins resolution. Multiple label matches are ambiguous even if one is newer. Automation supplies an exact ID or uniquely qualified selector. Branch ref names and attempt names obey the same rule; branch rename preserves BranchId.

### SG-D04 — Logical parent is singular and semantic

`LogicalParent(child, parent)` means: the atomic semantic contribution of `child` is the tree transformation `parent → child`.

```rust
enum ParentResolution {
    CompleteRoot,
    Resolved,
    Boundary { missing_git_parent: GitOid },
    Unresolved { reason: ParentResolutionError },
}
```

`Resolved` has exactly one edge. The other variants have none. `Boundary` is partial history, not a root; `Unresolved` blocks parent-dependent operations. Logical parentage is an acyclic forest.

### SG-D05 — Git parents stay separate and ordered

```rust
struct GitParentEdge { child: GitOid, parent: GitOid, parent_index: u32 }
```

A normal JJK-created state has its logical parent's commit as sole Git parent. An imported Git merge may have many ordered Git parents but one JJK logical parent.

Import rules:

1. proven complete Git root → `CompleteRoot`;
2. single parent → unique imported state for that parent commit;
3. merge → first-parent state is logical parent with basis `ImportedGitMainline`; all Git parents remain and non-mainline integration is provenance;
4. missing first parent → `Boundary`;
5. ambiguous mapping → `Unresolved`, never a guess.

The importer owns one `(SafeSpaceId, GitOid) → imported StateId` mapping. Semantic annotations sharing an OID do not replace it.

### SG-D06 — Attempts, branches, and workspaces differ

An **attempt** is a semantic exploration line. A **branch** is Git interoperability. A **workspace** is a checked-out execution location.

- Every state belongs to exactly one attempt.
- A root attempt may have no base; a forked attempt has one base state.
- Its first state has that base as logical parent.
- Explicit fork may create an empty attempt before first capture.
- Attempt identity survives branch rename/rebinding.
- A canonical branch may point to a state owned by an exploratory attempt.
- A workspace may navigate historically without owning a branch.
- One writable workspace holds the mutation lease for a branch/attempt binding; concurrent actors use distinct attempts and branches.

### SG-D07 — Composition is a hyperedge

A composition is:

`input states + target base + exact intent → zero or more candidates → result state(s)`

It is not an extra state parent. Each candidate records exact instructions, actor, isolated attempt/workspace where needed, conflicts, verification, and result. Retrying “best of A and B” appends a candidate; it never overwrites an answer. Atomic pick is the deterministic one-source member; semantic merge/harvest may yield plural candidates.

### SG-D08 — Atomic pick uses logical-parent delta

For source `S` with logical parent `P`:

`D(S) = CanonicalTreeDelta(tree(P), tree(S))`

For exact target-base `T`:

`R_tree = ApplyThreeWay(D(S), base=tree(P), ours=tree(T), theirs=tree(S))`

Only paths in `P → S` may change in `R_tree`; all other target entries stay byte-identical. Result `R` has logical parent `T`, sole Git parent `commit(T)`, no Git parent to `S`/`P`, and provenance to `T`, `S`, `P`, Delta, decisions, and operation.

Root pick is refused by default because it means adding the whole root snapshot; explicit root-as-addition requires full preview. Boundary/unresolved sources cannot be picked.

### SG-D09 — Return is navigation; fork is topology

A clean return creates a NavigationVisit and changes workspace placement. It creates no state, attempt, branch, or logical edge.

- Return to an available attempt tip resumes its branch.
- Return to a historical state materializes its exact tree in historical/detached mode and sets `pending_fork_origin`; no branch rewinds.
- If that branch is writable elsewhere, return stays detached/read-only; mutation forks instead of colliding.
- Dirty/staged/untracked/path-colliding work is protected by one meaningful recovery checkpoint when necessary or the command fails unchanged. Ignored files are not silently captured/deleted.
- First mutation after historical return creates a sibling attempt/branch rooted there; prior futures remain.
- Return to an unadvanced available tip extends the same attempt without an unnecessary fork.
- Explicit `fork` creates an attempt immediately; `--worktree` also binds an isolated workspace.

### SG-D10 — Traversal domains never blur

`up`, `down`, ancestry, children, siblings, and topology leaves use logical parentage. `back`, `forward`, and `return -` use navigation. Git ancestry uses Git-parent edges. Composition lineage uses composition/provenance. No command silently switches domains.

### SG-D11 — Promotion is evidence-gated CAS

A promotion records source state/attempt, VerificationRunIds, target BranchId, expected prior state/OID, result state/OID, approver/policy, mode, and rollback. It does not parent the source.

Direct/fast-forward may point canonical branch at the source. Replay/projection creates a result whose logical parent is previous canonical state and whose provenance points to source material. Rollback is a new compare-and-swap promotion, never history editing.

### SG-D12 — Archive is a reversible visibility overlay

Archive never deletes topology, provenance, evidence, or reachability. It preserves identity and state/archive refs. Recovery closes an archive episode and restores the same ID; ref-name collision requires an alternate visible name. Current/writable workspace targets require safe relocation. Filtered views show hidden-ancestor markers and `filtered=true`, never fake roots. Hard deletion and GC are out of scope.

### SG-D13 — SQLite WAL for local journal/projections

SQLite WAL is retained over mutable JSON (whole-file writes/weak constraints), custom event files (reimplemented indexes/transactions), and a graph DB (unneeded operational surface). It gives strict tables, transactions, recursive CTEs, and fast concurrent readers.

Limits are explicit: safe-space lock remains mandatory across Git/JJ/files; events and projections update in one transaction after durable prepare; `synchronous=FULL` protects prepare/commit; a hash-chained journal export prevents DB-only recovery. Unsupported/network filesystems explicitly use rollback-journal single-writer mode or refuse concurrent mutation. Live WAL is never metadata transport.

## Entity schemas

```rust
struct State {
    id: StateId, safe_space_id: SafeSpaceId, kind: StateKind,
    git_commit: GitOid, git_tree: GitOid,
    jj_commit: Option<JjCommitId>, jj_change: Option<JjChangeId>,
    parent_resolution: ParentResolution, topology_rank: u64,
    created_by: OperationId, created_at_utc_ns: i128,
    actor_id: ActorId, stats: StateStats,
}
enum StateKind { Init, New, Git, Save, Step, Nice, Cherry, Stash, Auto }
struct LogicalParent {
    child_state_id: StateId, parent_state_id: StateId,
    basis: LogicalParentBasis, established_by: EventId,
}
enum LogicalParentBasis {
    CapturedFromCurrent, AttemptBase, PickTargetBase,
    CompositionTargetBase, ImportedGitMainline, Migration, HumanResolution,
}
struct StateLabel {
    label_id: LabelId, state_id: StateId, scope: LabelScope,
    exact_text: String, search_key: String, is_primary: bool,
    valid_from_event: EventId, valid_until_event: Option<EventId>,
}
```

`star`, `pin`, approval, rejection, notes, and tags are annotations, not snapshot kinds. Clean star/nice creates no empty state. Every state verifies `commit.tree == git_tree` and has a state ref or equivalent reachability proof.

```rust
struct Attempt {
    id: AttemptId, safe_space_id: SafeSpaceId,
    base_state_id: Option<StateId>, display_name: String, purpose: String,
    status: AttemptStatus, created_by: OperationId,
    created_at_utc_ns: i128, owner: Option<ActorId>,
}
enum AttemptStatus { Active, Candidate, Chosen, Rejected, Parked, Archived }
struct AttemptState { attempt_id: AttemptId, state_id: StateId, sequence: u64 }
```

State ownership is unique; sequence is projection order, not identity.

```rust
struct Branch {
    id: BranchId, safe_space_id: SafeSpaceId, ref_name: RefNameBytes,
    role: BranchRole, lifecycle: BranchLifecycle,
    bound_attempt_id: Option<AttemptId>, observed_tip_oid: Option<GitOid>,
    tip_state_id: Option<StateId>, reconciliation: RefReconciliation,
    last_event_id: EventId,
}
enum BranchRole { Exploratory, CanonicalMain, Staging, Production, Submission, External }
enum BranchLifecycle { Active, Archived, MissingExternally }
enum RefReconciliation { Reconciled, ForeignTip, MissingObject, Diverged }
```

For `Reconciled`, tip state's commit equals the ref OID. A branch leaf is an active branch tip; a topology leaf has no visible logical children; an attempt tip is the latest owned live state.

```rust
struct Workspace {
    id: WorkspaceId, safe_space_id: SafeSpaceId,
    git_worktree_id: Vec<u8>, canonical_path_bytes: Vec<u8>, path_display: String,
    mode: WorkspaceMode, current_state_id: Option<StateId>,
    bound_attempt_id: Option<AttemptId>, bound_branch_id: Option<BranchId>,
    pending_fork_origin: Option<StateId>, mutation_lease: MutationLeaseState,
    last_observed: WorkspaceObservation,
}
enum WorkspaceMode { Attached, HistoricalDetached, ConflictSandbox, ReadOnly }
struct WorkspaceObservation {
    head_oid: Option<GitOid>, index_tree: Option<GitOid>,
    tracked_dirty: bool, staged_dirty: bool,
    untracked_count: u64, ignored_collision_count: u64,
    observed_at_utc_ns: i128,
}
```

```rust
struct Delta {
    id: DeltaId, source_state_id: StateId, source_parent_state_id: StateId,
    base_tree: GitOid, result_tree: GitOid,
    manifest_digest: ContentDigest, algorithm: DeltaAlgorithm,
    path_change_count: u64,
}
struct PathChange { path_bytes: Vec<u8>, before: Option<TreeEntry>, after: Option<TreeEntry> }
struct TreeEntry { mode: u32, object_oid: GitOid, kind: TreeEntryKind }
```

Canonical path records sort by raw bytes. Rename is delete+add for identity; display rename is annotation. Versioned canonical CBOR plus a domain separator determines DeltaId, independent of Git config, locale, drivers, filters, and rename heuristics.

```rust
struct Composition {
    id: CompositionId, safe_space_id: SafeSpaceId, kind: CompositionKind,
    target_base_state_id: StateId, intent_exact: String,
    created_by: OperationId, created_at_utc_ns: i128,
}
enum CompositionKind { AtomicPick, SemanticMerge, FeatureHarvest, FunctionalProjection }
struct CompositionInput {
    composition_id: CompositionId, ordinal: u32,
    role: CompositionInputRole, state_id: StateId,
}
enum CompositionInputRole { Source, Alternative, Upstream, Constraint, TargetBase }
struct CompositionCandidate {
    id: CompositionCandidateId, composition_id: CompositionId,
    candidate_ordinal: u32, instructions_exact: String,
    instructions_digest: ContentDigest, attempt_id: AttemptId,
    workspace_id: Option<WorkspaceId>, status: CandidateStatus,
    result_state_id: Option<StateId>, source_delta_id: Option<DeltaId>,
    created_by: OperationId,
}
enum CandidateStatus { Prepared, Running, Conflict, Materialized, Verified, Rejected, Chosen, Failed }
```

```rust
struct Promotion {
    id: PromotionId, source_state_id: StateId, source_attempt_id: AttemptId,
    target_branch_id: BranchId,
    expected_previous_state_id: Option<StateId>, expected_previous_oid: Option<GitOid>,
    result_state_id: StateId, result_oid: GitOid,
    policy_id: PromotionPolicyId, approver: ApprovalPrincipal,
    verification_run_ids: Vec<VerificationRunId>, mode: PromotionMode,
    reverses_promotion_id: Option<PromotionId>, status: PromotionStatus,
    created_by: OperationId,
}
enum PromotionMode { Direct, FastForward, ReplayedProjection, Rollback }
enum PromotionStatus { Prepared, Applied, Failed, Reversed }
```

```rust
struct NavigationVisit {
    id: NavigationVisitId, workspace_id: WorkspaceId,
    from_state_id: Option<StateId>, to_state_id: StateId,
    cause: NavigationCause, sequence: u64,
    created_at_utc_ns: i128, operation_id: OperationId,
}
enum NavigationCause { Return, ReturnDash, Back, Forward, Up, Down, Checkout, ForkReady, CaptureResult }
struct NavigationCursor {
    workspace_id: WorkspaceId, history_generation: u64,
    sequence: u64, current_visit_id: NavigationVisitId,
}
```

Visits are append-only and may revisit states. New ordinary navigation after back starts a generation rather than deleting forward visits.

```rust
struct ProvenanceRecord {
    id: ProvenanceId, subject: SubjectRef, relation: ProvenanceRelation,
    actor_id: ActorId, operation_id: OperationId, event_id: EventId,
    tool: ToolIdentity, command_argv_digest: Option<ContentDigest>,
    environment_digest: Option<ContentDigest>, narrative_exact: Option<String>,
    created_at_utc_ns: i128,
}
struct ProvenanceSource {
    provenance_id: ProvenanceId, ordinal: u32,
    role: ProvenanceSourceRole, source: SubjectRef,
    evidence_digest: Option<ContentDigest>,
}
enum SubjectRef {
    State(StateId), Attempt(AttemptId), Branch(BranchId), Workspace(WorkspaceId),
    Composition(CompositionId), Candidate(CompositionCandidateId), Delta(DeltaId),
    Promotion(PromotionId), Verification(VerificationRunId), GitCommit(GitOid),
    JjChange(JjChangeId), ExternalArtifact(ExternalArtifactId), Event(EventId),
}
enum ProvenanceRelation { CreatedBy, DerivedFrom, ImportedFrom, ComposedFrom, ResolvedBy, References }
```

Causal relations point to earlier committed subjects. `References` may cycle and is excluded from ancestry. Sensitive evidence uses redacted/content-addressed locators.

```rust
struct VerificationRun {
    id: VerificationRunId, subject: SubjectRef,
    subject_content_digest: ContentDigest, environment_digest: ContentDigest,
    policy_id: Option<VerificationPolicyId>, actor_id: ActorId,
    operation_id: OperationId, started_at_utc_ns: i128, finished_at_utc_ns: i128,
    aggregate: VerificationAggregate,
}
struct VerificationCheck {
    run_id: VerificationRunId, ordinal: u32,
    check_kind: VerificationCheckKind, command_argv_digest: Option<ContentDigest>,
    exit: ProcessExit, status: CheckStatus,
    stdout_evidence: Option<ContentDigest>, stderr_evidence: Option<ContentDigest>,
    assertion_digest: ContentDigest,
}
enum CheckStatus { Pass, Fail, Error, Skipped }
enum VerificationAggregate { Pass, Fail, Error, Partial }
```

Verification is immutable and binds exact content/environment, never a label.

```rust
struct ArchiveEpisode {
    id: ArchiveId, target: ArchivableRef, reason_exact: String,
    archived_by: OperationId, archived_at_utc_ns: i128,
    prior_placement_digest: ContentDigest, retained_ref: Option<RefNameBytes>,
    recovered_by: Option<OperationId>, recovered_at_utc_ns: Option<i128>,
}
enum ArchivableRef {
    State(StateId), Attempt(AttemptId), Branch(BranchId),
    Composition(CompositionId), Candidate(CompositionCandidateId),
}
```

## Edge catalog

| Edge | From → To | Cardinality | Cycles |
|---|---|---:|---|
| LogicalParent | State → State | child 0..1; parent 0..N | forbidden |
| StateInAttempt | State → Attempt | state exactly 1 | forbidden |
| AttemptBase | Attempt → State | attempt 0..1 | combined ancestry acyclic |
| BranchTip / BranchBoundAttempt | Branch → State / Attempt | branch 0..1 each | placement may revisit |
| WorkspaceAtState / OnBranch | Workspace → State / Branch | workspace 0..1 each | visits may repeat |
| CompositionInput | Composition → State | 1..N | causal cycle forbidden |
| CandidateOf / Result / Attempt | Candidate → Composition / State / Attempt | 1 / 0..1 / 1 | forbidden |
| DeltaSource / SourceParent | Delta → State | exactly 1 each | forbidden |
| PromotionSource / Previous / Result / Target | Promotion → State / Branch | 1 / 0..1 / 1 / 1 | rollback is chain |
| NavigationFrom / To | Visit → State | 0..1 / 1 | state revisits allowed |
| ProvenanceSource | Provenance → Subject | 0..N | causal no; reference yes |
| VerificationSubject | Verification → Subject | exactly 1 | not ancestry |
| ArchiveTarget | Archive → Archivable | exactly 1 | repeated episodes allowed |
| GitParent | Git commit → Git commit | 0..N ordered | forbidden by Git |

## Projection constraints

Core SQLite tables are `states`, `state_logical_parents`, `state_labels`, `attempts`, `attempt_states`, `branches`, `workspaces`, `deltas`, `delta_path_changes`, `compositions`, `composition_inputs`, `composition_candidates`, `promotions`, `promotion_verifications`, `navigation_visits`, `navigation_cursors`, `provenance_records`, `provenance_sources`, `verification_runs`, `verification_checks`, `archive_episodes`, `git_commits`, and `git_parent_edges`.

All are `STRICT`; entity keys are `BLOB CHECK(length(id)=16)`. `state_logical_parents.child_state_id`, `attempt_states.state_id`, `(attempt_id, sequence)`, live `(safe_space_id, ref_name)`, `(composition_id, candidate_ordinal)`, and `(workspace_id, navigation_sequence)` are unique. Foreign keys are real, including commit-time integrity checks for polymorphic SubjectRef.

Before inserting logical parent, a recursive ancestor query under the safe-space write lock rejects any ancestry containing child. Parent rank must be lower. Commit verification repeats the query. Projection rebuild detects and quarantines cycles instead of looping.

## Graph invariants

- **SG-I01:** entities belong to one SafeSpaceId; only typed external provenance crosses spaces.
- **SG-I02:** IDs are immutable/never reused; labels, refs, paths are not IDs.
- **SG-I03:** GitOid carries algorithm; cross-algorithm bytes never compare equal.
- **SG-I04:** every endpoint exists at the snapshot or is explicit boundary/external reference.
- **SG-I05:** state has zero/one logical parent; only `Resolved` has one.
- **SG-I06:** logical edges are acyclic and child rank exceeds parent rank.
- **SG-I07:** every state belongs to exactly one attempt.
- **SG-I08:** first state in forked attempt has attempt base as parent; later states descend from prior attempt state unless explicit composition target differs.
- **SG-I09:** logical child/parent share safe space and verified Git trees.
- **SG-I10:** normal JJK state Git parent is target base; composition source is not an extra Git parent.
- **SG-I11:** Git merge may have many Git parents but only one logical parent; other ancestry remains.
- **SG-I12:** attempt ancestry is acyclic.
- **SG-I13:** reconciled branch tip state resolves to actual ref OID.
- **SG-I14:** branch leaf, topology leaf, and attempt tip derive/render independently.
- **SG-I15:** one active ref name maps to one BranchId; rename preserves ID.
- **SG-I16:** writable branch is checked out in at most one workspace; one mutation lease guards a binding.
- **SG-I17:** historical return never moves ref or creates topology.
- **SG-I18:** composition inputs/target freeze by ID before mutation.
- **SG-I19:** materialized candidate has full provenance and one result or explicit no-op; conflict/failure claims no result.
- **SG-I20:** atomic Delta is source logical-parent→source only.
- **SG-I21:** atomic result's logical/Git parent is target base.
- **SG-I22:** only Delta paths may differ target→result.
- **SG-I23:** causal provenance points only to earlier subjects; reference cycles are excluded from lineage.
- **SG-I24:** candidates remain distinct/recoverable; choosing one deletes none.
- **SG-I25:** verification binds exact content/environment digests.
- **SG-I26:** promotion requires fresh exact evidence and expected prior ref OID.
- **SG-I27:** promotion/rollback are append-only CAS transitions.
- **SG-I28:** archive preserves graph/evidence/reachability and identity.
- **SG-I29:** filtered graph reports boundaries and never turns hidden-parent child into a root.
- **SG-I30:** navigation cycles never enter logical/attempt/Git/causal ancestry.
- **SG-I31:** ambiguous query cannot mutate/navigate non-interactively.
- **SG-I32:** projections rebuild reproducibly from the journal through the same EventId.

## Traversal and query semantics

Reads are pinned to `{SafeSpaceId, through_event_id, projection_version}`. Results report `complete`, `filtered`, `boundary_count`, and pagination cursor.

- `parent` → `Root | Parent(StateId) | Boundary(GitOid) | Unresolved(reason)`.
- `ancestors` follows only logical parent and stops explicitly at root/boundary/unresolved/filter boundary.
- `children` returns direct logical children ordered by committed topology rank then StateId.
- `siblings` returns other children of the same parent; complete roots may be siblings only in an explicit roots view.
- `descendants` uses a bounded recursive CTE.
- `attempt_path` returns owned states by sequence plus its external base marker.
- `attempt_futures(state)` returns attempts based at state.
- `topology_leaves` and `branch_leaves` are separate queries.
- `composition_lineage` follows only causal composition/provenance with bounds/visited set and never adds parents.
- `git_ancestry` follows ordered Git-parent facts and labels shallow boundaries.
- `up` follows logical parent; `down` uses direct logical children. Active navigation's next visit is preferred; multiple otherwise require interactive choice or exact selector.
- `back`/`forward` use navigation. `return -` visits the previous distinct state and appends a visit, producing toggle behavior.
- `story` is a curated path, neither full topology nor visit history.

Resolution phases are exact typed ID; unique typed prefix; exact primary label in explicit scope; exact alias/ref/attempt in explicit scope; normalized exact text; fuzzy label/message/tag. Multiple matches produce aligned choices with ID, kind, attempt, placements, date, stats, archive, and verification. Confidence orders display only and never authorizes mutation. Plans persist selected IDs, not query text.

## Exact atomic-pick derivation

1. Resolve/freeze source `S` and current exact target `T`. Dirty work becomes one necessary recovery state used as T, or command fails unchanged.
2. Require `LogicalParent(S)=P`; refuse boundary/unresolved and root by default.
3. Verify P/S/T commit-tree identities and object availability.
4. Walk P/S trees by raw path bytes; emit sorted before/after mode/type/OID entries; hash canonical versioned CBOR to DeltaId.
5. Reject NUL/absolute/`..` paths, case-fold collisions, file/directory collisions, unsafe symlink traversal, unsupported gitlinks, and sparse omissions.
6. In a scratch index/tree merge each changed path with base=P, ours=T, theirs=S. Deterministic internal text merge is versioned. Binary/mode/symlink applies only if ours equals base or theirs; otherwise conflict. User drivers/hooks/filters/config do not silently decide.
7. Prove locality: every T→R changed path is in Delta; every absent path has identical T/R entry.
8. Conflicts materialize only in an isolated candidate/workspace; target branch stays unchanged. Decisions are provenance.
9. Durable prepare records S/P/T IDs/OIDs, DeltaId, planned tree, expected target ref OID, workspace digest, and OperationId.
10. Create result commit with sole Git parent T and CAS the target ref. Optional JJ mirrors semantics.
11. In one DB transaction append result state (logical parent T), composition/candidate, Delta, provenance, placement updates, and navigation visit.
12. Verify ref, HEAD/index/worktree, result tree, state ref, invariants, and locality. Repair deterministically completes or rolls back; never creates a second result.

If result tree equals T, record a no-op candidate and create no empty state/commit.

## Return and fork rules

```text
AttachedTip --return(historical)--> HistoricalDetached
HistoricalDetached --first mutation--> NewSiblingAttempt --capture--> AttachedTip
AttachedTip --return(available same tip)--> AttachedTip
explicit fork(target) --> EmptyNewAttempt
EmptyNewAttempt + --worktree --> IsolatedWorkspaceReady
```

`HistoricalDetached` is semantic; an adapter may use a private safety ref/JJ workspace but must not claim it is the old branch tip. Fork names are presentation only. Name collision gets a visible suffix or prompt; AttemptId/BranchId stay authoritative.

## Orange/purple/fast-mode fixture

| State | Kind | Logical parent | Attempt | Content |
|---|---|---|---|---|
| M | Init | root | main | initial |
| G | Save | M | green | green, slow |
| P | Step | G | purple | purple, slow |
| FP | Step | P | purple | purple, fast |
| O | Step | G | orange | orange, slow |
| FO | Cherry | O | orange | orange, fast |

```text
M [main branch leaf]
└── G [green branch leaf]
    ├── P
    │   └── FP [purple branch leaf; source]
    └── O
        └── FO [orange branch leaf; current; nice]
             ⇐ provenance: delta(P → FP) applied onto O
```

Attempts are `main(base none, M)`, `green(base M, G)`, `purple(base G, P→FP)`, and `orange(base G, O→FO)`. Branch leaves are M/G/FP/FO; topology leaves are FP/FO. P is not a purple branch leaf; M/G remain branch leaves despite descendants elsewhere.

Atomic proof:

```text
P  = color=purple, fast=false
FP = color=purple, fast=true
D  = only fast false→true
O  = color=orange, fast=false
FO = color=orange, fast=true
```

Three-way uses P as base, O as ours, FP as theirs. FO has O as sole Git/logical parent. Composition inputs are Source(FP) and TargetBase(O); Delta records source FP and source parent P. Thus purple remains a sibling future and only fast mode reaches orange.

For “best combination of purple and orange,” one `cmp_X` freezes inputs P/O and may produce `cand_1 → attempt balanced → R1` and `cand_2 → attempt accessible → R2`. Different retry instructions create `cand_3`. Choice/promotion never deletes or reparents R1/R2.

## Failure modes

| Failure | Required response |
|---|---|
| missing shallow parent | Boundary; render it; block pick/up; explicit fetch only |
| imported merge ambiguity | retain all Git parents; one explicit mainline logical parent or Unresolved |
| duplicate label | show choices; automation fails unchanged |
| logical/attempt/provenance cycle | reject before mutation; rebuild quarantine if already present |
| leaf-type confusion | independent markers and legend |
| return would rewind ref | reject plan; return changes workspace only |
| branch writable elsewhere | detached/read-only return; mutation forks/waits |
| dirty/path collision | one recovery checkpoint or stop; ignored files untouched |
| target ref raced | CAS fails; reconcile/replan |
| root/boundary pick | root explicit-preview only; boundary/unresolved always refuse |
| config-dependent delta | canonical internal tree walk/hash |
| text/binary/mode conflict | isolated candidate; target unchanged |
| unsafe/case-colliding path | reject pre-materialization with raw paths |
| missing partial-clone object | explicit fetch plan or clean failure |
| composition retry | append candidate; never overwrite |
| stale verification | reject promotion; rerun |
| archived hidden ancestor | hidden marker + filtered=true |
| current target archived | reject or require safe relocation |
| crash between layers | prepared operation deterministically completes/rolls back |
| unsupported WAL filesystem | explicit single-writer fallback or refusal |
| navigation revisit | allowed only in navigation; excluded from ancestry cycles |

## Acceptance checks

- **SG-A01:** renaming label/attempt/ref/workspace path preserves IDs/edges.
- **SG-A02:** State/Git SHA-1/Git SHA-256/JJ identities cannot cross typed APIs.
- **SG-A03:** duplicate `purple` labels are explicit choices; automation fails unchanged.
- **SG-A04:** imported two-parent merge has two ordered Git parents, one logical mainline parent, and integration provenance.
- **SG-A05:** randomized forks/returns/picks/imports preserve forest invariants.
- **SG-A06:** self-parent, ancestor reversal, attempt-base cycle, and causal provenance cycle abort before Git/ref mutation.
- **SG-A07:** G→P then return G→O preserves sibling futures and refs.
- **SG-A08:** fast-mode fixture yields orange+fast; FO parent O; Delta P→FP; Git parents exclude P/FP.
- **SG-A09:** property test: every target→result changed path belongs to source-parent→source and all others are byte-identical.
- **SG-A10:** root default refusal, explicit root preview, and boundary/unresolved refusal.
- **SG-A11:** already-present contribution records no-op candidate and no empty state.
- **SG-A12:** text/binary/symlink/mode/gitlink/file-directory/case-fold conflicts never alter target before isolated resolution.
- **SG-A13:** clean return changes only workspace/navigation.
- **SG-A14:** historical return plus first mutation creates exactly one sibling attempt/branch.
- **SG-A15:** available unadvanced tip extends without fork.
- **SG-A16:** second worktree never steals/resets a writable branch.
- **SG-A17:** fixture branch leaves M/G/FP/FO and topology leaves FP/FO.
- **SG-A18:** return-dash/back/forward/up/down preserve visit cycles and structural acyclicity; ambiguity fails in automation.
- **SG-A19:** plural composition candidates keep distinct attempts/workspaces/results/evidence; choice deletes none.
- **SG-A20:** promotion succeeds only with exact fresh evidence and expected target OID.
- **SG-A21:** rollback preserves both promotion records and exploration.
- **SG-A22:** archive/recover retains refs/edges/evidence and same IDs.
- **SG-A23:** ref-name collision on recovery preserves BranchId with alternate visible name.
- **SG-A24:** every state/materialized candidate has actor, operation, event, sources, content digests, and evidence locators.
- **SG-A25:** projection rebuild through same EventId is byte-identical canonically.
- **SG-A26:** fault injection at every transaction seam yields exactly prior or planned state with one result.
- **SG-A27:** SHA-1/SHA-256 fixtures behave identically with domain separation.
- **SG-A28:** paginated/filtered reads stay pinned to snapshot token and report boundaries.
- **SG-A29:** transparent-Git commits/ref changes reconcile correctly and idempotently.
- **SG-A30:** WAL works locally; unsupported filesystem safely falls back/refuses.

## Explicit non-goals

- A general graph database or user-facing graph query language.
- Inferring semantic parentage from labels, timestamps, similarity, or AI.
- Making Git ancestry identical to JJK logical/composition topology.
- Treating names, paths, Git OIDs, or JJ IDs as JJK IDs.
- Silently choosing fuzzy targets for automation.
- Collapsing “best of A and B” into one answer.
- Silent repository merge-driver/hook/filter authority for v0.1 atomic pick.
- Hard deletion, Git object pruning, or GC policy in v0.1.
- Cross-host live SQLite replication or WAL transport.
- Full terminal/editor/process/conversation restoration; Timeshift owns it.
- AI functional regrouping of mixed commits in v0.1 core.
- Final CLI routing syntax or graph visual/color design.
- Replacing Git or requiring JJ for correctness.

## Implementation boundary

The graph module owns typed IDs, relation schemas, invariant checks, consistent-snapshot queries, traversal, and projection rebuild. Adapters supply verified Git/JJ/worktree facts and execute prepared mutation plans. The transaction coordinator owns locks, durable prepare, cross-layer repair, and closure. The event model owns envelopes and replay order. They meet at typed records here; no layer may weaken them with labels or untyped strings.

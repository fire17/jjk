use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::domain::{AttemptId, GitObjectId, StateId, WorkspaceId};

pub const READ_MODEL_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyState { Safe, Dirty, Diverged, Recovering, Unknown }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticMarker { Current, AttemptTip, Starred, Trusted, Dirty, Archived, Warning }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffScope { Atomic, FullSnapshot }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind { Added, Modified, Deleted, Renamed, Copied, TypeChanged, Unmerged }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind { Context, Addition, Deletion, Notice }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiffLine { pub kind: DiffLineKind, pub text: String }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiffHunk { pub header: String, pub lines: Vec<DiffLine> }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileDiff {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub kind: ChangeKind,
    pub binary: bool,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangeStats { pub changed_files: u32, pub insertions: u32, pub deletions: u32 }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PatchReadModel { pub scope: DiffScope, pub stats: ChangeStats, pub files: Vec<FileDiff> }

impl PatchReadModel {
    pub fn canonicalize(&mut self) {
        self.files.sort_by(|left, right| {
            left.new_path.as_deref().or(left.old_path.as_deref())
                .cmp(&right.new_path.as_deref().or(right.old_path.as_deref()))
                .then_with(|| left.old_path.cmp(&right.old_path))
        });
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnnotationReadModel { pub kind: String, pub value: String }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StateReadModel {
    pub id: StateId,
    pub attempt_id: AttemptId,
    pub logical_parent: Option<StateId>,
    pub git_object: GitObjectId,
    pub kind: String,
    pub label: String,
    pub message: Option<String>,
    pub created_at_utc: String,
    pub sequence: u64,
    pub archived: bool,
    pub annotations: Vec<AnnotationReadModel>,
    pub stats: ChangeStats,
}

impl StateReadModel {
    pub fn is_starred(&self) -> bool { self.annotations.iter().any(|a| a.kind == "star") }
    pub fn is_trusted(&self) -> bool {
        self.annotations.iter().any(|a| a.kind == "trust" && a.value == "trusted")
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AttemptReadModel { pub id: AttemptId, pub label: String, pub tip: StateId, pub archived: bool }

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorktreeChanges { pub staged: u32, pub unstaged: u32, pub untracked: u32, pub conflicted: u32 }

impl WorktreeChanges {
    pub fn is_dirty(&self) -> bool { self.staged > 0 || self.unstaged > 0 || self.untracked > 0 || self.conflicted > 0 }
    pub fn changed_files(&self) -> u32 { self.staged + self.unstaged + self.untracked + self.conflicted }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceReadModel {
    pub id: Option<WorkspaceId>,
    pub branch: Option<String>,
    pub head: Option<GitObjectId>,
    pub changes: WorktreeChanges,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryReadModel { pub required: bool, pub summary: Option<String> }

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct NavigationHistory { pub entries: Vec<StateId>, pub index: Option<usize> }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepositorySnapshot {
    pub revision: u64,
    pub repository_label: String,
    pub current_state: Option<StateId>,
    pub current_attempt: Option<AttemptId>,
    pub workspace: WorkspaceReadModel,
    pub recovery: RecoveryReadModel,
    pub states: Vec<StateReadModel>,
    pub attempts: Vec<AttemptReadModel>,
    pub navigation: NavigationHistory,
    pub capabilities: BTreeMap<String, bool>,
    pub warnings: Vec<String>,
}

impl RepositorySnapshot {
    pub fn canonicalize(&mut self) {
        self.states.sort_by(|left, right| left.sequence.cmp(&right.sequence).then_with(|| left.id.cmp(&right.id)));
        self.attempts.sort_by(|left, right| left.id.cmp(&right.id));
        for state in &mut self.states {
            state.annotations.sort_by(|left, right| left.kind.cmp(&right.kind).then_with(|| left.value.cmp(&right.value)));
        }
        self.warnings.sort();
        self.warnings.dedup();
    }
    pub fn visible_states(&self, include_archived: bool) -> impl Iterator<Item = &StateReadModel> {
        self.states.iter().filter(move |state| include_archived || !state.archived)
    }
    pub fn state(&self, id: StateId) -> Option<&StateReadModel> { self.states.iter().find(|state| state.id == id) }
    pub fn attempt(&self, id: AttemptId) -> Option<&AttemptReadModel> { self.attempts.iter().find(|attempt| attempt.id == id) }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CurrentReadModel {
    pub schema_version: u16,
    pub revision: u64,
    pub safety: SafetyState,
    pub state: Option<StateReadModel>,
    pub attempt: Option<AttemptReadModel>,
    pub workspace: WorkspaceReadModel,
    pub parent: Option<StateId>,
    pub history_position: Option<usize>,
    pub history_length: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StatusReadModel {
    pub schema_version: u16,
    pub revision: u64,
    pub repository_label: String,
    pub safety: SafetyState,
    pub current_state: Option<StateId>,
    pub current_attempt: Option<AttemptId>,
    pub workspace: WorkspaceReadModel,
    pub saved_states: usize,
    pub visible_states: usize,
    pub recovery: RecoveryReadModel,
    pub capabilities: BTreeMap<String, bool>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphNode { pub state: StateReadModel, pub markers: BTreeSet<SemanticMarker>, pub depth: usize, pub lane: usize }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge { pub parent: StateId, pub child: StateId }

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OmissionReadModel { pub archived_states: usize, pub incomplete: bool, pub reasons: Vec<String> }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphReadModel {
    pub schema_version: u16,
    pub revision: u64,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub omitted: OmissionReadModel,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoryEntry { pub state: StateReadModel, pub markers: BTreeSet<SemanticMarker> }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoryReadModel { pub schema_version: u16, pub revision: u64, pub entries: Vec<StoryEntry> }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShowReadModel {
    pub schema_version: u16,
    pub revision: u64,
    pub state: StateReadModel,
    pub parent: Option<StateId>,
    pub patch: PatchReadModel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiffReadModel {
    pub schema_version: u16,
    pub revision: u64,
    pub from: Option<StateId>,
    pub to: StateId,
    pub patch: PatchReadModel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum QueryOutcome {
    Current(CurrentReadModel), Status(StatusReadModel), Graph(GraphReadModel), Story(StoryReadModel), Show(ShowReadModel), Diff(DiffReadModel),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryError {
    Store(String), StateNotFound(StateId), InvalidGraph { state: StateId, reason: String },
    PatchUnavailable { from: Option<StateId>, to: StateId, scope: DiffScope },
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(message) => write!(f, "query store failed: {message}"),
            Self::StateNotFound(state) => write!(f, "state {state} does not exist at this revision"),
            Self::InvalidGraph { state, reason } => write!(f, "invalid graph at state {state}: {reason}"),
            Self::PatchUnavailable { from, to, scope } => write!(f, "{scope:?} diff from {from:?} to {to} is unavailable"),
        }
    }
}
impl Error for QueryError {}

/// Snapshot-consistent read boundary implemented behind application services.
/// Renderers consume its typed results and never receive this capability.
pub trait ReadSnapshotSource {
    fn read_snapshot(&self) -> Result<RepositorySnapshot, QueryError>;
    fn read_diff(&self, from: Option<StateId>, to: StateId, scope: DiffScope, expected_revision: u64) -> Result<PatchReadModel, QueryError>;
}

pub struct QueryService<'source, S: ReadSnapshotSource + ?Sized> { source: &'source S }

impl<'source, S: ReadSnapshotSource + ?Sized> QueryService<'source, S> {
    pub fn new(source: &'source S) -> Self { Self { source } }
    pub fn snapshot(&self) -> Result<RepositorySnapshot, QueryError> {
        let mut snapshot = self.source.read_snapshot()?;
        snapshot.canonicalize();
        Ok(snapshot)
    }
    pub fn current(&self) -> Result<CurrentReadModel, QueryError> {
        let snapshot = self.snapshot()?;
        let safety = safety(&snapshot);
        let state = snapshot.current_state.and_then(|id| snapshot.state(id)).cloned();
        let attempt = snapshot.current_attempt.and_then(|id| snapshot.attempt(id)).cloned();
        let parent = state.as_ref().and_then(|record| record.logical_parent);
        Ok(CurrentReadModel {
            schema_version: READ_MODEL_SCHEMA_VERSION, revision: snapshot.revision, safety, state, attempt,
            workspace: snapshot.workspace, parent, history_position: snapshot.navigation.index.map(|index| index + 1),
            history_length: snapshot.navigation.entries.len(), warnings: snapshot.warnings,
        })
    }
    pub fn status(&self) -> Result<StatusReadModel, QueryError> {
        let snapshot = self.snapshot()?;
        let safety = safety(&snapshot);
        let visible_states = snapshot.visible_states(false).count();
        Ok(StatusReadModel {
            schema_version: READ_MODEL_SCHEMA_VERSION, revision: snapshot.revision, repository_label: snapshot.repository_label,
            safety, current_state: snapshot.current_state, current_attempt: snapshot.current_attempt,
            workspace: snapshot.workspace, saved_states: snapshot.states.len(), visible_states, recovery: snapshot.recovery,
            capabilities: snapshot.capabilities, warnings: snapshot.warnings,
        })
    }
    pub fn graph(&self, include_archived: bool) -> Result<GraphReadModel, QueryError> { layout_graph(self.snapshot()?, include_archived) }
    pub fn story(&self) -> Result<StoryReadModel, QueryError> {
        let snapshot = self.snapshot()?;
        let tips: BTreeSet<_> = snapshot.attempts.iter().map(|attempt| attempt.tip).collect();
        let entries = snapshot.visible_states(false).filter(|state| state.kind == "nice" || state.is_starred()).cloned()
            .map(|state| StoryEntry { markers: markers_for(&snapshot, &state, &tips), state }).collect();
        Ok(StoryReadModel { schema_version: READ_MODEL_SCHEMA_VERSION, revision: snapshot.revision, entries })
    }
    pub fn show(&self, state_id: StateId) -> Result<ShowReadModel, QueryError> {
        let snapshot = self.snapshot()?;
        let state = snapshot.state(state_id).cloned().ok_or(QueryError::StateNotFound(state_id))?;
        let parent = state.logical_parent;
        let mut patch = self.source.read_diff(parent, state.id, DiffScope::Atomic, snapshot.revision)?;
        patch.canonicalize();
        Ok(ShowReadModel { schema_version: READ_MODEL_SCHEMA_VERSION, revision: snapshot.revision, state, parent, patch })
    }
    pub fn diff(&self, from: Option<StateId>, to: StateId, scope: DiffScope) -> Result<DiffReadModel, QueryError> {
        let snapshot = self.snapshot()?;
        if let Some(id) = from { if snapshot.state(id).is_none() { return Err(QueryError::StateNotFound(id)); } }
        if snapshot.state(to).is_none() { return Err(QueryError::StateNotFound(to)); }
        let mut patch = self.source.read_diff(from, to, scope, snapshot.revision)?;
        patch.canonicalize();
        Ok(DiffReadModel { schema_version: READ_MODEL_SCHEMA_VERSION, revision: snapshot.revision, from, to, patch })
    }
}

fn safety(snapshot: &RepositorySnapshot) -> SafetyState {
    if snapshot.recovery.required { SafetyState::Recovering }
    else if snapshot.warnings.iter().any(|warning| warning.contains("reconcil")) { SafetyState::Unknown }
    else if snapshot.warnings.iter().any(|warning| warning.contains("diverg")) { SafetyState::Diverged }
    else if snapshot.workspace.changes.is_dirty() { SafetyState::Dirty }
    else { SafetyState::Safe }
}

fn markers_for(snapshot: &RepositorySnapshot, state: &StateReadModel, tips: &BTreeSet<StateId>) -> BTreeSet<SemanticMarker> {
    let mut markers = BTreeSet::new();
    if snapshot.current_state == Some(state.id) { markers.insert(SemanticMarker::Current); }
    if tips.contains(&state.id) { markers.insert(SemanticMarker::AttemptTip); }
    if state.is_starred() { markers.insert(SemanticMarker::Starred); }
    if state.is_trusted() { markers.insert(SemanticMarker::Trusted); }
    if state.archived { markers.insert(SemanticMarker::Archived); }
    markers
}

fn layout_graph(mut snapshot: RepositorySnapshot, include_archived: bool) -> Result<GraphReadModel, QueryError> {
    snapshot.canonicalize();
    let visible: BTreeMap<_, _> = snapshot.visible_states(include_archived).map(|state| (state.id, state.clone())).collect();
    let tips: BTreeSet<_> = snapshot.attempts.iter().map(|attempt| attempt.tip).collect();
    let mut children: BTreeMap<Option<StateId>, Vec<StateId>> = BTreeMap::new();
    let mut archived_states = 0;
    for state in &snapshot.states {
        if state.archived && !include_archived { archived_states += 1; continue; }
        let parent = match state.logical_parent {
            Some(parent) if visible.contains_key(&parent) => Some(parent),
            Some(parent) if snapshot.state(parent).is_none() => return Err(QueryError::InvalidGraph { state: state.id, reason: format!("logical parent {parent} is absent") }),
            _ => None,
        };
        children.entry(parent).or_default().push(state.id);
    }
    for ids in children.values_mut() {
        ids.sort_by(|left, right| visible[left].sequence.cmp(&visible[right].sequence).then_with(|| left.cmp(right)));
    }
    let mut nodes = Vec::with_capacity(visible.len());
    let mut edges = Vec::with_capacity(visible.len().saturating_sub(1));
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut next_lane = 0;

    struct Walker<'a> {
        snapshot: &'a RepositorySnapshot, visible: &'a BTreeMap<StateId, StateReadModel>, tips: &'a BTreeSet<StateId>,
        children: &'a BTreeMap<Option<StateId>, Vec<StateId>>, visiting: &'a mut BTreeSet<StateId>, visited: &'a mut BTreeSet<StateId>,
        nodes: &'a mut Vec<GraphNode>, edges: &'a mut Vec<GraphEdge>, next_lane: &'a mut usize,
    }
    impl Walker<'_> {
        fn walk(&mut self, id: StateId, depth: usize, lane: usize) -> Result<(), QueryError> {
            if self.visited.contains(&id) { return Ok(()); }
            if !self.visiting.insert(id) { return Err(QueryError::InvalidGraph { state: id, reason: "logical-parent cycle".into() }); }
            let state = &self.visible[&id];
            self.nodes.push(GraphNode { state: state.clone(), markers: markers_for(self.snapshot, state, self.tips), depth, lane });
            for (index, child) in self.children.get(&Some(id)).into_iter().flatten().copied().enumerate() {
                self.edges.push(GraphEdge { parent: id, child });
                let child_lane = if index == 0 { lane } else { *self.next_lane += 1; *self.next_lane };
                self.walk(child, depth + 1, child_lane)?;
            }
            self.visiting.remove(&id); self.visited.insert(id); Ok(())
        }
    }
    let roots = children.get(&None).cloned().unwrap_or_default();
    for (index, root) in roots.into_iter().enumerate() {
        if index > 0 { next_lane += 1; }
        let root_lane = next_lane;
        Walker { snapshot: &snapshot, visible: &visible, tips: &tips, children: &children, visiting: &mut visiting, visited: &mut visited,
            nodes: &mut nodes, edges: &mut edges, next_lane: &mut next_lane }.walk(root, 0, root_lane)?;
    }
    if visited.len() != visible.len() {
        let state = *visible.keys().find(|id| !visited.contains(id)).expect("unvisited state exists");
        return Err(QueryError::InvalidGraph { state, reason: "state is unreachable from every root (cycle suspected)".into() });
    }
    Ok(GraphReadModel { schema_version: READ_MODEL_SCHEMA_VERSION, revision: snapshot.revision, nodes, edges,
        omitted: OmissionReadModel { archived_states, incomplete: archived_states > 0,
            reasons: if archived_states > 0 { vec!["archived states hidden; pass include_archived to expand".into()] } else { Vec::new() } },
        warnings: snapshot.warnings })
}

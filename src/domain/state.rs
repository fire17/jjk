use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::DomainError;

use super::{
    id::{AnnotationId, AttemptId, StateId},
    provenance::{GitObjectId, JjChangeId, JjCommitId},
};

#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum StateKind {
    Init,
    Save,
    Step,
    Nice,
    Git,
    New,
    Stash,
    Cherry,
    Auto,
    Import,
}

#[derive(
    Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct ChangeStats {
    pub changed_files: u32,
    pub insertions: u64,
    pub deletions: u64,
}

#[derive(
    Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ParentResolution {
    CompleteRoot,
    Resolved { parent: StateId },
    Boundary { missing_git_parent: GitObjectId },
    Unresolved { reason: String },
}

impl ParentResolution {
    #[must_use]
    pub const fn parent(&self) -> Option<StateId> {
        match self {
            Self::Resolved { parent } => Some(*parent),
            Self::CompleteRoot | Self::Boundary { .. } | Self::Unresolved { .. } => None,
        }
    }

    #[must_use]
    pub const fn is_complete_root(&self) -> bool {
        matches!(self, Self::CompleteRoot)
    }
}

#[derive(
    Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct State {
    pub id: StateId,
    pub kind: StateKind,
    pub git_object: GitObjectId,
    pub jj_change: Option<JjChangeId>,
    pub jj_commit: Option<JjCommitId>,
    pub parent: ParentResolution,
    pub attempt_id: AttemptId,
    pub topology_rank: u64,
    pub label: String,
    pub message: Option<String>,
    pub stats: ChangeStats,
    /// Visibility is a reversible projection overlay; it never changes topology.
    pub archived: bool,
}

impl State {
    pub fn new(
        id: StateId,
        kind: StateKind,
        git_object: GitObjectId,
        logical_parent: Option<StateId>,
        attempt_id: AttemptId,
        label: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let parent = logical_parent.map_or(ParentResolution::CompleteRoot, |parent| {
            ParentResolution::Resolved { parent }
        });
        Self::with_parent(id, kind, git_object, parent, attempt_id, 0, label)
    }

    pub fn with_parent(
        id: StateId,
        kind: StateKind,
        git_object: GitObjectId,
        parent: ParentResolution,
        attempt_id: AttemptId,
        topology_rank: u64,
        label: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(DomainError::InvalidValue {
                kind: "state label",
                reason: "must be non-empty".into(),
            });
        }
        if parent.parent() == Some(id) {
            return Err(DomainError::LogicalParentCycle);
        }
        if matches!(&parent, ParentResolution::Unresolved { reason } if reason.trim().is_empty()) {
            return Err(DomainError::InvalidValue {
                kind: "parent resolution",
                reason: "unresolved reason must be non-empty".into(),
            });
        }
        Ok(Self {
            id,
            kind,
            git_object,
            jj_change: None,
            jj_commit: None,
            parent,
            attempt_id,
            topology_rank,
            label,
            message: None,
            stats: ChangeStats::default(),
            archived: false,
        })
    }

    #[must_use]
    pub const fn logical_parent(&self) -> Option<StateId> {
        self.parent.parent()
    }
}

#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum AnnotationKind {
    Label,
    Tag,
    Star,
    Note,
    Handoff,
    Trust,
}

#[derive(
    Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct Annotation {
    pub id: AnnotationId,
    pub state_id: StateId,
    pub kind: AnnotationKind,
    pub value: String,
    pub replaces: Option<AnnotationId>,
}

impl Annotation {
    pub fn new(
        id: AnnotationId,
        state_id: StateId,
        kind: AnnotationKind,
        value: impl Into<String>,
        replaces: Option<AnnotationId>,
    ) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty() || value.contains('\0') {
            return Err(DomainError::InvalidValue {
                kind: "annotation",
                reason: "must be non-empty and contain no NUL".into(),
            });
        }
        Ok(Self {
            id,
            state_id,
            kind,
            value,
            replaces,
        })
    }
}

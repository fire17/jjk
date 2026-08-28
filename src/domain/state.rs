use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use crate::error::DomainError;
use super::{id::{AnnotationId, AttemptId, StateId}, provenance::{GitObjectId, JjChangeId, JjCommitId}};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")] pub enum StateKind { Save, Step, Nice, Git, New, Stash, Cherry, Auto, Import }
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] pub struct ChangeStats { pub changed_files: u32, pub insertions: u64, pub deletions: u64 }
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)]
pub struct State { pub id: StateId, pub kind: StateKind, pub git_object: GitObjectId, pub jj_change: Option<JjChangeId>, pub jj_commit: Option<JjCommitId>, pub logical_parent: Option<StateId>, pub attempt_id: AttemptId, pub label: String, pub message: Option<String>, pub stats: ChangeStats, pub archived: bool }
impl State { pub fn new(id: StateId, kind: StateKind, git_object: GitObjectId, logical_parent: Option<StateId>, attempt_id: AttemptId, label: impl Into<String>) -> Result<Self, DomainError> { let label = label.into(); if label.trim().is_empty() { return Err(DomainError::InvalidValue { kind: "state label", reason: "must be non-empty".into() }); } if logical_parent == Some(id) { return Err(DomainError::LogicalParentCycle); } Ok(Self { id, kind, git_object, jj_change: None, jj_commit: None, logical_parent, attempt_id, label, message: None, stats: ChangeStats::default(), archived: false }) } }
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] #[serde(rename_all = "kebab-case")] pub enum AnnotationKind { Label, Tag, Star, Note, Handoff, Trust }
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] pub struct Annotation { pub id: AnnotationId, pub state_id: StateId, pub kind: AnnotationKind, pub value: String, pub replaces: Option<AnnotationId> }

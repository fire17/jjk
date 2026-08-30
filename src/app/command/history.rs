//! Pure whole-control-plane undo/redo and history-divergence planning.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{ArtifactId, AttemptId, GitObjectId, StateId};

/// Exact value retained by a Git reference in a control snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlRefTarget {
    /// A direct reference to a Git object.
    Direct { object: GitObjectId },
    /// A symbolic reference to another refname.
    Symbolic { refname: String },
}

/// Exact attachment of `HEAD`, independent of the current state projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HeadAttachment {
    /// `HEAD` names a reference. The reference may be unborn and absent from `refs`.
    Symbolic { refname: String },
    /// `HEAD` directly names a Git object.
    Detached { object: GitObjectId },
}

/// Immutable state of every mutable control-plane surface restored by undo/redo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ControlSnapshot {
    /// Complete reference namespace. `BTreeMap` gives canonical, deterministic ordering.
    pub refs: BTreeMap<String, ControlRefTarget>,
    /// Exact symbolic or detached `HEAD` attachment.
    pub head: HeadAttachment,
    /// Artifact containing the exact index image.
    pub index_artifact_id: ArtifactId,
    /// Artifact containing the exact worktree image.
    pub worktree_artifact_id: ArtifactId,
    /// Selected semantic state, if the repository has one.
    pub current_state: Option<StateId>,
    /// Selected semantic attempt, if the repository has one.
    pub current_attempt: Option<AttemptId>,
    /// Projection revision represented by this snapshot.
    pub projection_revision: u64,
}

/// Linear control history and its currently restored snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ControlHistory {
    /// Immutable snapshots in chronological order.
    pub snapshots: Vec<ControlSnapshot>,
    /// Index of the currently restored snapshot.
    pub cursor: usize,
}

impl ControlHistory {
    /// Starts a history line at one exact snapshot.
    #[must_use]
    pub fn new(initial: ControlSnapshot) -> Self {
        Self {
            snapshots: vec![initial],
            cursor: 0,
        }
    }
}

/// Requested cursor transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoryDirection {
    Undo,
    Redo,
}

/// Typed effects for transaction coordination; adapters only restore the supplied snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum HistoryEffect {
    /// Atomically restore every control-plane surface from the immutable snapshot.
    RestoreControlSnapshot { snapshot: ControlSnapshot },
    /// Persist the cursor move without rewriting the immutable history line.
    SetHistoryCursor { cursor: usize },
    /// Persist a new history line after a successful ordinary mutation.
    ReplaceHistory { history: ControlHistory },
}

/// Complete undo or redo plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryTransitionPlan {
    pub direction: HistoryDirection,
    pub from_cursor: usize,
    pub to_cursor: usize,
    /// Exact target supplied to the restoring transaction.
    pub target: ControlSnapshot,
    /// Input-equivalent history with only its cursor changed.
    pub resulting_history: ControlHistory,
    /// Ordered typed effects.
    pub effects: Vec<HistoryEffect>,
}

/// Complete plan for appending the snapshot produced by a non-history mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecordMutationPlan {
    /// Snapshot produced by the mutation.
    pub snapshot: ControlSnapshot,
    /// New line. Any redo-only suffix is absent here and nowhere else is mutated.
    pub resulting_history: ControlHistory,
    /// Ordered typed effects.
    pub effects: Vec<HistoryEffect>,
}

/// Invalid history or unavailable transition.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HistoryPlanError {
    #[error("control history must contain at least one snapshot")]
    EmptyHistory,
    #[error("control history cursor {cursor} is outside {snapshot_count} snapshots")]
    CursorOutOfBounds {
        cursor: usize,
        snapshot_count: usize,
    },
    #[error("no earlier control snapshot to undo to")]
    NoUndo,
    #[error("no later control snapshot to redo to")]
    NoRedo,
}

/// Plans one cursor transition without mutating the supplied history.
pub fn plan_history_transition(
    history: &ControlHistory,
    direction: HistoryDirection,
) -> Result<HistoryTransitionPlan, HistoryPlanError> {
    validate_history(history)?;
    let to_cursor = match direction {
        HistoryDirection::Undo => history
            .cursor
            .checked_sub(1)
            .ok_or(HistoryPlanError::NoUndo)?,
        HistoryDirection::Redo => {
            let next = history.cursor + 1;
            if next >= history.snapshots.len() {
                return Err(HistoryPlanError::NoRedo);
            }
            next
        }
    };
    let target = history.snapshots[to_cursor].clone();
    let mut resulting_history = history.clone();
    resulting_history.cursor = to_cursor;

    Ok(HistoryTransitionPlan {
        direction,
        from_cursor: history.cursor,
        to_cursor,
        target: target.clone(),
        resulting_history,
        effects: vec![
            HistoryEffect::RestoreControlSnapshot { snapshot: target },
            HistoryEffect::SetHistoryCursor { cursor: to_cursor },
        ],
    })
}

/// Plans recording an ordinary mutation.
///
/// When the cursor is behind the tip, only the cloned line in this plan is truncated before the
/// new snapshot is appended. The caller's history and its redo suffix remain untouched.
pub fn plan_record_mutation(
    history: &ControlHistory,
    snapshot: ControlSnapshot,
) -> Result<RecordMutationPlan, HistoryPlanError> {
    validate_history(history)?;
    let mut snapshots = history.snapshots[..=history.cursor].to_vec();
    snapshots.push(snapshot.clone());
    let resulting_history = ControlHistory {
        cursor: snapshots.len() - 1,
        snapshots,
    };

    Ok(RecordMutationPlan {
        snapshot,
        resulting_history: resulting_history.clone(),
        effects: vec![HistoryEffect::ReplaceHistory {
            history: resulting_history,
        }],
    })
}

fn validate_history(history: &ControlHistory) -> Result<(), HistoryPlanError> {
    if history.snapshots.is_empty() {
        return Err(HistoryPlanError::EmptyHistory);
    }
    if history.cursor >= history.snapshots.len() {
        return Err(HistoryPlanError::CursorOutOfBounds {
            cursor: history.cursor,
            snapshot_count: history.snapshots.len(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ObjectAlgorithm;

    fn object(byte: u8) -> GitObjectId {
        GitObjectId::new(ObjectAlgorithm::Sha1, vec![byte; 20]).unwrap()
    }

    fn snapshot(revision: u64, byte: u8) -> ControlSnapshot {
        let mut refs = BTreeMap::new();
        refs.insert(
            "refs/heads/main".into(),
            ControlRefTarget::Direct {
                object: object(byte),
            },
        );
        refs.insert(
            "refs/jjk/current".into(),
            ControlRefTarget::Symbolic {
                refname: "refs/heads/main".into(),
            },
        );
        ControlSnapshot {
            refs,
            head: if revision % 2 == 0 {
                HeadAttachment::Detached {
                    object: object(byte.wrapping_add(1)),
                }
            } else {
                HeadAttachment::Symbolic {
                    refname: "refs/heads/main".into(),
                }
            },
            index_artifact_id: ArtifactId::new_v7(),
            worktree_artifact_id: ArtifactId::new_v7(),
            current_state: Some(StateId::new_v7()),
            current_attempt: Some(AttemptId::new_v7()),
            projection_revision: revision,
        }
    }

    #[test]
    fn undo_then_redo_restores_the_exact_whole_control_snapshot() {
        let first = snapshot(10, 0x10);
        let middle = snapshot(11, 0x20);
        let latest = snapshot(12, 0x30);
        let history = ControlHistory {
            snapshots: vec![first, middle.clone(), latest.clone()],
            cursor: 2,
        };
        let original = history.clone();

        let undo = plan_history_transition(&history, HistoryDirection::Undo).unwrap();
        assert_eq!(undo.target, middle);
        assert_eq!(undo.resulting_history.snapshots, history.snapshots);
        assert_eq!(undo.resulting_history.cursor, 1);
        assert_eq!(
            undo.effects[0],
            HistoryEffect::RestoreControlSnapshot {
                snapshot: middle.clone()
            }
        );

        let redo =
            plan_history_transition(&undo.resulting_history, HistoryDirection::Redo).unwrap();
        assert_eq!(redo.target, latest.clone());
        assert_eq!(redo.resulting_history, original);
        assert_eq!(
            redo.effects[0],
            HistoryEffect::RestoreControlSnapshot { snapshot: latest }
        );
        assert_eq!(
            history, original,
            "planning must not mutate the caller's line"
        );
    }

    #[test]
    fn mutation_after_undo_truncates_redo_only_in_the_resulting_plan() {
        let first = snapshot(20, 0x40);
        let restored = snapshot(21, 0x50);
        let abandoned_future = snapshot(22, 0x60);
        let history = ControlHistory {
            snapshots: vec![first.clone(), restored.clone(), abandoned_future.clone()],
            cursor: 1,
        };
        let original = history.clone();
        let divergent = snapshot(23, 0x70);

        let plan = plan_record_mutation(&history, divergent.clone()).unwrap();

        assert_eq!(
            plan.resulting_history.snapshots,
            vec![first, restored, divergent]
        );
        assert_eq!(plan.resulting_history.cursor, 2);
        assert_eq!(
            history, original,
            "the input redo line must remain immutable"
        );
        assert_eq!(history.snapshots[2], abandoned_future);
        assert_eq!(
            plan.effects,
            vec![HistoryEffect::ReplaceHistory {
                history: plan.resulting_history.clone()
            }]
        );
    }

    #[test]
    fn unavailable_transitions_are_typed_errors() {
        let history = ControlHistory::new(snapshot(1, 1));
        assert_eq!(
            plan_history_transition(&history, HistoryDirection::Undo),
            Err(HistoryPlanError::NoUndo)
        );
        assert_eq!(
            plan_history_transition(&history, HistoryDirection::Redo),
            Err(HistoryPlanError::NoRedo)
        );
    }
}

//! Pure, reversible plans for hiding and recovering semantic states.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{ArchiveId, AttemptId, StateId};

/// The topology that archive must preserve and recovery must restore exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StateTopology {
    /// Stable semantic identity. Archive never replaces it.
    pub state_id: StateId,
    /// Sole logical parent, including a root's lack of a parent.
    pub logical_parent: Option<StateId>,
    /// Semantic attempt containing the state.
    pub attempt_id: AttemptId,
}

/// Committed archive facts observed for one state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArchiveState {
    /// Current topology in the committed projection.
    pub topology: StateTopology,
    /// Whether ordinary views currently hide this state.
    pub archived: bool,
    /// Open archive episode when the state is archived.
    pub active_archive: Option<ArchiveId>,
}

/// Exact current-selection facts relevant to safe archival.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CurrentSelection {
    /// State currently restored in the workspace.
    pub state_id: StateId,
    /// Attempt currently selected with that state.
    pub attempt_id: AttemptId,
}

/// Facts frozen by the caller before planning an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArchiveContext {
    /// State selected for archival.
    pub target: ArchiveState,
    /// Current workspace selection, if one exists.
    pub current: Option<CurrentSelection>,
    /// Resolved replacement named by the request, if one was named.
    pub replacement: Option<ArchiveState>,
    /// False when the requested archive identity already belongs to another episode.
    pub archive_id_available: bool,
}

/// Explicit request to hide a state without deleting it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArchiveRequest {
    /// New archive episode identity.
    pub archive_id: ArchiveId,
    /// Exact state selected by resolution.
    pub state_id: StateId,
    /// User-supplied reason, retained verbatim.
    pub reason: String,
    /// Required safe destination when the target is the current state.
    pub replacement_state: Option<StateId>,
}

/// Typed, ordered effects for an archive operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ArchiveEffect {
    /// Prove the state identity and its reachability anchor remain retained.
    EnsureStateRetained {
        /// Identity which must continue to resolve after archival.
        state_id: StateId,
    },
    /// Move the current selection before hiding its former target.
    RelocateCurrent {
        /// Current state being left.
        from_state: StateId,
        /// Explicit visible replacement.
        to_state: StateId,
        /// Replacement's semantic attempt.
        to_attempt: AttemptId,
    },
    /// Open an append-only archive episode with the exact recovery context.
    RecordArchiveEpisode {
        /// Archive episode identity.
        archive_id: ArchiveId,
        /// Topology which recovery must later verify and retain.
        original_topology: StateTopology,
        /// Verbatim user reason.
        reason: String,
    },
    /// Change only the visibility projection; topology and identity are untouched.
    SetStateVisibility {
        /// Stable state identity.
        state_id: StateId,
        /// True for archive and false for recovery.
        archived: bool,
    },
}

/// Complete reversible archive plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArchivePlan {
    /// Episode which owns the future recovery.
    pub archive_id: ArchiveId,
    /// Immutable topology carried into the archive episode.
    pub retained_topology: StateTopology,
    /// Ordered effects. No hard-delete effect exists in this API.
    pub effects: Vec<ArchiveEffect>,
}

/// Facts from the open episode needed to plan recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpenArchiveEpisode {
    /// Open episode identity.
    pub archive_id: ArchiveId,
    /// Archived state identity.
    pub state_id: StateId,
    /// Topology captured before the state was hidden.
    pub original_topology: StateTopology,
    /// Whether the episode is still open.
    pub open: bool,
}

/// Facts frozen by the caller before planning recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecoverContext {
    /// Current committed state projection.
    pub target: ArchiveState,
    /// Episode selected for recovery.
    pub episode: OpenArchiveEpisode,
}

/// Explicit state recovery request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecoverRequest {
    /// State to reveal under its original identity.
    pub state_id: StateId,
    /// Open episode to close.
    pub archive_id: ArchiveId,
}

/// Typed, ordered effects for recovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RecoverEffect {
    /// Refuse recovery if any topology was severed or silently reassociated.
    VerifyTopologyUnchanged {
        /// Exact topology persisted by the archive episode.
        expected: StateTopology,
    },
    /// Reveal the same state identity; this creates no replacement state.
    SetStateVisibility {
        /// Stable state identity.
        state_id: StateId,
        /// Always false in a recovery plan.
        archived: bool,
    },
    /// Close the append-only episode after restoring visibility.
    CloseArchiveEpisode {
        /// Episode being closed.
        archive_id: ArchiveId,
        /// State whose visibility was restored.
        state_id: StateId,
        /// Exact logical-parent and attempt context retained throughout.
        restored_topology: StateTopology,
    },
}

/// Complete reversible recovery plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecoverPlan {
    /// Episode closed by this recovery.
    pub archive_id: ArchiveId,
    /// Exact original topology restored to visible projections.
    pub restored_topology: StateTopology,
    /// Ordered effects. Recovery reveals; it never recreates or reassigns identity.
    pub effects: Vec<RecoverEffect>,
}

/// Archive or recovery planning failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArchivePlanError {
    /// Resolved state and requested state disagree.
    #[error("resolved archive target does not match the requested state")]
    TargetMismatch,
    /// Archive reasons are durable audit facts and cannot be blank.
    #[error("archive reason must not be empty")]
    EmptyReason,
    /// The archive identity is already owned by another lifecycle.
    #[error("archive identity is already in use")]
    ArchiveIdInUse,
    /// Hiding a hidden state would create overlapping episodes.
    #[error("state is already archived")]
    AlreadyArchived,
    /// A visible state cannot already have an open archive episode.
    #[error("visible state has an inconsistent open archive episode")]
    InconsistentArchiveState,
    /// Hiding the workspace's current identity requires an explicit destination.
    #[error("archiving the current state requires an explicit replacement")]
    CurrentStateRequiresReplacement,
    /// A replacement is meaningful only for relocating the current state.
    #[error("replacement is only valid when archiving the current state")]
    UnexpectedReplacement,
    /// The requested replacement was not resolved in the frozen context.
    #[error("explicit replacement state is unavailable")]
    ReplacementUnavailable,
    /// A state cannot replace itself while it is being hidden.
    #[error("archive replacement must differ from the target state")]
    ReplacementIsTarget,
    /// An archived identity cannot become the visible current replacement.
    #[error("archive replacement must be visible")]
    ReplacementArchived,
    /// Current state and current attempt facts disagree with committed topology.
    #[error("current selection is inconsistent with the target topology")]
    InconsistentCurrentSelection,
    /// Recovery requires a hidden state.
    #[error("state is not archived")]
    NotArchived,
    /// Recovery must close the state projection's exact open episode.
    #[error("requested archive episode is not active for this state")]
    ArchiveEpisodeMismatch,
    /// A closed episode cannot be recovered twice.
    #[error("archive episode is already closed")]
    ArchiveEpisodeClosed,
    /// Recovery never guesses after topology has changed.
    #[error("archived state topology differs from its recorded recovery context")]
    TopologyChanged,
}

/// Plans a reversible visibility overlay, relocating a current target only when the
/// caller explicitly names a visible replacement.
pub fn plan_archive(
    context: &ArchiveContext,
    request: ArchiveRequest,
) -> Result<ArchivePlan, ArchivePlanError> {
    if context.target.topology.state_id != request.state_id {
        return Err(ArchivePlanError::TargetMismatch);
    }
    if request.reason.trim().is_empty() {
        return Err(ArchivePlanError::EmptyReason);
    }
    if !context.archive_id_available {
        return Err(ArchivePlanError::ArchiveIdInUse);
    }
    if context.target.archived {
        return Err(ArchivePlanError::AlreadyArchived);
    }
    if context.target.active_archive.is_some() {
        return Err(ArchivePlanError::InconsistentArchiveState);
    }

    let target_is_current = context
        .current
        .is_some_and(|current| current.state_id == request.state_id);
    let relocation = match (target_is_current, request.replacement_state) {
        (true, None) => return Err(ArchivePlanError::CurrentStateRequiresReplacement),
        (false, Some(_)) => return Err(ArchivePlanError::UnexpectedReplacement),
        (false, None) => None,
        (true, Some(replacement_id)) => {
            let current = context
                .current
                .expect("target_is_current proves a current selection");
            if current.attempt_id != context.target.topology.attempt_id {
                return Err(ArchivePlanError::InconsistentCurrentSelection);
            }
            if replacement_id == request.state_id {
                return Err(ArchivePlanError::ReplacementIsTarget);
            }
            let replacement = context
                .replacement
                .filter(|state| state.topology.state_id == replacement_id)
                .ok_or(ArchivePlanError::ReplacementUnavailable)?;
            if replacement.archived || replacement.active_archive.is_some() {
                return Err(ArchivePlanError::ReplacementArchived);
            }
            Some(ArchiveEffect::RelocateCurrent {
                from_state: request.state_id,
                to_state: replacement_id,
                to_attempt: replacement.topology.attempt_id,
            })
        }
    };

    let topology = context.target.topology;
    let mut effects = vec![ArchiveEffect::EnsureStateRetained {
        state_id: request.state_id,
    }];
    if let Some(relocation) = relocation {
        effects.push(relocation);
    }
    effects.extend([
        ArchiveEffect::RecordArchiveEpisode {
            archive_id: request.archive_id,
            original_topology: topology,
            reason: request.reason,
        },
        ArchiveEffect::SetStateVisibility {
            state_id: request.state_id,
            archived: true,
        },
    ]);

    Ok(ArchivePlan {
        archive_id: request.archive_id,
        retained_topology: topology,
        effects,
    })
}

/// Plans recovery only when the open episode and current topology still match exactly.
pub fn plan_recover(
    context: &RecoverContext,
    request: RecoverRequest,
) -> Result<RecoverPlan, ArchivePlanError> {
    if context.target.topology.state_id != request.state_id
        || context.episode.state_id != request.state_id
        || context.episode.original_topology.state_id != request.state_id
    {
        return Err(ArchivePlanError::TargetMismatch);
    }
    if !context.target.archived {
        return Err(ArchivePlanError::NotArchived);
    }
    if !context.episode.open {
        return Err(ArchivePlanError::ArchiveEpisodeClosed);
    }
    if context.target.active_archive != Some(request.archive_id)
        || context.episode.archive_id != request.archive_id
    {
        return Err(ArchivePlanError::ArchiveEpisodeMismatch);
    }
    if context.target.topology != context.episode.original_topology {
        return Err(ArchivePlanError::TopologyChanged);
    }

    let topology = context.episode.original_topology;
    Ok(RecoverPlan {
        archive_id: request.archive_id,
        restored_topology: topology,
        effects: vec![
            RecoverEffect::VerifyTopologyUnchanged { expected: topology },
            RecoverEffect::SetStateVisibility {
                state_id: request.state_id,
                archived: false,
            },
            RecoverEffect::CloseArchiveEpisode {
                archive_id: request.archive_id,
                state_id: request.state_id,
                restored_topology: topology,
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topology(
        state_id: StateId,
        parent: Option<StateId>,
        attempt_id: AttemptId,
    ) -> StateTopology {
        StateTopology {
            state_id,
            logical_parent: parent,
            attempt_id,
        }
    }

    fn archive_context(target: StateTopology) -> ArchiveContext {
        ArchiveContext {
            target: ArchiveState {
                topology: target,
                archived: false,
                active_archive: None,
            },
            current: None,
            replacement: None,
            archive_id_available: true,
        }
    }

    #[test]
    fn archive_hides_identity_without_erasing_or_rewriting_topology() {
        let parent = StateId::new_v7();
        let state = StateId::new_v7();
        let attempt = AttemptId::new_v7();
        let original = topology(state, Some(parent), attempt);
        let archive_id = ArchiveId::new_v7();

        let plan = plan_archive(
            &archive_context(original),
            ArchiveRequest {
                archive_id,
                state_id: state,
                reason: "superseded experiment".into(),
                replacement_state: None,
            },
        )
        .unwrap();

        assert_eq!(plan.retained_topology, original);
        assert_eq!(
            plan.effects,
            vec![
                ArchiveEffect::EnsureStateRetained { state_id: state },
                ArchiveEffect::RecordArchiveEpisode {
                    archive_id,
                    original_topology: original,
                    reason: "superseded experiment".into(),
                },
                ArchiveEffect::SetStateVisibility {
                    state_id: state,
                    archived: true
                },
            ]
        );
    }

    #[test]
    fn current_state_requires_explicit_visible_replacement_before_hiding() {
        let state = StateId::new_v7();
        let attempt = AttemptId::new_v7();
        let original = topology(state, None, attempt);
        let mut context = archive_context(original);
        context.current = Some(CurrentSelection {
            state_id: state,
            attempt_id: attempt,
        });

        let error = plan_archive(
            &context,
            ArchiveRequest {
                archive_id: ArchiveId::new_v7(),
                state_id: state,
                reason: "park this".into(),
                replacement_state: None,
            },
        )
        .unwrap_err();
        assert_eq!(error, ArchivePlanError::CurrentStateRequiresReplacement);

        let replacement = StateId::new_v7();
        let replacement_attempt = AttemptId::new_v7();
        context.replacement = Some(ArchiveState {
            topology: topology(replacement, Some(state), replacement_attempt),
            archived: false,
            active_archive: None,
        });
        let plan = plan_archive(
            &context,
            ArchiveRequest {
                archive_id: ArchiveId::new_v7(),
                state_id: state,
                reason: "park this".into(),
                replacement_state: Some(replacement),
            },
        )
        .unwrap();

        assert!(matches!(
            plan.effects.as_slice(),
            [
                ArchiveEffect::EnsureStateRetained { .. },
                ArchiveEffect::RelocateCurrent { from_state, to_state, to_attempt },
                ArchiveEffect::RecordArchiveEpisode { .. },
                ArchiveEffect::SetStateVisibility { archived: true, .. },
            ] if *from_state == state && *to_state == replacement && *to_attempt == replacement_attempt
        ));
    }

    #[test]
    fn archive_rejects_hidden_targets_and_hidden_replacements() {
        let state = StateId::new_v7();
        let attempt = AttemptId::new_v7();
        let archive_id = ArchiveId::new_v7();
        let mut hidden = archive_context(topology(state, None, attempt));
        hidden.target.archived = true;
        hidden.target.active_archive = Some(ArchiveId::new_v7());
        assert_eq!(
            plan_archive(
                &hidden,
                ArchiveRequest {
                    archive_id,
                    state_id: state,
                    reason: "already hidden".into(),
                    replacement_state: None,
                },
            ),
            Err(ArchivePlanError::AlreadyArchived)
        );

        let replacement = StateId::new_v7();
        let mut current = archive_context(topology(state, None, attempt));
        current.current = Some(CurrentSelection {
            state_id: state,
            attempt_id: attempt,
        });
        current.replacement = Some(ArchiveState {
            topology: topology(replacement, None, AttemptId::new_v7()),
            archived: true,
            active_archive: Some(ArchiveId::new_v7()),
        });
        assert_eq!(
            plan_archive(
                &current,
                ArchiveRequest {
                    archive_id,
                    state_id: state,
                    reason: "relocate safely".into(),
                    replacement_state: Some(replacement),
                },
            ),
            Err(ArchivePlanError::ReplacementArchived)
        );
    }

    #[test]
    fn recover_restores_exact_logical_parent_and_attempt_context() {
        let parent = StateId::new_v7();
        let state = StateId::new_v7();
        let attempt = AttemptId::new_v7();
        let original = topology(state, Some(parent), attempt);
        let archive_id = ArchiveId::new_v7();
        let context = RecoverContext {
            target: ArchiveState {
                topology: original,
                archived: true,
                active_archive: Some(archive_id),
            },
            episode: OpenArchiveEpisode {
                archive_id,
                state_id: state,
                original_topology: original,
                open: true,
            },
        };

        let plan = plan_recover(
            &context,
            RecoverRequest {
                state_id: state,
                archive_id,
            },
        )
        .unwrap();

        assert_eq!(plan.restored_topology.logical_parent, Some(parent));
        assert_eq!(plan.restored_topology.attempt_id, attempt);
        assert_eq!(
            plan.effects.last(),
            Some(&RecoverEffect::CloseArchiveEpisode {
                archive_id,
                state_id: state,
                restored_topology: original,
            })
        );
    }

    #[test]
    fn recover_rejects_topology_reassociation_instead_of_guessing() {
        let state = StateId::new_v7();
        let attempt = AttemptId::new_v7();
        let original = topology(state, Some(StateId::new_v7()), attempt);
        let archive_id = ArchiveId::new_v7();
        let context = RecoverContext {
            target: ArchiveState {
                topology: topology(state, Some(StateId::new_v7()), attempt),
                archived: true,
                active_archive: Some(archive_id),
            },
            episode: OpenArchiveEpisode {
                archive_id,
                state_id: state,
                original_topology: original,
                open: true,
            },
        };

        assert_eq!(
            plan_recover(
                &context,
                RecoverRequest {
                    state_id: state,
                    archive_id
                }
            ),
            Err(ArchivePlanError::TopologyChanged)
        );
    }
}

//! Pure planning for exact atomic picks and their explicit conflict continuation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    ArtifactRef, AttemptId, CompositionId, DeltaId, GitObjectId, NativePath, ProvenanceId, StateId,
    WorkspaceId,
};

/// Identity of the only delta an atomic pick may apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExactDeltaIdentity {
    /// Stable identity allocated for the canonical parent-to-state delta.
    pub delta_id: DeltaId,
    /// Logical parent of the source state; never an inferred Git ancestor.
    pub source_parent_state_id: StateId,
    /// Source state whose direct change is being selected.
    pub source_state_id: StateId,
}

/// Frozen semantic inputs and reserved output identities for one atomic pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PickRequestIdentity {
    /// Composition record owning the pick provenance.
    pub composition_id: CompositionId,
    /// Exact source parent-to-source delta.
    pub source_delta: ExactDeltaIdentity,
    /// Attempt that owns the source state and must not be advanced by this pick.
    pub source_attempt_id: AttemptId,
    /// Explicit state onto which the exact delta is applied.
    pub target_base_state_id: StateId,
    /// Attempt advanced only after a result has been materialized and verified.
    pub target_attempt_id: AttemptId,
    /// Identity reserved for the resulting state.
    pub result_state_id: StateId,
    /// Identity reserved for the result's complete pick provenance.
    pub result_provenance_id: ProvenanceId,
}

/// Adapter-observed result of applying the exact delta in isolation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PickApplicationOutcome {
    /// The exact delta produced a materializable result tree without conflicts.
    Applied {
        /// Verified Git tree for the result state.
        result_tree: GitObjectId,
    },
    /// Applying the exact delta produced conflicts and no semantic result yet.
    Conflicted {
        /// Isolated workspace containing the conflict, never the source or target workspace.
        resolution_workspace_id: WorkspaceId,
        /// Durable description/materialization of the unresolved conflict.
        conflict_artifact: ArtifactRef,
        /// Conflicting paths. The planner canonicalizes these by native-path ordering.
        conflicting_paths: Vec<NativePath>,
    },
}

/// Atomic-pick intent after all semantic identities have been resolved and frozen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PickRequest {
    /// Frozen semantic identities.
    pub identity: PickRequestIdentity,
    /// Outcome observed by the isolated exact-delta engine.
    pub outcome: PickApplicationOutcome,
}

/// Typed effects for the initial atomic-pick operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PickEffect {
    /// Derive exactly the source logical-parent to source-state delta.
    DeriveExactDelta { identity: ExactDeltaIdentity },
    /// Apply that exact delta to the explicit target base, not source ancestry.
    ApplyExactDelta {
        identity: ExactDeltaIdentity,
        target_base_state_id: StateId,
    },
    /// Create the result with the target base as its sole semantic/Git parent.
    CreateResultState {
        result_state_id: StateId,
        result_tree: GitObjectId,
        sole_parent_state_id: StateId,
        attempt_id: AttemptId,
    },
    /// Record complete derivation as provenance, never as ancestry.
    RecordPickProvenance {
        provenance_id: ProvenanceId,
        composition_id: CompositionId,
        identity: ExactDeltaIdentity,
        target_base_state_id: StateId,
        result_state_id: StateId,
        resolution_artifact: Option<ArtifactRef>,
    },
    /// Advance only the target attempt after the result and provenance exist.
    AdvanceTargetAttempt {
        attempt_id: AttemptId,
        expected_tip_state_id: StateId,
        result_state_id: StateId,
    },
    /// Materialize conflict facts in the isolated resolution workspace.
    MaterializeConflict {
        resolution_workspace_id: WorkspaceId,
        conflict_artifact: ArtifactRef,
        conflicting_paths: Vec<NativePath>,
    },
    /// Pause without manufacturing a result or guessing a conflict decision.
    AwaitExplicitResolution {
        composition_id: CompositionId,
        resolution_workspace_id: WorkspaceId,
    },
}

/// Frozen continuation required to turn a conflict into a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PickResolutionContinuation {
    /// All initial semantic identities; re-resolution is forbidden during continuation.
    pub identity: PickRequestIdentity,
    /// Workspace in which the conflict was isolated.
    pub resolution_workspace_id: WorkspaceId,
    /// Original unresolved-conflict artifact.
    pub conflict_artifact: ArtifactRef,
    /// Canonically ordered conflicting paths requiring explicit decisions.
    pub conflicting_paths: Vec<NativePath>,
}

/// Initial plan outcome. A conflict carries no result state effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PickPlanOutcome {
    /// The result can be materialized by the ordered effects.
    Applied {
        result_state_id: StateId,
        result_provenance_id: ProvenanceId,
    },
    /// The operation must stop until an explicit resolution is supplied.
    AwaitingResolution {
        continuation: PickResolutionContinuation,
    },
}

/// Complete initial atomic-pick plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PickPlan {
    /// Frozen request identities.
    pub identity: PickRequestIdentity,
    /// Applied or conflict-paused outcome.
    pub outcome: PickPlanOutcome,
    /// Ordered typed effects.
    pub effects: Vec<PickEffect>,
}

/// Explicit user/agent resolution of a previously paused pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PickResolution {
    /// Durable artifact containing every explicit conflict decision.
    pub resolution_artifact: ArtifactRef,
    /// Verified Git tree produced from those decisions.
    pub resolved_tree: GitObjectId,
}

/// Typed effects that are valid only after explicit conflict resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PickResolutionEffect {
    /// Apply the supplied decisions to the frozen conflict and inspect the exact tree.
    ApplyResolution {
        resolution_workspace_id: WorkspaceId,
        conflict_artifact: ArtifactRef,
        resolution_artifact: ArtifactRef,
        resolved_tree: GitObjectId,
    },
    /// Create the result with the original target base as its sole parent.
    CreateResultState {
        result_state_id: StateId,
        result_tree: GitObjectId,
        sole_parent_state_id: StateId,
        attempt_id: AttemptId,
    },
    /// Record exact source delta, target base, decision artifact, and result provenance.
    RecordPickProvenance {
        provenance_id: ProvenanceId,
        composition_id: CompositionId,
        identity: ExactDeltaIdentity,
        target_base_state_id: StateId,
        result_state_id: StateId,
        resolution_artifact: ArtifactRef,
    },
    /// Advance only the target attempt after the resolved result exists.
    AdvanceTargetAttempt {
        attempt_id: AttemptId,
        expected_tip_state_id: StateId,
        result_state_id: StateId,
    },
}

/// Complete continuation plan for an explicitly resolved conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PickResolutionPlan {
    /// Original frozen identities.
    pub identity: PickRequestIdentity,
    /// Ordered resolution effects.
    pub effects: Vec<PickResolutionEffect>,
}

/// Atomic-pick planning failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PickPlanError {
    /// A direct source delta cannot have identical endpoints.
    #[error("pick source parent and source state must differ")]
    SourceParentEqualsSource,
    /// A result must be a new semantic state, not the selected source state.
    #[error("pick source and result state must differ")]
    SourceEqualsResult,
    /// A result cannot reuse the source-parent identity.
    #[error("pick result state must differ from the source parent")]
    ResultEqualsSourceParent,
    /// A materialized result cannot reuse its target-base identity.
    #[error("pick result state must differ from the target base")]
    ResultEqualsTargetBase,
    /// Conflict continuation requires at least one exact path to resolve.
    #[error("conflicting pick must name at least one conflicting path")]
    EmptyConflictSet,
}

/// Plans one exact atomic pick, stopping conflicts before any semantic result is created.
pub fn plan_pick(request: PickRequest) -> Result<PickPlan, PickPlanError> {
    validate_identity(request.identity)?;

    let identity = request.identity;
    let mut effects = vec![
        PickEffect::DeriveExactDelta {
            identity: identity.source_delta,
        },
        PickEffect::ApplyExactDelta {
            identity: identity.source_delta,
            target_base_state_id: identity.target_base_state_id,
        },
    ];

    let outcome = match request.outcome {
        PickApplicationOutcome::Applied { result_tree } => {
            effects.extend([
                PickEffect::CreateResultState {
                    result_state_id: identity.result_state_id,
                    result_tree: result_tree.clone(),
                    sole_parent_state_id: identity.target_base_state_id,
                    attempt_id: identity.target_attempt_id,
                },
                PickEffect::RecordPickProvenance {
                    provenance_id: identity.result_provenance_id,
                    composition_id: identity.composition_id,
                    identity: identity.source_delta,
                    target_base_state_id: identity.target_base_state_id,
                    result_state_id: identity.result_state_id,
                    resolution_artifact: None,
                },
                PickEffect::AdvanceTargetAttempt {
                    attempt_id: identity.target_attempt_id,
                    expected_tip_state_id: identity.target_base_state_id,
                    result_state_id: identity.result_state_id,
                },
            ]);
            PickPlanOutcome::Applied {
                result_state_id: identity.result_state_id,
                result_provenance_id: identity.result_provenance_id,
            }
        }
        PickApplicationOutcome::Conflicted {
            resolution_workspace_id,
            conflict_artifact,
            mut conflicting_paths,
        } => {
            canonicalize_conflicts(&mut conflicting_paths)?;
            effects.extend([
                PickEffect::MaterializeConflict {
                    resolution_workspace_id,
                    conflict_artifact: conflict_artifact.clone(),
                    conflicting_paths: conflicting_paths.clone(),
                },
                PickEffect::AwaitExplicitResolution {
                    composition_id: identity.composition_id,
                    resolution_workspace_id,
                },
            ]);
            PickPlanOutcome::AwaitingResolution {
                continuation: PickResolutionContinuation {
                    identity,
                    resolution_workspace_id,
                    conflict_artifact,
                    conflicting_paths,
                },
            }
        }
    };

    Ok(PickPlan {
        identity,
        outcome,
        effects,
    })
}

/// Plans the only path from a paused conflict to a semantic result.
pub fn plan_pick_resolution(
    continuation: PickResolutionContinuation,
    resolution: PickResolution,
) -> Result<PickResolutionPlan, PickPlanError> {
    validate_identity(continuation.identity)?;
    let mut conflicting_paths = continuation.conflicting_paths;
    canonicalize_conflicts(&mut conflicting_paths)?;

    let identity = continuation.identity;
    Ok(PickResolutionPlan {
        identity,
        effects: vec![
            PickResolutionEffect::ApplyResolution {
                resolution_workspace_id: continuation.resolution_workspace_id,
                conflict_artifact: continuation.conflict_artifact,
                resolution_artifact: resolution.resolution_artifact.clone(),
                resolved_tree: resolution.resolved_tree.clone(),
            },
            PickResolutionEffect::CreateResultState {
                result_state_id: identity.result_state_id,
                result_tree: resolution.resolved_tree,
                sole_parent_state_id: identity.target_base_state_id,
                attempt_id: identity.target_attempt_id,
            },
            PickResolutionEffect::RecordPickProvenance {
                provenance_id: identity.result_provenance_id,
                composition_id: identity.composition_id,
                identity: identity.source_delta,
                target_base_state_id: identity.target_base_state_id,
                result_state_id: identity.result_state_id,
                resolution_artifact: resolution.resolution_artifact,
            },
            PickResolutionEffect::AdvanceTargetAttempt {
                attempt_id: identity.target_attempt_id,
                expected_tip_state_id: identity.target_base_state_id,
                result_state_id: identity.result_state_id,
            },
        ],
    })
}

fn validate_identity(identity: PickRequestIdentity) -> Result<(), PickPlanError> {
    let source_parent = identity.source_delta.source_parent_state_id;
    let source = identity.source_delta.source_state_id;
    let result = identity.result_state_id;
    if source_parent == source {
        return Err(PickPlanError::SourceParentEqualsSource);
    }
    if source == result {
        return Err(PickPlanError::SourceEqualsResult);
    }
    if source_parent == result {
        return Err(PickPlanError::ResultEqualsSourceParent);
    }
    if identity.target_base_state_id == result {
        return Err(PickPlanError::ResultEqualsTargetBase);
    }
    Ok(())
}

fn canonicalize_conflicts(paths: &mut Vec<NativePath>) -> Result<(), PickPlanError> {
    if paths.is_empty() {
        return Err(PickPlanError::EmptyConflictSet);
    }
    paths.sort();
    paths.dedup();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ArtifactId, Hash256, ObjectAlgorithm};

    fn oid(byte: u8) -> GitObjectId {
        GitObjectId::new(ObjectAlgorithm::Sha1, vec![byte; 20]).unwrap()
    }

    fn artifact(byte: u8) -> ArtifactRef {
        ArtifactRef {
            id: ArtifactId::new_v7(),
            sha256: Hash256([byte; 32]),
            byte_length: 32,
            media_type: "application/vnd.jjk.pick-resolution".into(),
        }
    }

    fn snake_identity() -> PickRequestIdentity {
        let purple_slow = StateId::new_v7();
        let purple_fast = StateId::new_v7();
        PickRequestIdentity {
            composition_id: CompositionId::new_v7(),
            source_delta: ExactDeltaIdentity {
                delta_id: DeltaId::new_v7(),
                source_parent_state_id: purple_slow,
                source_state_id: purple_fast,
            },
            source_attempt_id: AttemptId::new_v7(),
            target_base_state_id: StateId::new_v7(), // orange, slow
            target_attempt_id: AttemptId::new_v7(),
            result_state_id: StateId::new_v7(), // orange, fast
            result_provenance_id: ProvenanceId::new_v7(),
        }
    }

    #[test]
    fn orange_pick_applies_only_the_parent_to_fast_delta() {
        let identity = snake_identity();
        let plan = plan_pick(PickRequest {
            identity,
            outcome: PickApplicationOutcome::Applied {
                result_tree: oid(4),
            },
        })
        .unwrap();

        assert_eq!(
            plan.effects[0],
            PickEffect::DeriveExactDelta {
                identity: identity.source_delta,
            }
        );
        assert_eq!(
            plan.effects[1],
            PickEffect::ApplyExactDelta {
                identity: identity.source_delta,
                target_base_state_id: identity.target_base_state_id,
            }
        );
        assert!(matches!(
            &plan.effects[2],
            PickEffect::CreateResultState {
                result_state_id,
                sole_parent_state_id,
                attempt_id,
                ..
            } if *result_state_id == identity.result_state_id
                && *sole_parent_state_id == identity.target_base_state_id
                && *attempt_id == identity.target_attempt_id
        ));
        assert!(!plan.effects.iter().any(|effect| matches!(
            effect,
            PickEffect::CreateResultState { sole_parent_state_id, .. }
                if *sole_parent_state_id == identity.source_delta.source_parent_state_id
                    || *sole_parent_state_id == identity.source_delta.source_state_id
        )));
    }

    #[test]
    fn successful_pick_advances_target_but_leaves_source_attempt_unchanged() {
        let identity = snake_identity();
        let plan = plan_pick(PickRequest {
            identity,
            outcome: PickApplicationOutcome::Applied {
                result_tree: oid(5),
            },
        })
        .unwrap();

        let advanced = plan
            .effects
            .iter()
            .filter_map(|effect| match effect {
                PickEffect::AdvanceTargetAttempt { attempt_id, .. } => Some(*attempt_id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(advanced, vec![identity.target_attempt_id]);
        assert!(!advanced.contains(&identity.source_attempt_id));
        assert_eq!(plan.identity.source_attempt_id, identity.source_attempt_id);
    }

    #[test]
    fn conflicting_pick_requires_explicit_resolution_before_result_effects() {
        let identity = snake_identity();
        let workspace = WorkspaceId::new_v7();
        let conflict_artifact = artifact(7);
        let plan = plan_pick(PickRequest {
            identity,
            outcome: PickApplicationOutcome::Conflicted {
                resolution_workspace_id: workspace,
                conflict_artifact: conflict_artifact.clone(),
                conflicting_paths: vec![NativePath::unix(b"mode".to_vec()).unwrap()],
            },
        })
        .unwrap();

        assert!(!plan.effects.iter().any(|effect| matches!(
            effect,
            PickEffect::CreateResultState { .. }
                | PickEffect::RecordPickProvenance { .. }
                | PickEffect::AdvanceTargetAttempt { .. }
        )));
        let PickPlanOutcome::AwaitingResolution { continuation } = plan.outcome else {
            panic!("conflict must pause with a continuation");
        };

        let resolution_artifact = artifact(8);
        let continuation_plan = plan_pick_resolution(
            continuation,
            PickResolution {
                resolution_artifact: resolution_artifact.clone(),
                resolved_tree: oid(9),
            },
        )
        .unwrap();
        assert!(matches!(
            &continuation_plan.effects[0],
            PickResolutionEffect::ApplyResolution { resolution_artifact: actual, .. }
                if *actual == resolution_artifact
        ));
        assert!(matches!(
            &continuation_plan.effects[1],
            PickResolutionEffect::CreateResultState { sole_parent_state_id, .. }
                if *sole_parent_state_id == identity.target_base_state_id
        ));
    }

    #[test]
    fn invalid_or_reused_state_identities_are_rejected() {
        let mut identity = snake_identity();
        identity.source_delta.source_parent_state_id = identity.source_delta.source_state_id;
        assert_eq!(
            plan_pick(PickRequest {
                identity,
                outcome: PickApplicationOutcome::Applied {
                    result_tree: oid(1),
                },
            }),
            Err(PickPlanError::SourceParentEqualsSource)
        );

        let mut identity = snake_identity();
        identity.result_state_id = identity.source_delta.source_state_id;
        assert_eq!(
            plan_pick(PickRequest {
                identity,
                outcome: PickApplicationOutcome::Applied {
                    result_tree: oid(2),
                },
            }),
            Err(PickPlanError::SourceEqualsResult)
        );
    }

    #[test]
    fn conflict_paths_are_canonicalized_and_must_be_nonempty() {
        let identity = snake_identity();
        let workspace = WorkspaceId::new_v7();
        let duplicate = NativePath::unix(b"z".to_vec()).unwrap();
        let first = NativePath::unix(b"a".to_vec()).unwrap();
        let plan = plan_pick(PickRequest {
            identity,
            outcome: PickApplicationOutcome::Conflicted {
                resolution_workspace_id: workspace,
                conflict_artifact: artifact(3),
                conflicting_paths: vec![duplicate.clone(), first.clone(), duplicate],
            },
        })
        .unwrap();
        let PickPlanOutcome::AwaitingResolution { continuation } = plan.outcome else {
            panic!("conflict must pause");
        };
        assert_eq!(
            continuation.conflicting_paths,
            vec![first, NativePath::unix(b"z".to_vec()).unwrap()]
        );

        assert_eq!(
            plan_pick(PickRequest {
                identity,
                outcome: PickApplicationOutcome::Conflicted {
                    resolution_workspace_id: workspace,
                    conflict_artifact: artifact(4),
                    conflicting_paths: vec![],
                },
            }),
            Err(PickPlanError::EmptyConflictSet)
        );
    }
}

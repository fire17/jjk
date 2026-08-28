//! Pure plans for attempts, optional Git branch bindings, worktrees, and lease ownership.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{ActorId, AttemptId, StateId, WorkspaceId};

/// Requested substrate materialization for a semantic attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ForkMaterialization {
    /// Semantic attempt only; Git branch/worktree are not identities and need not exist.
    AttemptOnly,
    /// Bind an ordinary Git branch, but keep the current checkout unchanged.
    Branch { refname: String },
    /// Bind a branch and provision an isolated linked worktree.
    Worktree {
        /// Ordinary branch refname.
        refname: String,
        /// Repository-relative locator chosen by the application policy.
        relative_locator: String,
        /// Workspace identity reserved for the checkout.
        workspace_id: WorkspaceId,
    },
}

/// Fork intent after identifiers are reserved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ForkRequest {
    /// New semantic attempt.
    pub attempt_id: AttemptId,
    /// Exact state at which the sibling begins.
    pub from_state: StateId,
    /// Non-empty objective.
    pub objective: String,
    /// Actor receiving mutation ownership when a worktree is provisioned.
    pub owner: ActorId,
    /// Stable worker/session diagnostic name.
    pub worker: String,
    /// Optional substrate materialization.
    pub materialization: ForkMaterialization,
}

/// Typed fork/worktree effect; adapters translate these, never the planner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ForkEffect {
    /// Record a semantic sibling attempt.
    RecordAttempt {
        attempt_id: AttemptId,
        from_state: StateId,
        objective: String,
    },
    /// Compare-and-swap create/update a branch at the source state's exact object.
    BindBranch {
        attempt_id: AttemptId,
        refname: String,
        target_state: StateId,
    },
    /// Provision a checkout. No cwd claim is made; the result is a typed locator.
    ProvisionWorktree {
        workspace_id: WorkspaceId,
        attempt_id: AttemptId,
        refname: String,
        relative_locator: String,
    },
    /// Establish exclusive workspace ownership/lease after provisioning verifies.
    AcquireWorkspaceLease {
        workspace_id: WorkspaceId,
        owner: ActorId,
        worker: String,
    },
}

/// Pure fork plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ForkPlan {
    /// New attempt identity.
    pub attempt_id: AttemptId,
    /// Original source state remains unchanged.
    pub source_state: StateId,
    /// Path to return or print for an explicit shell/editor handoff.
    pub directory_handoff: Option<String>,
    /// Ordered typed effects.
    pub effects: Vec<ForkEffect>,
}

/// Fork planning error.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ForkPlanError {
    /// Objective is required for future actors.
    #[error("fork objective must not be empty")]
    EmptyObjective,
    /// Branch refname is required when materializing a branch.
    #[error("branch refname must not be empty")]
    EmptyRefname,
    /// Worktree locator must be repository-relative and non-empty.
    #[error("worktree locator must be repository-relative and non-empty")]
    InvalidWorktreeLocator,
    /// Worker identity is required before leasing a worktree.
    #[error("worker identity must not be empty")]
    EmptyWorker,
}

/// Plans a fork without mutating or moving the source checkout.
pub fn plan_fork(request: ForkRequest) -> Result<ForkPlan, ForkPlanError> {
    let objective = request.objective.trim().to_owned();
    if objective.is_empty() {
        return Err(ForkPlanError::EmptyObjective);
    }
    let mut effects = vec![ForkEffect::RecordAttempt {
        attempt_id: request.attempt_id,
        from_state: request.from_state,
        objective,
    }];
    let directory_handoff = match request.materialization {
        ForkMaterialization::AttemptOnly => None,
        ForkMaterialization::Branch { refname } => {
            let refname = require_refname(refname)?;
            effects.push(ForkEffect::BindBranch {
                attempt_id: request.attempt_id,
                refname,
                target_state: request.from_state,
            });
            None
        }
        ForkMaterialization::Worktree {
            refname,
            relative_locator,
            workspace_id,
        } => {
            let refname = require_refname(refname)?;
            let relative_locator = validate_locator(relative_locator)?;
            let worker = request.worker.trim().to_owned();
            if worker.is_empty() {
                return Err(ForkPlanError::EmptyWorker);
            }
            effects.extend([
                ForkEffect::BindBranch {
                    attempt_id: request.attempt_id,
                    refname: refname.clone(),
                    target_state: request.from_state,
                },
                ForkEffect::ProvisionWorktree {
                    workspace_id,
                    attempt_id: request.attempt_id,
                    refname,
                    relative_locator: relative_locator.clone(),
                },
                ForkEffect::AcquireWorkspaceLease {
                    workspace_id,
                    owner: request.owner,
                    worker,
                },
            ]);
            Some(relative_locator)
        }
    };
    Ok(ForkPlan {
        attempt_id: request.attempt_id,
        source_state: request.from_state,
        directory_handoff,
        effects,
    })
}

fn require_refname(refname: String) -> Result<String, ForkPlanError> {
    if refname.trim().is_empty() {
        Err(ForkPlanError::EmptyRefname)
    } else {
        Ok(refname)
    }
}

fn validate_locator(locator: String) -> Result<String, ForkPlanError> {
    let path = std::path::Path::new(&locator);
    let escapes = path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        });
    if locator.trim().is_empty() || escapes {
        Err(ForkPlanError::InvalidWorktreeLocator)
    } else {
        Ok(locator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(materialization: ForkMaterialization) -> ForkRequest {
        ForkRequest {
            attempt_id: AttemptId::new_v7(),
            from_state: StateId::new_v7(),
            objective: "try faster parser".into(),
            owner: ActorId::new_v7(),
            worker: "parser-agent".into(),
            materialization,
        }
    }

    #[test]
    fn semantic_attempt_does_not_prematurely_create_branch() {
        let plan = plan_fork(request(ForkMaterialization::AttemptOnly)).unwrap();
        assert_eq!(plan.effects.len(), 1);
        assert!(matches!(plan.effects[0], ForkEffect::RecordAttempt { .. }));
        assert_eq!(plan.directory_handoff, None);
    }

    #[test]
    fn worktree_plan_leases_isolated_checkout_and_returns_handoff_path() {
        let source = StateId::new_v7();
        let mut input = request(ForkMaterialization::Worktree {
            refname: "jjk/parser-fast".into(),
            relative_locator: ".jjk/worktrees/parser-fast".into(),
            workspace_id: WorkspaceId::new_v7(),
        });
        input.from_state = source;
        let plan = plan_fork(input).unwrap();
        assert_eq!(plan.source_state, source);
        assert_eq!(plan.directory_handoff.as_deref(), Some(".jjk/worktrees/parser-fast"));
        assert!(plan.effects.iter().any(|effect| matches!(effect, ForkEffect::AcquireWorkspaceLease { .. })));
        assert!(plan.effects.iter().all(|effect| !matches!(effect, ForkEffect::RecordAttempt { from_state, .. } if *from_state != source)));
    }
}

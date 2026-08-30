//! Pure plans for semantic forks, ordinary branches, isolated worktrees, and leases.

use crate::domain::{
    ActorId, AttemptId, GitBranchRef, RepoRelativePath, StateId, WorkerId, WorkspaceId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ForkMaterialization {
    AttemptOnly,
    Branch {
        refname: GitBranchRef,
    },
    Worktree {
        refname: GitBranchRef,
        relative_locator: RepoRelativePath,
        workspace_id: WorkspaceId,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ForkRequest {
    pub attempt_id: AttemptId,
    pub from_state: StateId,
    pub objective: String,
    pub owner: ActorId,
    pub worker_id: Option<WorkerId>,
    pub materialization: ForkMaterialization,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ForkEffect {
    RecordAttempt {
        attempt_id: AttemptId,
        from_state: StateId,
        objective: String,
    },
    BindBranchCas {
        attempt_id: AttemptId,
        refname: GitBranchRef,
        target_state: StateId,
        expected_absent: bool,
    },
    ProvisionWorktree {
        workspace_id: WorkspaceId,
        attempt_id: AttemptId,
        refname: GitBranchRef,
        relative_locator: RepoRelativePath,
        target_state: StateId,
    },
    AcquireWorkspaceLease {
        workspace_id: WorkspaceId,
        owner: ActorId,
        worker_id: WorkerId,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DirectoryHandoffPlan {
    pub workspace_id: WorkspaceId,
    pub relative_locator: RepoRelativePath,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ForkPlan {
    pub attempt_id: AttemptId,
    pub source_state: StateId,
    pub source_checkout_mutated: bool,
    pub directory_handoff: Option<DirectoryHandoffPlan>,
    pub effects: Vec<ForkEffect>,
}
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ForkPlanError {
    #[error("fork objective must not be empty")]
    EmptyObjective,
    #[error("a worktree fork requires a stable worker ID")]
    MissingWorker,
}

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
            effects.push(ForkEffect::BindBranchCas {
                attempt_id: request.attempt_id,
                refname,
                target_state: request.from_state,
                expected_absent: true,
            });
            None
        }
        ForkMaterialization::Worktree {
            refname,
            relative_locator,
            workspace_id,
        } => {
            let worker_id = request.worker_id.ok_or(ForkPlanError::MissingWorker)?;
            effects.extend([
                ForkEffect::BindBranchCas {
                    attempt_id: request.attempt_id,
                    refname: refname.clone(),
                    target_state: request.from_state,
                    expected_absent: true,
                },
                ForkEffect::ProvisionWorktree {
                    workspace_id,
                    attempt_id: request.attempt_id,
                    refname,
                    relative_locator: relative_locator.clone(),
                    target_state: request.from_state,
                },
                ForkEffect::AcquireWorkspaceLease {
                    workspace_id,
                    owner: request.owner,
                    worker_id,
                },
            ]);
            Some(DirectoryHandoffPlan {
                workspace_id,
                relative_locator,
            })
        }
    };
    Ok(ForkPlan {
        attempt_id: request.attempt_id,
        source_state: request.from_state,
        source_checkout_mutated: false,
        directory_handoff,
        effects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NativePath;
    fn request(materialization: ForkMaterialization) -> ForkRequest {
        ForkRequest {
            attempt_id: AttemptId::new_v7(),
            from_state: StateId::new_v7(),
            objective: "try parser".into(),
            owner: ActorId::new_v7(),
            worker_id: Some(WorkerId::new_v7()),
            materialization,
        }
    }
    #[test]
    fn attempt_only_has_no_substrate_effect() {
        let p = plan_fork(request(ForkMaterialization::AttemptOnly)).unwrap();
        assert_eq!(p.effects.len(), 1);
        assert!(!p.source_checkout_mutated);
        assert!(p.directory_handoff.is_none())
    }
    #[test]
    fn worktree_is_pinned_to_source_and_does_not_claim_cwd_change() {
        let source = StateId::new_v7();
        let ws = WorkspaceId::new_v7();
        let mut r = request(ForkMaterialization::Worktree {
            refname: GitBranchRef::new(b"refs/heads/jjk/parser".to_vec()).unwrap(),
            relative_locator: RepoRelativePath::new(
                NativePath::unix(b".worktrees/parser".to_vec()).unwrap(),
            )
            .unwrap(),
            workspace_id: ws,
        });
        r.from_state = source;
        let p = plan_fork(r).unwrap();
        assert_eq!(p.source_state, source);
        assert!(!p.source_checkout_mutated);
        assert_eq!(
            p.directory_handoff.as_ref().map(|h| h.workspace_id),
            Some(ws)
        );
        assert!(p.effects.iter().any(
            |e| matches!(e,ForkEffect::ProvisionWorktree{target_state,..}if *target_state==source)
        ));
        assert!(p.effects.iter().any(
            |e| matches!(e,ForkEffect::AcquireWorkspaceLease{workspace_id,..}if *workspace_id==ws)
        ))
    }
    #[test]
    fn worktree_requires_stable_worker() {
        let mut r = request(ForkMaterialization::Worktree {
            refname: GitBranchRef::new(b"refs/heads/jjk/parser".to_vec()).unwrap(),
            relative_locator: RepoRelativePath::new(
                NativePath::unix(b".worktrees/parser".to_vec()).unwrap(),
            )
            .unwrap(),
            workspace_id: WorkspaceId::new_v7(),
        });
        r.worker_id = None;
        assert_eq!(plan_fork(r), Err(ForkPlanError::MissingWorker))
    }
}

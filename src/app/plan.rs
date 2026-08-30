use serde::{Deserialize, Serialize};

use crate::app::query::RepositorySnapshot;
use crate::domain::{AttemptId, StateId};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationMode {
    Return,
    Back,
    Forward,
    Up,
    Down,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NavigationPlan {
    pub based_on_revision: u64,
    pub mode: NavigationMode,
    pub origin_state: Option<StateId>,
    pub origin_attempt: Option<AttemptId>,
    pub target_state: StateId,
    pub target_attempt: AttemptId,
    pub target_was_tip: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NavigationPlanError {
    NoCurrentState,
    NoHistoryTarget,
    NoParent,
    NoChild,
    AmbiguousChildren(Vec<StateId>),
    MissingState(StateId),
}

pub fn plan_navigation(
    snapshot: &RepositorySnapshot,
    mode: NavigationMode,
    explicit: Option<StateId>,
) -> Result<NavigationPlan, NavigationPlanError> {
    let current = snapshot.current_state;
    let target = match mode {
        NavigationMode::Return => explicit.ok_or(NavigationPlanError::NoHistoryTarget)?,
        NavigationMode::Back => history_target(snapshot, -1)?,
        NavigationMode::Forward => history_target(snapshot, 1)?,
        NavigationMode::Up => snapshot
            .state(current.ok_or(NavigationPlanError::NoCurrentState)?)
            .ok_or_else(|| NavigationPlanError::MissingState(current.expect("checked")))?
            .logical_parent
            .ok_or(NavigationPlanError::NoParent)?,
        NavigationMode::Down => {
            let current = current.ok_or(NavigationPlanError::NoCurrentState)?;
            let mut children = snapshot
                .visible_states(false)
                .filter(|state| state.logical_parent == Some(current))
                .map(|state| state.id)
                .collect::<Vec<_>>();
            children.sort();
            match children.as_slice() {
                [] => return Err(NavigationPlanError::NoChild),
                [only] => *only,
                _ => return Err(NavigationPlanError::AmbiguousChildren(children)),
            }
        }
    };
    let state = snapshot
        .state(target)
        .ok_or(NavigationPlanError::MissingState(target))?;
    let target_was_tip = snapshot
        .attempts
        .iter()
        .any(|attempt| attempt.id == state.attempt_id && attempt.tip == target);
    Ok(NavigationPlan {
        based_on_revision: snapshot.revision,
        mode,
        origin_state: current,
        origin_attempt: snapshot.current_attempt,
        target_state: target,
        target_attempt: state.attempt_id,
        target_was_tip,
    })
}

fn history_target(
    snapshot: &RepositorySnapshot,
    delta: isize,
) -> Result<StateId, NavigationPlanError> {
    let index = snapshot
        .navigation
        .index
        .ok_or(NavigationPlanError::NoHistoryTarget)? as isize
        + delta;
    if index < 0 {
        return Err(NavigationPlanError::NoHistoryTarget);
    }
    snapshot
        .navigation
        .entries
        .get(index as usize)
        .copied()
        .ok_or(NavigationPlanError::NoHistoryTarget)
}

use crate::app::plan::{plan_navigation, NavigationMode, NavigationPlan, NavigationPlanError};
use crate::app::query::RepositorySnapshot;
use crate::domain::StateId;

pub fn plan(snapshot: &RepositorySnapshot, mode: NavigationMode, explicit: Option<StateId>) -> Result<NavigationPlan, NavigationPlanError> {
    plan_navigation(snapshot, mode, explicit)
}

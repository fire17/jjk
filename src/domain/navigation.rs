use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use super::{id::{NavigationId,StateId,WorkspaceId},provenance::UtcTimestamp};
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] #[serde(rename_all="kebab-case")] pub enum ActivationMode { Exact, Back, Forward, Parent, Child, Search }
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] pub struct ConfidenceBasisPoints(pub u16);
impl ConfidenceBasisPoints { #[must_use] pub const fn new(value:u16)->Option<Self>{if value<=10_000{Some(Self(value))}else{None}} }
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)] pub struct NavigationCandidate { pub state_id: StateId, pub confidence: ConfidenceBasisPoints, pub reasons: Vec<String> }
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)] pub struct NavigationVisit { pub id: NavigationId, pub state_id: StateId, pub workspace_id: WorkspaceId, pub prior_state_id: Option<StateId>, pub mode: ActivationMode, pub recorded_at: UtcTimestamp }

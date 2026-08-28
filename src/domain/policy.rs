use super::id::ValidationId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum RiskLevel {
    ReadOnly,
    Reversible,
    Destructive,
    External,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SafetyPolicy {
    pub version: u16,
    pub allow_dirty_worktree: bool,
    pub preserve_unique_work: bool,
    pub require_recovery_point: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PromotionPolicy {
    pub name: String,
    pub version: u16,
    pub required_validations: Vec<ValidationId>,
    pub require_expected_old_ref: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RetentionPolicy {
    pub keep_events_forever: bool,
    pub minimum_snapshots: u16,
    pub retain_recovery_days: u32,
}

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use super::{id::{ArtifactId, StateId, ValidationId}, provenance::{Hash256, UtcTimestamp}};
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] pub struct EvidenceRef { pub artifact_id: ArtifactId, pub sha256: Hash256, pub media_type: String, pub byte_length: u64 }
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] #[serde(rename_all="kebab-case")] pub enum ValidationOutcome { Pass, Fail, Error, Skipped }
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)] pub struct ValidationRecord { pub id: ValidationId, pub subject: StateId, pub suite: String, pub outcome: ValidationOutcome, pub evidence: Vec<EvidenceRef>, pub environment_fingerprint: Hash256, pub recorded_at: UtcTimestamp, pub expires_at: Option<UtcTimestamp> }

//! Pure planning for factual agent handoffs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    EvidenceRef, HandoffId, NativePath, RepoRelativePath, ResumeCommand, StateId, WorkspaceHandoff,
    WorkspaceOwner,
};

/// Complete handoff intent after the caller reserves its stable identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HandoffRequest {
    /// Stable identity for this immutable handoff record.
    pub handoff_id: HandoffId,
    /// Actor and optional worker offering the handoff.
    pub owner: WorkspaceOwner,
    /// Non-empty statement of the work's intended outcome.
    pub objective: String,
    /// Exact state from which the work began.
    pub base_state: StateId,
    /// Exact state produced by the work, when one exists.
    pub produced_state: Option<StateId>,
    /// Content-addressed validation evidence supporting the handoff.
    pub evidence: Vec<EvidenceRef>,
    /// Known remaining risks; an empty list means none are declared.
    pub risks: Vec<String>,
    /// Exact program, arguments, and repository-relative working directory for resumption.
    pub resume: ResumeCommand,
}

/// Typed handoff effect consumed by persistence orchestration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum HandoffEffect {
    /// Persist the complete immutable handoff fact.
    RecordHandoff(WorkspaceHandoff),
}

/// Pure handoff plan with one deterministic persistence effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HandoffPlan {
    /// The sole ordered effect. Handoff planning never provisions a worktree.
    pub effects: [HandoffEffect; 1],
}

impl HandoffPlan {
    /// Returns the factual domain record carried by this plan.
    #[must_use]
    pub const fn handoff(&self) -> &WorkspaceHandoff {
        match &self.effects[0] {
            HandoffEffect::RecordHandoff(handoff) => handoff,
        }
    }
}

/// Handoff planning failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HandoffPlanError {
    /// Future actors require an objective rather than an empty note.
    #[error("handoff objective must not be empty")]
    EmptyObjective,
    /// A resume recipe must identify an executable program.
    #[error("handoff resume program must not be empty")]
    EmptyResumeProgram,
    /// The resume directory must remain within the repository.
    #[error("handoff resume cwd must be a non-empty repository-relative path")]
    InvalidResumeCwd,
}

/// Plans an immutable handoff without provisioning or changing any workspace.
pub fn plan_handoff(request: HandoffRequest) -> Result<HandoffPlan, HandoffPlanError> {
    let objective = request.objective.trim().to_owned();
    if objective.is_empty() {
        return Err(HandoffPlanError::EmptyObjective);
    }
    if native_path_is_blank(&request.resume.program) {
        return Err(HandoffPlanError::EmptyResumeProgram);
    }
    if RepoRelativePath::new(request.resume.relative_cwd.clone()).is_err() {
        return Err(HandoffPlanError::InvalidResumeCwd);
    }

    let handoff = WorkspaceHandoff {
        id: request.handoff_id,
        owner: request.owner,
        objective,
        base_state: request.base_state,
        produced_state: request.produced_state,
        validation: request.evidence,
        remaining_risks: request.risks,
        resume: request.resume,
    };
    handoff
        .validate()
        .expect("planner constructs a valid handoff record");

    Ok(HandoffPlan {
        effects: [HandoffEffect::RecordHandoff(handoff)],
    })
}

fn native_path_is_blank(path: &NativePath) -> bool {
    match path {
        NativePath::UnixBytes(units) => {
            units.is_empty() || units.contains(&0) || units.iter().all(u8::is_ascii_whitespace)
        }
        NativePath::WindowsWide(units) => {
            units.is_empty()
                || units.contains(&0)
                || char::decode_utf16(units.iter().copied())
                    .all(|unit| unit.is_ok_and(char::is_whitespace))
        }
    }
}

/// Stable JSON request accepted by the runtime handoff command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RuntimeHandoffRequest {
    pub owner: WorkspaceOwner,
    pub objective: String,
    pub base_state: StateId,
    pub produced_state: Option<StateId>,
    pub validation_ids: Vec<crate::domain::ValidationId>,
    pub remaining_risks: Vec<String>,
    pub resume: RuntimeResumeRequest,
}

/// UTF-8 transport form of a non-executing resume recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RuntimeResumeRequest {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
}

/// Parse and domain-validate a handoff file without accessing or mutating repository state.
pub(crate) fn parse_runtime_handoff(bytes: &[u8]) -> Result<RuntimeHandoffRequest, String> {
    #[derive(Deserialize)]
    struct OwnerWire {
        actor_id: String,
        worker_id: Option<String>,
    }
    #[derive(Deserialize)]
    struct RequestWire {
        owner: OwnerWire,
        objective: String,
        base_state: String,
        produced_state: Option<String>,
        validation_ids: Vec<String>,
        remaining_risks: Vec<String>,
        resume: RuntimeResumeRequest,
    }
    let wire: RequestWire = serde_json::from_slice(bytes)
        .map_err(|error| format!("invalid handoff request JSON: {error}"))?;
    let mut request = RuntimeHandoffRequest {
        owner: WorkspaceOwner {
            actor_id: wire
                .owner
                .actor_id
                .parse()
                .map_err(|error: crate::error::DomainError| error.to_string())?,
            worker_id: wire
                .owner
                .worker_id
                .map(|id| {
                    id.parse()
                        .map_err(|error: crate::error::DomainError| error.to_string())
                })
                .transpose()?,
        },
        objective: wire.objective,
        base_state: wire
            .base_state
            .parse()
            .map_err(|error: crate::error::DomainError| error.to_string())?,
        produced_state: wire
            .produced_state
            .map(|id| {
                id.parse()
                    .map_err(|error: crate::error::DomainError| error.to_string())
            })
            .transpose()?,
        validation_ids: wire
            .validation_ids
            .into_iter()
            .map(|id| {
                id.parse()
                    .map_err(|error: crate::error::DomainError| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?,
        remaining_risks: wire.remaining_risks,
        resume: wire.resume,
    };
    request.objective = request.objective.trim().to_owned();
    if request.objective.is_empty() {
        return Err(HandoffPlanError::EmptyObjective.to_string());
    }
    if request.resume.program.trim().is_empty() || request.resume.program.as_bytes().contains(&0) {
        return Err(HandoffPlanError::EmptyResumeProgram.to_string());
    }
    let cwd = NativePath::unix(request.resume.cwd.as_bytes().to_vec())
        .map_err(|error| error.to_string())?;
    RepoRelativePath::new(cwd).map_err(|_| HandoffPlanError::InvalidResumeCwd.to_string())?;
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ActorId, ArtifactId, Hash256, WorkerId};

    fn native(value: &[u8]) -> NativePath {
        NativePath::unix(value.to_vec()).unwrap()
    }

    fn request() -> HandoffRequest {
        HandoffRequest {
            handoff_id: HandoffId::new_v7(),
            owner: WorkspaceOwner {
                actor_id: ActorId::new_v7(),
                worker_id: Some(WorkerId::new_v7()),
            },
            objective: "finish the parser cutover".into(),
            base_state: StateId::new_v7(),
            produced_state: Some(StateId::new_v7()),
            evidence: vec![EvidenceRef {
                artifact_id: ArtifactId::new_v7(),
                sha256: Hash256::digest(b"focused test output"),
                media_type: "text/plain".into(),
                byte_length: 19,
            }],
            risks: vec!["full suite has not run".into()],
            resume: ResumeCommand {
                program: native(b"cargo"),
                arguments: vec![b"test".to_vec(), b"parser".to_vec()],
                relative_cwd: native(b"crates/parser"),
            },
        }
    }

    #[test]
    fn plan_and_json_schema_round_trip_preserves_exact_resume_recipe() {
        let planned = plan_handoff(request()).unwrap();
        let json = serde_json::to_vec(&planned).unwrap();
        let decoded: HandoffPlan = serde_json::from_slice(&json).unwrap();
        assert_eq!(decoded, planned);

        let handoff = planned.handoff();
        assert_eq!(handoff.resume.program, native(b"cargo"));
        assert_eq!(
            handoff.resume.arguments,
            vec![b"test".to_vec(), b"parser".to_vec()]
        );
        assert_eq!(handoff.resume.relative_cwd, native(b"crates/parser"));
        assert_eq!(handoff.validation.len(), 1);
    }

    #[test]
    fn empty_or_whitespace_program_is_rejected() {
        for program in [native(b""), native(b" \t\n")] {
            let mut input = request();
            input.resume.program = program;
            assert_eq!(
                plan_handoff(input),
                Err(HandoffPlanError::EmptyResumeProgram)
            );
        }
    }

    #[test]
    fn cwd_must_be_repository_relative_for_both_native_encodings() {
        let invalid = [
            native(b""),
            native(b"/tmp/work"),
            native(b"../work"),
            native(b"crates/../outside"),
            NativePath::windows("C:\\work".encode_utf16().collect()).unwrap(),
            NativePath::windows("..\\work".encode_utf16().collect()).unwrap(),
        ];

        for relative_cwd in invalid {
            let mut input = request();
            input.resume.relative_cwd = relative_cwd;
            assert_eq!(plan_handoff(input), Err(HandoffPlanError::InvalidResumeCwd));
        }
    }

    #[test]
    fn objective_is_required_and_planning_never_provisions_a_worktree() {
        let mut input = request();
        input.objective = " \n".into();
        assert_eq!(plan_handoff(input), Err(HandoffPlanError::EmptyObjective));

        let plan = plan_handoff(request()).unwrap();
        assert!(matches!(plan.effects, [HandoffEffect::RecordHandoff(_)]));
    }

    #[test]
    fn runtime_request_parser_preserves_argv_and_rejects_untyped_or_escaping_input() {
        let input = serde_json::json!({
            "owner":{"actor_id":ActorId::new_v7().to_string(),"worker_id":WorkerId::new_v7().to_string()},
            "objective":" deliver state ","base_state":StateId::new_v7().to_string(),"produced_state":StateId::new_v7().to_string(),
            "validation_ids":[crate::domain::ValidationId::new_v7().to_string()],"remaining_risks":[],
            "resume":{"program":"sh","args":["-c","printf safe"],"cwd":"worktrees/alpha"}
        });
        let parsed = parse_runtime_handoff(&serde_json::to_vec(&input).unwrap()).unwrap();
        assert_eq!(parsed.objective, "deliver state");
        assert_eq!(parsed.resume.args, vec!["-c", "printf safe"]);
        let mut escaping = input.clone();
        escaping["resume"]["cwd"] = serde_json::json!("../outside");
        assert!(
            parse_runtime_handoff(&serde_json::to_vec(&escaping).unwrap())
                .unwrap_err()
                .contains("repository-relative")
        );
        let mut untyped = input;
        untyped["owner"]["actor_id"] = serde_json::json!("01H47G0000E008000000000001");
        assert!(parse_runtime_handoff(&serde_json::to_vec(&untyped).unwrap()).is_err());
    }
}

//! Pure planning for explicit `save`, `step`, and `nice` captures.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{AttemptId, StateId, WorkspaceId};

/// User-facing capture flavor. All flavors use the same safety protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CaptureKind {
    /// Ordinary durable state.
    Save,
    /// Small meaningful checkpoint.
    Step,
    /// A deliberately good, memorable waypoint.
    Nice,
}

impl CaptureKind {
    /// Stable event value.
    pub const fn event_kind(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::Step => "step",
            Self::Nice => "nice",
        }
    }
}

/// Exact workspace facts used by the capture planner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CaptureContext {
    /// Checkout in which the capture occurs.
    pub workspace_id: WorkspaceId,
    /// Semantic state currently restored in that checkout.
    pub current_state: Option<StateId>,
    /// Attempt currently selected by navigation.
    pub current_attempt: AttemptId,
    /// Tip of that attempt before navigation/capture.
    pub current_attempt_tip: Option<StateId>,
    /// Set only when navigation returned to a non-tip state.
    pub historical_return: Option<HistoricalReturn>,
    /// Whether captured repository content differs from the current state's tree.
    pub content: ContentDelta,
}

/// Return context is an observation, not an instruction to create a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HistoricalReturn {
    /// Historical state that was restored.
    pub state_id: StateId,
    /// Attempt containing the old future.
    pub attempt_id: AttemptId,
    /// Existing tip that must remain reachable.
    pub preserved_tip: StateId,
}

/// Content comparison against the restored state's exact tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ContentDelta {
    /// No content divergence has occurred.
    Unchanged,
    /// The capture contains a new tree.
    Changed,
}

/// Capture intent after IDs have been allocated by the application boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CaptureRequest {
    /// Flavor of capture.
    pub kind: CaptureKind,
    /// Non-empty display label.
    pub label: String,
    /// Optional factual detail.
    pub message: Option<String>,
    /// Identity reserved for the resulting state.
    pub state_id: StateId,
    /// Identity reserved for a sibling attempt, used only upon real divergence.
    pub divergence_attempt_id: AttemptId,
}

/// Typed effects consumed by transaction coordination and adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CaptureEffect {
    /// Construct the interoperable commit/tree without moving an ordinary branch.
    CaptureTree {
        /// Logical parent used by the semantic state.
        logical_parent: Option<StateId>,
        /// Stable state kind.
        kind: String,
        /// Non-empty commit subject source.
        label: String,
    },
    /// Retain the resulting object under the JJK-owned state ref.
    RetainState { state_id: StateId },
    /// Create a semantic sibling attempt rooted at the returned state.
    ForkAttempt {
        /// New attempt identity.
        attempt_id: AttemptId,
        /// Shared historical root.
        from_state: StateId,
        /// Old future which must remain untouched.
        preserved_tip: StateId,
    },
    /// Record the immutable semantic state.
    RecordState {
        /// State identity.
        state_id: StateId,
        /// Sole logical parent.
        logical_parent: Option<StateId>,
        /// Owning attempt.
        attempt_id: AttemptId,
        /// Capturing checkout.
        workspace_id: WorkspaceId,
        /// Stable event kind.
        kind: String,
        /// User label.
        label: String,
        /// Optional detail.
        message: Option<String>,
    },
}

/// Pure, complete capture plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CapturePlan {
    /// Attempt receiving the new state.
    pub target_attempt: AttemptId,
    /// Existing future retained by delayed divergence.
    pub preserved_future: Option<StateId>,
    /// No branch is created merely by returning; fork/worktree materialization is separate.
    pub creates_ordinary_branch: bool,
    /// Ordered typed effects.
    pub effects: Vec<CaptureEffect>,
}

/// Capture planning failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapturePlanError {
    /// Labels become commit subjects and cannot be blank.
    #[error("capture label must not be empty")]
    EmptyLabel,
    /// Caller supplied inconsistent historical-return facts.
    #[error("historical return state does not match the current restored state")]
    InconsistentHistoricalReturn,
}

/// Plans a capture while delaying sibling creation until content actually diverges.
pub fn plan_capture(
    context: &CaptureContext,
    request: CaptureRequest,
) -> Result<CapturePlan, CapturePlanError> {
    let label = request.label.trim().to_owned();
    if label.is_empty() {
        return Err(CapturePlanError::EmptyLabel);
    }
    if let Some(returned) = context.historical_return {
        if context.current_state != Some(returned.state_id) {
            return Err(CapturePlanError::InconsistentHistoricalReturn);
        }
    }

    let diverges = context.historical_return.is_some() && context.content == ContentDelta::Changed;
    let target_attempt = if diverges {
        request.divergence_attempt_id
    } else {
        context.current_attempt
    };
    let logical_parent = context.current_state;
    let preserved_future = diverges
        .then(|| {
            context
                .historical_return
                .map(|returned| returned.preserved_tip)
        })
        .flatten();

    let mut effects = vec![CaptureEffect::CaptureTree {
        logical_parent,
        kind: request.kind.event_kind().to_owned(),
        label: label.clone(),
    }];
    if let Some(returned) = context.historical_return.filter(|_| diverges) {
        effects.push(CaptureEffect::ForkAttempt {
            attempt_id: target_attempt,
            from_state: returned.state_id,
            preserved_tip: returned.preserved_tip,
        });
    }
    effects.extend([
        CaptureEffect::RetainState {
            state_id: request.state_id,
        },
        CaptureEffect::RecordState {
            state_id: request.state_id,
            logical_parent,
            attempt_id: target_attempt,
            workspace_id: context.workspace_id,
            kind: request.kind.event_kind().to_owned(),
            label,
            message: request.message.filter(|message| !message.trim().is_empty()),
        },
    ]);

    Ok(CapturePlan {
        target_attempt,
        preserved_future,
        creates_ordinary_branch: false,
        effects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn returned_context(content: ContentDelta) -> (CaptureContext, StateId, StateId, AttemptId) {
        let green = StateId::new_v7();
        let purple = StateId::new_v7();
        let original_attempt = AttemptId::new_v7();
        (
            CaptureContext {
                workspace_id: WorkspaceId::new_v7(),
                current_state: Some(green),
                current_attempt: original_attempt,
                current_attempt_tip: Some(purple),
                historical_return: Some(HistoricalReturn {
                    state_id: green,
                    attempt_id: original_attempt,
                    preserved_tip: purple,
                }),
                content,
            },
            green,
            purple,
            original_attempt,
        )
    }

    #[test]
    fn navigation_and_unchanged_capture_do_not_prematurely_branch() {
        let (context, _, purple, original_attempt) = returned_context(ContentDelta::Unchanged);
        let plan = plan_capture(
            &context,
            CaptureRequest {
                kind: CaptureKind::Save,
                label: "still green".into(),
                message: None,
                state_id: StateId::new_v7(),
                divergence_attempt_id: AttemptId::new_v7(),
            },
        )
        .unwrap();

        assert_eq!(plan.target_attempt, original_attempt);
        assert_eq!(plan.preserved_future, None);
        assert!(!plan.creates_ordinary_branch);
        assert!(
            !plan
                .effects
                .iter()
                .any(|effect| matches!(effect, CaptureEffect::ForkAttempt { .. }))
        );
        assert_ne!(context.current_state, Some(purple));
    }

    #[test]
    fn first_changed_capture_after_return_creates_sibling_and_preserves_future() {
        let (context, green, purple, _) = returned_context(ContentDelta::Changed);
        let orange_attempt = AttemptId::new_v7();
        let plan = plan_capture(
            &context,
            CaptureRequest {
                kind: CaptureKind::Save,
                label: "orange".into(),
                message: None,
                state_id: StateId::new_v7(),
                divergence_attempt_id: orange_attempt,
            },
        )
        .unwrap();

        assert_eq!(plan.target_attempt, orange_attempt);
        assert_eq!(plan.preserved_future, Some(purple));
        assert!(!plan.creates_ordinary_branch);
        assert!(plan.effects.iter().any(|effect| matches!(
            effect,
            CaptureEffect::ForkAttempt { from_state, preserved_tip, .. }
                if *from_state == green && *preserved_tip == purple
        )));
        assert!(plan.effects.iter().any(|effect| matches!(
            effect,
            CaptureEffect::RecordState { logical_parent: Some(parent), attempt_id, kind, .. }
                if *parent == green && *attempt_id == orange_attempt && kind == "save"
        )));
    }

    #[test]
    fn capture_flavors_keep_stable_event_kind() {
        assert_eq!(CaptureKind::Save.event_kind(), "save");
        assert_eq!(CaptureKind::Step.event_kind(), "step");
        assert_eq!(CaptureKind::Nice.event_kind(), "nice");
    }
}

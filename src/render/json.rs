use serde::Serialize;

pub const COMMAND_SCHEMA: &str = "jjk.command/v1";

/// Invocation and projection facts shared by success and failure envelopes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnvelopeMeta<'a> {
    pub request_id: Option<&'a str>,
    pub operation_id: Option<&'a str>,
    pub projection_version: Option<u64>,
    pub outcome: &'a str,
}

impl Default for EnvelopeMeta<'static> {
    fn default() -> Self {
        Self {
            request_id: None,
            operation_id: None,
            projection_version: None,
            outcome: "observed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MachineError<'a> {
    pub code: &'a str,
    pub message: &'a str,
    pub subject_ids: &'a [&'a str],
    pub retryable: bool,
    pub recovery_commands: &'a [Vec<&'a str>],
}

#[derive(Serialize)]
struct CommandEnvelope<'a, T: Serialize> {
    schema: &'static str,
    request_id: Option<&'a str>,
    operation_id: Option<&'a str>,
    projection_version: Option<u64>,
    outcome: &'a str,
    result: Option<&'a T>,
    warnings: &'a [String],
    error: Option<&'a MachineError<'a>>,
}

pub fn render<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    render_with_meta(value, EnvelopeMeta::default(), &[])
}

pub fn render_with_meta<T: Serialize>(
    value: &T,
    meta: EnvelopeMeta<'_>,
    warnings: &[String],
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&CommandEnvelope {
        schema: COMMAND_SCHEMA,
        request_id: meta.request_id,
        operation_id: meta.operation_id,
        projection_version: meta.projection_version,
        outcome: meta.outcome,
        result: Some(value),
        warnings,
        error: None,
    })
}

pub fn render_error(code: &str, message: &str) -> Result<String, serde_json::Error> {
    let error = MachineError {
        code,
        message,
        subject_ids: &[],
        retryable: false,
        recovery_commands: &[],
    };
    render_error_with_meta(
        &error,
        EnvelopeMeta {
            outcome: "failed",
            ..EnvelopeMeta::default()
        },
        &[],
    )
}

pub fn render_error_with_meta(
    error: &MachineError<'_>,
    meta: EnvelopeMeta<'_>,
    warnings: &[String],
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&CommandEnvelope::<()> {
        schema: COMMAND_SCHEMA,
        request_id: meta.request_id,
        operation_id: meta.operation_id,
        projection_version: meta.projection_version,
        outcome: meta.outcome,
        result: None,
        warnings,
        error: Some(error),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn success_and_error_use_the_same_required_envelope_fields() {
        let meta = EnvelopeMeta {
            request_id: Some("req_fixed"),
            operation_id: None,
            projection_version: Some(7),
            outcome: "observed",
        };
        let success: Value =
            serde_json::from_str(&render_with_meta(&json!({"value": 1}), meta, &[]).unwrap())
                .unwrap();
        let failure: Value = serde_json::from_str(
            &render_error_with_meta(
                &MachineError {
                    code: "AMBIGUOUS",
                    message: "choose one",
                    subject_ids: &["st_b", "st_a"],
                    retryable: false,
                    recovery_commands: &[vec!["jjk", "see", "--format", "json"]],
                },
                EnvelopeMeta {
                    outcome: "failed",
                    ..meta
                },
                &[],
            )
            .unwrap(),
        )
        .unwrap();
        for field in [
            "schema",
            "request_id",
            "operation_id",
            "projection_version",
            "outcome",
            "result",
            "warnings",
            "error",
        ] {
            assert!(success.get(field).is_some(), "success missing {field}");
            assert!(failure.get(field).is_some(), "failure missing {field}");
        }
        assert_eq!(success["schema"], COMMAND_SCHEMA);
        assert_eq!(failure["schema"], COMMAND_SCHEMA);
        assert!(success["error"].is_null());
        assert!(failure["result"].is_null());
    }

    #[test]
    fn machine_output_is_deterministic_untruncated_and_ansi_free() {
        let value = json!({"z": "full terminal-safe message", "a": [3, 2, 1]});
        let rendered = render(&value).unwrap();
        assert!(!rendered.contains('\u{1b}'));
        assert!(rendered.contains("full terminal-safe message"));
        assert_eq!(rendered, render(&value).unwrap());
    }
}

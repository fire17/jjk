use crate::app::query::*;
use crate::cli::OutputPolicy;
use crate::render::graph;
use crate::render::style::{Role, Styler, fit, sanitize};

pub fn outcome(outcome: &QueryOutcome, policy: OutputPolicy) -> String {
    match outcome {
        QueryOutcome::Current(model) => current(model, policy),
        QueryOutcome::Status(model) => status(model, policy),
        QueryOutcome::Graph(model) => graph::render(model, policy),
        QueryOutcome::Story(model) => story(model, policy),
        QueryOutcome::Show(model) => show(model, policy),
        QueryOutcome::Diff(model) => diff(model, policy),
    }
}

pub fn current(model: &CurrentReadModel, policy: OutputPolicy) -> String {
    let styler = Styler::new(policy);
    let mut lines = vec![format!("safety: {:?}", model.safety).to_lowercase()];
    if let Some(state) = &model.state {
        lines.push(styler.paint(
            Role::Strong,
            &fit(
                &format!(
                    "current: {} [{}] {}",
                    state.id,
                    sanitize(&state.kind),
                    sanitize(&state.label)
                ),
                policy.terminal_width,
            ),
        ));
        lines.push(fit(
            &format!("attempt: {}", state.attempt_id),
            policy.terminal_width,
        ));
        lines.push(fit(
            &format!("git: {}", state.git_object),
            policy.terminal_width,
        ));
        lines.push(fit(
            &format!(
                "parent: {}",
                model
                    .parent
                    .map_or_else(|| "none".into(), |id| id.to_string())
            ),
            policy.terminal_width,
        ));
    } else {
        lines.push("current: none".into());
    }
    lines.push(fit(
        &format!(
            "workspace: {}",
            model
                .workspace
                .branch
                .as_deref()
                .map(sanitize)
                .unwrap_or_else(|| "detached".into())
        ),
        policy.terminal_width,
    ));
    if let Some(position) = model.history_position {
        lines.push(format!("history: {position}/{}", model.history_length));
    }
    warnings(&mut lines, &model.warnings, policy, &styler);
    lines.join("\n")
}

pub fn status(model: &StatusReadModel, policy: OutputPolicy) -> String {
    let styler = Styler::new(policy);
    let changes = &model.workspace.changes;
    let worktree = if changes.is_dirty() {
        format!(
            "dirty ({} files; staged={}, unstaged={}, untracked={}, conflicted={})",
            changes.changed_files(),
            changes.staged,
            changes.unstaged,
            changes.untracked,
            changes.conflicted
        )
    } else {
        "clean".into()
    };
    let mut lines = vec![
        styler.paint(
            match model.safety {
                SafetyState::Safe => Role::Good,
                SafetyState::Dirty | SafetyState::Diverged => Role::Warning,
                _ => Role::Error,
            },
            &format!("safety: {:?}", model.safety).to_lowercase(),
        ),
        fit(
            &format!("safe space: {}", sanitize(&model.repository_label)),
            policy.terminal_width,
        ),
        fit(
            &format!(
                "branch: {}",
                model
                    .workspace
                    .branch
                    .as_deref()
                    .map(sanitize)
                    .unwrap_or_else(|| "detached".into())
            ),
            policy.terminal_width,
        ),
        fit(&format!("worktree: {worktree}"), policy.terminal_width),
        format!(
            "states: {} visible / {} saved",
            model.visible_states, model.saved_states
        ),
        format!(
            "recovery: {}",
            if model.recovery.required {
                "required"
            } else {
                "none"
            }
        ),
    ];
    for (capability, enabled) in &model.capabilities {
        lines.push(fit(
            &format!(
                "{capability}: {}",
                if *enabled { "available" } else { "unavailable" }
            ),
            policy.terminal_width,
        ));
    }
    warnings(&mut lines, &model.warnings, policy, &styler);
    lines.join("\n")
}

pub fn story(model: &StoryReadModel, policy: OutputPolicy) -> String {
    if model.entries.is_empty() {
        return "No nice or starred states yet.".into();
    }
    model
        .entries
        .iter()
        .map(|entry| {
            let marker = if entry.markers.contains(&SemanticMarker::Starred) {
                "+ "
            } else {
                ""
            };
            fit(
                &format!(
                    "{marker}{} [{}] {} — {}",
                    entry.state.id,
                    sanitize(&entry.state.kind),
                    sanitize(&entry.state.label),
                    entry
                        .state
                        .message
                        .as_deref()
                        .map(sanitize)
                        .unwrap_or_default()
                ),
                policy.terminal_width,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn show(model: &ShowReadModel, policy: OutputPolicy) -> String {
    let header = fit(
        &format!(
            "{} [{}] {}",
            model.state.id,
            sanitize(&model.state.kind),
            sanitize(&model.state.label)
        ),
        policy.terminal_width,
    );
    format!("{header}\n{}", patch(&model.patch, policy))
}

pub fn diff(model: &DiffReadModel, policy: OutputPolicy) -> String {
    patch(&model.patch, policy)
}

fn patch(model: &PatchReadModel, policy: OutputPolicy) -> String {
    let mut lines = vec![format!(
        "{} file(s), +{}, -{}",
        model.stats.changed_files, model.stats.insertions, model.stats.deletions
    )];
    for file in &model.files {
        let old = file
            .old_path
            .as_deref()
            .map(sanitize)
            .unwrap_or_else(|| "/dev/null".into());
        let new = file
            .new_path
            .as_deref()
            .map(sanitize)
            .unwrap_or_else(|| "/dev/null".into());
        lines.push(fit(&format!("--- {old}"), policy.terminal_width));
        lines.push(fit(&format!("+++ {new}"), policy.terminal_width));
        if file.binary {
            lines.push("Binary files differ".into());
            continue;
        }
        for hunk in &file.hunks {
            lines.push(fit(&sanitize(&hunk.header), policy.terminal_width));
            for line in &hunk.lines {
                let prefix = match line.kind {
                    DiffLineKind::Addition => "+",
                    DiffLineKind::Deletion => "-",
                    DiffLineKind::Context => " ",
                    DiffLineKind::Notice => "!",
                };
                lines.push(fit(
                    &format!("{prefix}{}", sanitize(&line.text)),
                    policy.terminal_width,
                ));
            }
        }
    }
    lines.join("\n")
}

fn warnings(lines: &mut Vec<String>, values: &[String], policy: OutputPolicy, styler: &Styler) {
    for warning in values {
        lines.push(styler.paint(
            Role::Warning,
            &fit(&format!("! {}", sanitize(warning)), policy.terminal_width),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::query::{
        RecoveryReadModel, SafetyState, StatusReadModel, WorkspaceReadModel, WorktreeChanges,
    };
    use crate::cli::OutputMode;
    use std::collections::BTreeMap;

    #[test]
    fn status_is_bounded_at_supported_widths() {
        let mut capabilities = BTreeMap::new();
        capabilities.insert("git".into(), true);
        capabilities.insert("jj".into(), false);
        let model = StatusReadModel {
            schema_version: 1,
            revision: 9,
            repository_label: "safe space with a hostile\nsecond row".into(),
            safety: SafetyState::Dirty,
            current_state: None,
            current_attempt: None,
            workspace: WorkspaceReadModel {
                id: None,
                branch: Some("feature/rendering-with-a-long-branch-name".into()),
                head: None,
                changes: WorktreeChanges {
                    staged: 1,
                    unstaged: 2,
                    untracked: 3,
                    conflicted: 1,
                },
            },
            saved_states: 12,
            visible_states: 10,
            recovery: RecoveryReadModel {
                required: true,
                summary: Some("repair pending".into()),
            },
            capabilities,
            warnings: vec!["external history is ambiguous until reconciliation completes".into()],
        };
        for width in [40, 80, 120, 200] {
            let policy = OutputPolicy::deterministic(OutputMode::Human, false, width, true);
            let rendered = status(&model, policy);
            assert!(
                rendered
                    .lines()
                    .all(|line| crate::render::style::visible_width(line) <= width),
                "width {width}: {rendered}"
            );
            assert!(rendered.contains("recovery: required"));
            assert!(rendered.contains("! external history"));
        }
    }
}

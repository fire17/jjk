use crate::app::query::{GraphNode, GraphReadModel, SemanticMarker};
use crate::cli::OutputPolicy;
use crate::render::style::{Role, Styler, fit, sanitize};

pub fn render(model: &GraphReadModel, policy: OutputPolicy) -> String {
    let width = policy.terminal_width.max(1);
    if model.nodes.is_empty() {
        let mut lines = vec![fit("No states saved yet.", width)];
        append_completeness(&mut lines, model, policy);
        return lines.join("\n");
    }

    let styler = Styler::new(policy);
    let mut lines = vec![
        fit("* current  ^ attempt tip  + starred  ! warning", width),
        String::new(),
    ];
    for node in &model.nodes {
        let plain = node_line(node, width);
        let line = if node.markers.contains(&SemanticMarker::Current) {
            styler.paint(Role::Strong, &plain)
        } else if node.markers.contains(&SemanticMarker::AttemptTip) {
            styler.paint(Role::Accent, &plain)
        } else {
            plain
        };
        lines.push(line);
    }
    append_completeness(&mut lines, model, policy);
    lines.join("\n")
}

fn node_line(node: &GraphNode, width: usize) -> String {
    let current = marker(node, SemanticMarker::Current, '*');
    let tip = marker(node, SemanticMarker::AttemptTip, '^');
    let star = marker(node, SemanticMarker::Starred, '+');
    let warning = marker(node, SemanticMarker::Warning, '!');
    let markers = format!("{current}{tip}{star}{warning}");
    let id = node.state.id.to_string();
    let shown_id = if width >= 120 {
        id.as_str()
    } else {
        short_id(&id, if width < 80 { 10 } else { 14 })
    };
    let connector = if node.depth == 0 {
        String::new()
    } else if width < 80 {
        format!("{}> ", node.depth)
    } else {
        let visible_depth = node.depth.min(8);
        let folded = if node.depth > visible_depth {
            format!("{}…", node.depth - visible_depth)
        } else {
            String::new()
        };
        format!(
            "{folded}{}└─ ",
            "  ".repeat(visible_depth.saturating_sub(1))
        )
    };
    let kind = sanitize(&node.state.kind);
    let label = sanitize(&node.state.label);
    let base = if width < 80 {
        format!("{connector}{markers} {shown_id} [{kind}] {label}")
    } else if width < 120 {
        format!(
            "{connector}{markers} {shown_id} [{kind}] {label}  lane:{}",
            node.lane
        )
    } else {
        let message = node
            .state
            .message
            .as_deref()
            .map(sanitize)
            .filter(|value| !value.is_empty());
        let details = message.map_or_else(String::new, |value| format!(" — {value}"));
        format!(
            "{connector}{markers} {shown_id} [{kind}] {label}{details}  +{} -{}  lane:{}",
            node.state.stats.insertions, node.state.stats.deletions, node.lane
        )
    };
    fit(&base, width)
}

fn marker(node: &GraphNode, marker: SemanticMarker, shown: char) -> char {
    if node.markers.contains(&marker) {
        shown
    } else {
        ' '
    }
}

fn short_id(id: &str, width: usize) -> &str {
    &id[..id.len().min(width)]
}

fn append_completeness(lines: &mut Vec<String>, model: &GraphReadModel, policy: OutputPolicy) {
    let width = policy.terminal_width.max(1);
    let styler = Styler::new(policy);
    if model.omitted.incomplete {
        let summary = fit(
            &format!(
                "! INCOMPLETE: {} archived state(s) hidden",
                model.omitted.archived_states
            ),
            width,
        );
        lines.push(styler.paint(Role::Warning, &summary));
        for reason in &model.omitted.reasons {
            lines.push(styler.paint(
                Role::Warning,
                &fit(&format!("! reason: {}", sanitize(reason)), width),
            ));
        }
    }
    for warning in &model.warnings {
        lines.push(styler.paint(
            Role::Warning,
            &fit(&format!("! warning: {}", sanitize(warning)), width),
        ));
    }
}

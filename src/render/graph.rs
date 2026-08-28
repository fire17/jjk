use crate::app::query::{GraphReadModel, SemanticMarker};
use crate::cli::OutputPolicy;
use crate::render::style::{fit, sanitize, Role, Styler};

pub fn render(model: &GraphReadModel, policy: OutputPolicy) -> String {
    if model.nodes.is_empty() { return "No states saved yet.".to_owned(); }
    let styler = Styler::new(policy);
    let mut lines = vec!["* current  ^ attempt tip  + starred  ! warning".to_owned(), String::new()];
    for node in &model.nodes {
        let current = if node.markers.contains(&SemanticMarker::Current) { "*" } else { " " };
        let tip = if node.markers.contains(&SemanticMarker::AttemptTip) { "^" } else { " " };
        let star = if node.markers.contains(&SemanticMarker::Starred) { "+" } else { " " };
        let warning = if node.markers.contains(&SemanticMarker::Warning) { "!" } else { " " };
        let prefix = if node.depth == 0 { "".to_owned() } else { format!("{}└─ ", "   ".repeat(node.depth.saturating_sub(1))) };
        let fixed = prefix.chars().count() + 8 + node.state.id.to_string().len();
        let detail_width = policy.terminal_width.saturating_sub(fixed).max(4);
        let detail = fit(&format!("[{}] {}", sanitize(&node.state.kind), sanitize(&node.state.label)), detail_width);
        let line = format!("{prefix}{current}{tip}{star}{warning} {} {detail}", node.state.id);
        lines.push(if node.markers.contains(&SemanticMarker::Current) { styler.paint(Role::Strong, &line) }
            else if node.markers.contains(&SemanticMarker::AttemptTip) { styler.paint(Role::Accent, &line) } else { line });
    }
    if model.omitted.incomplete {
        lines.push(styler.paint(Role::Warning, &fit(&format!("! incomplete: {} archived state(s) hidden", model.omitted.archived_states), policy.terminal_width)));
    }
    lines.into_iter().map(|line| fit_ansi(&line, policy.terminal_width)).collect::<Vec<_>>().join("\n")
}

fn fit_ansi(value: &str, width: usize) -> String {
    if crate::render::style::visible_width(value) <= width { value.to_owned() } else { fit(value, width) }
}

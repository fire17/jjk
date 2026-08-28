use crate::app::resolve::ResolutionCandidate;
use crate::render::style::{fit, sanitize};

pub fn ambiguity(candidates: &[ResolutionCandidate], width: usize) -> String {
    if candidates.is_empty() { return String::new(); }
    if width < 60 {
        return candidates.iter().enumerate().map(|(index, candidate)| {
            format!("{}. {} [{}] {}", index + 1, candidate.id, sanitize(&candidate.kind), fit(&candidate.label, width.saturating_sub(20)))
        }).collect::<Vec<_>>().join("\n");
    }
    let id_width = candidates.iter().map(|candidate| candidate.id.to_string().len()).max().unwrap_or(8).min(32);
    let kind_width = candidates.iter().map(|candidate| candidate.kind.chars().count()).max().unwrap_or(4).clamp(4, 12);
    let fixed = 4 + id_width + kind_width + 6;
    let label_width = width.saturating_sub(fixed).max(8);
    let mut lines = vec![format!("#   {:id_width$}  {:kind_width$}  label", "id", "kind")];
    for (index, candidate) in candidates.iter().enumerate() {
        lines.push(format!("{:<3} {:id_width$}  {:kind_width$}  {}", index + 1, fit(&candidate.id.to_string(), id_width),
            fit(&candidate.kind, kind_width), fit(&candidate.label, label_width)));
    }
    lines.join("\n")
}

use crate::app::resolve::ResolutionCandidate;
use crate::render::style::{fit, sanitize};

pub fn ambiguity(candidates: &[ResolutionCandidate], width: usize) -> String {
    if candidates.is_empty() {
        return String::new();
    }
    let width = width.max(1);
    let mut ordered = candidates.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.created_at_utc
            .cmp(&right.created_at_utc)
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut lines = vec![fit(
        &format!(
            "AMBIGUOUS: {} states match; choose an exact ID",
            ordered.len()
        ),
        width,
    )];
    if width < 60 {
        lines.extend(ordered.iter().enumerate().map(|(index, candidate)| {
            fit(
                &format!(
                    "{}. {} [{}] {}",
                    index + 1,
                    candidate.id,
                    sanitize(&candidate.kind),
                    sanitize(&candidate.label)
                ),
                width,
            )
        }));
        return lines.join("\n");
    }
    let id_width = ordered
        .iter()
        .map(|candidate| candidate.id.to_string().len())
        .max()
        .unwrap_or(8)
        .min(32);
    let kind_width = ordered
        .iter()
        .map(|candidate| sanitize(&candidate.kind).chars().count())
        .max()
        .unwrap_or(4)
        .clamp(4, 12);
    let fixed = 4 + id_width + kind_width + 4;
    let label_width = width.saturating_sub(fixed).max(1);
    lines.push(fit(
        &format!("#   {:id_width$}  {:kind_width$}  label", "id", "kind"),
        width,
    ));
    for (index, candidate) in ordered.iter().enumerate() {
        lines.push(fit(
            &format!(
                "{:<3} {:id_width$}  {:kind_width$}  {}",
                index + 1,
                fit(&candidate.id.to_string(), id_width),
                fit(&candidate.kind, kind_width),
                fit(&candidate.label, label_width)
            ),
            width,
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::StateId;

    fn id(last: u8) -> StateId {
        let mut bytes = [0_u8; 16];
        bytes[6] = 0x70;
        bytes[8] = 0x80;
        bytes[15] = last;
        StateId::from_bytes(bytes).unwrap()
    }

    #[test]
    fn ambiguity_is_stable_visible_and_width_bounded() {
        let later = ResolutionCandidate {
            id: id(2),
            label: "second very long label".into(),
            kind: "nice".into(),
            attempt: "a".into(),
            created_at_utc: "2026-01-02T00:00:00Z".into(),
        };
        let earlier = ResolutionCandidate {
            id: id(1),
            label: "first\nspoof".into(),
            kind: "save".into(),
            attempt: "b".into(),
            created_at_utc: "2026-01-01T00:00:00Z".into(),
        };
        for width in [40, 80, 120, 200] {
            let forward = ambiguity(&[later.clone(), earlier.clone()], width);
            let reverse = ambiguity(&[earlier.clone(), later.clone()], width);
            assert_eq!(forward, reverse);
            assert!(forward.starts_with("AMBIGUOUS:"));
            assert!(
                forward
                    .lines()
                    .all(|line| crate::render::style::visible_width(line) <= width)
            );
            assert!(
                forward.find(&earlier.id.to_string()).unwrap()
                    < forward.find(&later.id.to_string()).unwrap()
            );
        }
    }
}

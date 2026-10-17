//! Read-facing selection errors with ranked canonical candidates, and the
//! boundary that keeps merge-layer codes out of read errors.
use crate::output::AgentError;
use ara_core::merge::MergeError;
use ara_core::write::ArtifactSnapshot;
use serde_json::json;

pub const MAX_CANDIDATES: usize = 40;
/// Characters compared per label; ranking cost stays bounded per candidate.
const RANK_CHARS: usize = 64;

pub struct Candidates {
    addresses: Vec<String>,
    capped: bool,
}
impl Candidates {
    /// Keep `addresses` in the given order, capped at [`MAX_CANDIDATES`].
    pub fn ordered(addresses: impl IntoIterator<Item = String>) -> Self {
        let mut addresses: Vec<String> = addresses.into_iter().collect();
        let capped = addresses.len() > MAX_CANDIDATES;
        addresses.truncate(MAX_CANDIDATES);
        Self { addresses, capped }
    }
    /// Rank `(address, label)` pairs by edit distance between the normalized
    /// label and `wanted`, then by the given (source) order.
    pub fn ranked<'a>(wanted: &str, items: impl IntoIterator<Item = (String, &'a str)>) -> Self {
        let wanted = normalize(wanted);
        let mut seen = std::collections::BTreeSet::new();
        let mut scored: Vec<(usize, usize, String)> = items
            .into_iter()
            .filter(|(address, _)| seen.insert(address.clone()))
            .enumerate()
            .map(|(order, (address, label))| (distance(&wanted, &normalize(label)), order, address))
            .collect();
        scored.sort();
        Self::ordered(scored.into_iter().map(|(_, _, address)| address))
    }
    fn attach(self, error: AgentError) -> AgentError {
        AgentError {
            details: Some(Box::new(
                json!({"candidates": self.addresses, "capped": self.capped}),
            )),
            ..error
        }
    }
}

/// Locale-independent tolerant form: trimmed, Unicode lowercase, with no
/// accent stripping or Unicode normalization.
pub fn normalize(text: &str) -> String {
    text.trim().to_lowercase()
}
fn distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().take(RANK_CHARS).collect();
    let right: Vec<char> = right.chars().take(RANK_CHARS).collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    for (i, a) in left.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, b) in right.iter().enumerate() {
            current.push(
                (previous[j] + usize::from(a != b))
                    .min(previous[j + 1] + 1)
                    .min(current[j] + 1),
            );
        }
        previous = current;
    }
    previous[right.len()]
}

pub fn unknown(id: &str, candidates: Candidates) -> AgentError {
    candidates.attach(AgentError::unknown(id))
}
pub fn ambiguous(id: &str, candidates: Candidates) -> AgentError {
    candidates.attach(AgentError {
        id: Some(id.into()),
        ..AgentError::semantic(
            "ambiguous_heading",
            format!("Selector `{id}` matches more than one target; read a candidate address"),
        )
    })
}

pub enum Miss {
    Unknown,
    Ambiguous,
}
/// Classify a merge-layer failure raised while resolving a read selector.
/// Only a failed selection becomes a miss, and only once the
/// request-independent identity records index cleanly. Corrupt or
/// unindexable records keep a truthful diagnostic without the internal
/// `merge.*` code.
pub fn classify(error: MergeError, snapshot: &ArtifactSnapshot) -> Result<Miss, AgentError> {
    let miss = match error.code.as_str() {
        "merge.unknown_identity" => Miss::Unknown,
        "merge.redirect_ambiguous" | "merge.selector_ambiguous" => Miss::Ambiguous,
        _ => return Err(lookup_failed(error)),
    };
    ara_core::merge::check_identities(snapshot).map_err(lookup_failed)?;
    Ok(miss)
}
fn lookup_failed(error: MergeError) -> AgentError {
    AgentError {
        exit: error.exit_code(),
        details: error
            .field
            .as_ref()
            .map(|path| Box::new(json!({ "path": path }))),
        ..AgentError::semantic(
            "identity_lookup_failed",
            format!("Recorded identities cannot be consulted: {}", error.message),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking_is_stable_deduplicated_and_capped() {
        let labels: Vec<String> = (0..50).map(|i| format!("Item {i:02}")).collect();
        let ranked = Candidates::ranked(
            "item 7",
            labels
                .iter()
                .map(|label| (label.clone(), label.as_str()))
                .chain([("Item 07".to_owned(), "Item 07")]),
        );
        assert!(ranked.capped);
        assert_eq!(ranked.addresses.len(), MAX_CANDIDATES);
        assert_eq!(ranked.addresses[..2], ["Item 07", "Item 17"]);
        assert_eq!(
            ranked.addresses.iter().filter(|a| *a == "Item 07").count(),
            1
        );
    }

    #[test]
    fn normalization_is_trim_and_unicode_lowercase_only() {
        assert_eq!(normalize("  ÉTAPE Ä "), "étape ä");
        assert_ne!(normalize("Étape"), normalize("Etape"));
    }
}

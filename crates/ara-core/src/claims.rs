//! Lenient Markdown parser for `logic/claims.md`.
//!
//! Claims are semi-structured prose: `## C01: Title` headers followed by
//! `- **Key**: value` bullets. The corpus drifts (e.g. `Dependencies` appears as
//! `none`, `[]`, `[C01]`, `C01`, or `C02, C04`), so bullet values are scanned
//! for `C##` / `E##` tokens rather than parsed as a fixed shape. Missing bullets
//! are tolerated. Duplicate claim ids are surfaced as data for the caller to
//! turn into an error diagnostic — this module stays free of the `Diagnostic`
//! type.

use crate::manifest::{Claim, ClaimId, is_canonical_id};
use std::collections::BTreeSet;

/// Result of parsing `claims.md`: claims in source order, plus any claim ids
/// that appeared more than once (first occurrence wins; the rest are dups).
pub(crate) struct ParsedClaims {
    pub claims: Vec<Claim>,
    pub duplicate_ids: Vec<String>,
}

/// Parses claim content. Never fails: malformed content yields fewer claims,
/// not an error.
pub(crate) fn parse_claims(md: &str) -> ParsedClaims {
    let lines: Vec<&str> = md.lines().collect();
    let mut claims = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut duplicate_ids = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let Some((id, title)) = parse_header(lines[i]) else {
            i += 1;
            continue;
        };

        // Body runs until the next level-2 header (the next claim or section).
        let mut j = i + 1;
        while j < lines.len() && !is_level2_header(lines[j]) {
            j += 1;
        }
        let body = &lines[i + 1..j];

        let mut statement = None;
        let mut status = None;
        let mut falsification = None;
        let mut proof = Vec::new();
        let mut deps = Vec::new();
        for line in body {
            let Some((key, value)) = parse_bullet(line) else {
                continue;
            };
            match key.to_ascii_lowercase().as_str() {
                "statement" => statement = non_empty(&value),
                "status" => status = non_empty(&value),
                "falsification criteria" => falsification = non_empty(&value),
                "proof" => proof = extract_ids(&value, 'E'),
                "dependencies" => {
                    deps = extract_ids(&value, 'C')
                        .into_iter()
                        .map(ClaimId::new)
                        .collect()
                }
                _ => {}
            }
        }

        if seen.contains(&id) {
            duplicate_ids.push(id);
        } else {
            seen.insert(id.clone());
            claims.push(Claim {
                id: ClaimId::new(id),
                title,
                statement,
                status,
                falsification,
                proof,
                deps,
            });
        }
        i = j; // re-examine the terminating header on the next iteration
    }

    ParsedClaims {
        claims,
        duplicate_ids,
    }
}

/// Matches `## C\d+: title`, returning `(id, title)`. Non-claim `##` headers
/// and deeper/shallower headers return `None`.
fn parse_header(line: &str) -> Option<(String, String)> {
    let rest = line.trim_start().strip_prefix("## ")?;
    let (raw_id, raw_title) = rest.split_once(':')?;
    let id = raw_id.trim();
    if !is_canonical_id(id, 'C') {
        return None;
    }
    let title = raw_title.trim();
    if title.is_empty() {
        return None;
    }
    Some((id.to_string(), title.to_string()))
}

/// True for any level-2 header line (`## ...`), which terminates a claim body.
fn is_level2_header(line: &str) -> bool {
    line.trim_start().starts_with("## ")
}

/// Matches `- **Key**: value` or `**Key.** value`, returning `(key, value)`.
fn parse_bullet(line: &str) -> Option<(String, String)> {
    let t = line.trim_start();
    let rest = t
        .strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .unwrap_or(t);
    let rest = rest.trim_start().strip_prefix("**")?;
    let (key, rest_after) = rest.split_once("**")?;
    let after = rest_after.trim_start();
    let value = after.strip_prefix(':').unwrap_or(after).trim();
    let key = key.trim();
    let key = key.strip_suffix('.').unwrap_or(key).trim_end();
    Some((key.to_string(), value.to_string()))
}

/// Extracts every `^<prefix>\d+$` token, splitting on non-alphanumeric
/// separators. Handles `[C01]`, `C01`, `C02, C04`, `none`, `[]` uniformly.
fn extract_ids(value: &str, prefix: char) -> Vec<String> {
    value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|tok| is_canonical_id(tok, prefix))
        .map(|s| s.to_string())
        .collect()
}

/// Trims and returns `None` for empty values.
fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_falsification_and_references_across_claims() {
        for (leader, label_end, separator) in [
            ("- ", "", ":"),
            ("* ", "", ":"),
            ("", "", ":"),
            ("", ".", ""),
        ] {
            let md = format!(
                "## C01: Comparison\n{leader}**Falsification criteria{label_end}**{separator} The improvement disappears under a matched comparison.\n{leader}**Proof{label_end}**{separator} [E01, E02]\n{leader}**Dependencies{label_end}**{separator} [C02]\n{leader}**Unknown label{label_end}**{separator} ignored\n## C02: Control\n{leader}**Statement{label_end}**{separator} Choices are fixed.\n"
            );
            let out = parse_claims(&md);
            let json = serde_json::to_value(&out.claims).unwrap();
            assert_eq!(
                json[0]["falsification"],
                "The improvement disappears under a matched comparison."
            );
            assert_eq!(out.claims[0].proof, ["E01", "E02"]);
            assert_eq!(out.claims[0].deps, [ClaimId::new("C02")]);
            assert!(json[1].get("falsification").is_none());
            assert!(out.claims[1].proof.is_empty());
            assert!(out.claims[1].deps.is_empty());
        }
    }

    #[test]
    fn blank_falsification_is_absent() {
        let out = parse_claims("## C01: Blank\n- **Falsification criteria**:   \n");
        let json = serde_json::to_value(&out.claims[0]).unwrap();
        assert!(json.get("falsification").is_none());
    }

    #[test]
    fn parses_canonical_claims() {
        let md = "\
# Claims

## C01: Attention-only architecture achieves SOTA
- **Statement**: A model based entirely on self-attention achieves SOTA.
- **Status**: supported
- **Proof**: [E01]
- **Dependencies**: []
- **Tags**: architecture, translation

## C02: Transformers train faster
- **Statement**: The Transformer requires less training time.
- **Status**: supported
- **Proof**: [E02]
- **Dependencies**: [C01]
";
        let out = parse_claims(md);
        assert!(out.duplicate_ids.is_empty());
        assert_eq!(out.claims.len(), 2);
        let c1 = &out.claims[0];
        assert_eq!(c1.id, ClaimId::new("C01"));
        assert_eq!(c1.title, "Attention-only architecture achieves SOTA");
        assert!(c1.statement.as_deref().unwrap().starts_with("A model"));
        assert_eq!(c1.status.as_deref(), Some("supported"));
        assert_eq!(c1.proof, vec!["E01"]);
        assert!(c1.deps.is_empty());
        let c2 = &out.claims[1];
        assert_eq!(c2.deps, vec![ClaimId::new("C01")]);
    }

    #[test]
    fn tolerates_dependency_drift() {
        // Bare id, comma list, literal "none", empty brackets, and a list all
        // reduce to extracted C## tokens.
        for (dep_line, expected) in [
            ("- **Dependencies**: none", Vec::<&str>::new()),
            ("- **Dependencies**: []", vec![]),
            ("- **Dependencies**: C01", vec!["C01"]),
            ("- **Dependencies**: C02, C04", vec!["C02", "C04"]),
            ("- **Dependencies**: [C01, C03]", vec!["C01", "C03"]),
        ] {
            let md = format!("## C09: Drift\n- **Statement**: x\n{dep_line}\n");
            let out = parse_claims(&md);
            let deps: Vec<String> = out.claims[0].deps.iter().map(|d| d.to_string()).collect();
            assert_eq!(deps, expected, "for line: {dep_line}");
        }
    }

    #[test]
    fn tolerates_missing_bullets() {
        let out = parse_claims("## C01: Bare claim, no bullets at all\n");
        assert_eq!(out.claims.len(), 1);
        let c = &out.claims[0];
        assert_eq!(c.title, "Bare claim, no bullets at all");
        assert!(c.statement.is_none());
        assert!(c.status.is_none());
        assert!(c.proof.is_empty());
        assert!(c.deps.is_empty());
    }

    #[test]
    fn detects_duplicate_claim_id() {
        let md = "## C01: First\n- **Statement**: a\n## C01: Second\n- **Statement**: b\n";
        let out = parse_claims(md);
        assert_eq!(out.claims.len(), 1);
        assert_eq!(out.claims[0].statement.as_deref(), Some("a")); // first wins
        assert_eq!(out.duplicate_ids, vec!["C01"]);
    }

    #[test]
    fn ignores_non_claim_headers_and_deeper_levels() {
        let md = "\
# Claims
## Overview: not a claim
### C01: too deep, not a claim
## C01: real claim
- **Statement**: ok
";
        let out = parse_claims(md);
        assert_eq!(out.claims.len(), 1);
        assert_eq!(out.claims[0].statement.as_deref(), Some("ok"));
    }

    #[test]
    fn proof_extracts_multiple_evidence_ids() {
        let md = "## C02: multi\n- **Proof**: [E01, E02]\n";
        let out = parse_claims(md);
        assert_eq!(out.claims[0].proof, vec!["E01", "E02"]);
    }
}

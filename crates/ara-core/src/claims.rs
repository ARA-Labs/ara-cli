//! Lenient Markdown parser for `logic/claims.md`.
//!
//! Claims are semi-structured prose: `## C01: Title` headers followed by
//! `- **Key**: value` bullets. A header may also separate the ID from the title
//! with a spaced ASCII hyphen, en dash or em dash (`## C01 — Title`); see
//! [`claim_heading`]. The corpus drifts (e.g. `Dependencies` appears as
//! `none`, `[]`, `[C01]`, `C01`, or `C02, C04`), so bullet values are scanned
//! for `C##` / `E##` tokens rather than parsed as a fixed shape. Missing bullets
//! are tolerated. Duplicate claim ids are surfaced as data for the caller to
//! turn into an error diagnostic — this module stays free of the `Diagnostic`
//! type.

use crate::manifest::{Claim, ClaimId, SourceField, is_canonical_id};
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
    let mut claims = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut duplicate_ids = Vec::new();
    for section in crate::markdown::document_sections(crate::stray_fence::CLAIMS, md) {
        let Some((id, title)) = claim_heading(section.heading) else {
            continue;
        };
        let mut claim = Claim {
            id: ClaimId::new(id),
            title: title.to_string(),
            statement: None,
            status: None,
            proof: Vec::new(),
            deps: Vec::new(),
            proof_content: None,
            provenance: None,
            falsification: None,
            conditions: None,
            sources: None,
            tags: None,
            last_revised: None,
            source_fields: Vec::new(),
            body: Some(md[section.range.clone()].to_string()),
        };
        for field in crate::markdown::fields(md, section.body_range) {
            let value = crate::markdown::decode_field(&field).into_owned();
            match field.name.to_ascii_lowercase().as_str() {
                "statement" => claim.statement = non_empty(&value),
                "status" => claim.status = non_empty(&value),
                "proof" => {
                    claim.proof = extract_ids(&value, 'E');
                    claim.proof_content = non_empty(&value);
                }
                "dependencies" => {
                    claim.deps = extract_ids(&value, 'C')
                        .into_iter()
                        .map(ClaimId::new)
                        .collect()
                }
                "provenance" => claim.provenance = non_empty(&value),
                "falsification" | "falsification criteria" => {
                    claim.falsification = non_empty(&value)
                }
                "conditions" => claim.conditions = non_empty(&value),
                "sources" => claim.sources = non_empty(&value),
                "tags" => claim.tags = non_empty(&value),
                "last revised" => claim.last_revised = non_empty(&value),
                _ => {}
            }
            claim.source_fields.push(SourceField {
                name: field.name.to_string(),
                value,
            });
        }
        if !seen.insert(id.to_string()) {
            duplicate_ids.push(id.to_string());
        } else {
            claims.push(claim);
        }
    }

    ParsedClaims {
        claims,
        duplicate_ids,
    }
}

/// Separators accepted between a claim ID and its title, besides `:`. Each
/// needs whitespace on both sides, so a hyphen inside an ID or a hyphenated
/// word never splits a heading.
const DASH_SEPARATORS: [char; 3] = ['-', '\u{2013}', '\u{2014}'];

/// The `(id, title)` of a level-two claim heading's text: a canonical `C##`
/// ID, then `:` or a spaced dash separator, then a nonempty title. Both are
/// trimmed slices of `heading`; the source spelling is never rewritten.
pub fn claim_heading(heading: &str) -> Option<(&str, &str)> {
    if let Some((id, title)) = heading.split_once(':')
        && is_canonical_id(id.trim(), 'C')
    {
        let title = title.trim();
        return (!title.is_empty()).then(|| (id.trim(), title));
    }
    let heading = heading.trim_start();
    let id_end = heading.find(char::is_whitespace)?;
    let (id, rest) = heading.split_at(id_end);
    let rest = rest.trim_start();
    let separator = rest.chars().next()?;
    let title = rest[separator.len_utf8()..].strip_prefix(char::is_whitespace)?;
    let title = title.trim();
    (is_canonical_id(id, 'C') && DASH_SEPARATORS.contains(&separator) && !title.is_empty())
        .then_some((id, title))
}

/// Level-two headings of `logic/claims.md` that start like a claim ID
/// (`C` and digits, then no further alphanumeric) but do not parse as a claim,
/// as `(one-based line, heading)`. Their claims are dropped by the parser.
pub fn unparsed_claim_headings(md: &str) -> Vec<(usize, &str)> {
    crate::markdown::document_sections(crate::stray_fence::CLAIMS, md)
        .into_iter()
        .filter(|section| {
            let digits = section.heading.strip_prefix('C').map(|rest| {
                let count = rest.bytes().take_while(u8::is_ascii_digit).count();
                (count, rest[count..].chars().next())
            });
            matches!(digits, Some((1.., next)) if !next.is_some_and(char::is_alphanumeric))
                && claim_heading(section.heading).is_none()
        })
        .map(|section| {
            let line = md[..section.range.start].matches('\n').count() + 1;
            (line, section.heading)
        })
        .collect()
}

/// Claim-like heading lines of `logic/claims.md` hidden by a code fence that
/// is still open at EOF, as `(one-based line, line)`. The parser folds them
/// into the preceding claim, so their claims are lost without any dangling
/// reference pointing at them. Closed fences (intentional examples) are fine.
pub fn fenced_claim_headings(md: &str) -> Vec<(usize, &str)> {
    hidden_claim_headings(
        md,
        crate::stray_fence::body_start(crate::stray_fence::CLAIMS, md),
    )
}

/// [`fenced_claim_headings`] scanned from byte offset `start`.
pub(crate) fn hidden_claim_headings(md: &str, start: usize) -> Vec<(usize, &str)> {
    crate::markdown::unclosed_code_fence_lines(md, start)
        .into_iter()
        .filter(|(_, line)| claim_like_heading(line))
        .collect()
}

/// A line starting with `#` whose text, ignoring `#`, `*`, whitespace and
/// case, starts with a claim ID: `C` and digits, then no alphanumeric.
fn claim_like_heading(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix('#') else {
        return false;
    };
    let compact: String = rest
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '#' | '*'))
        .collect();
    let Some(rest) = compact.strip_prefix(['C', 'c']) else {
        return false;
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && !rest[digits..].starts_with(char::is_alphanumeric)
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
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
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
    fn claim_headings_accept_colon_and_spaced_dash_separators() {
        let md = "# Claims\n## C01: Colon\n- **Statement**: a\n## C02 - Hyphen\n## C03 \u{2013} En dash\n## C04 \u{2014} Em dash: with colon\n## C05 :  Spaced colon\n";
        let out = parse_claims(md);
        let parsed: Vec<_> = out
            .claims
            .iter()
            .map(|c| (c.id.as_str(), c.title.as_str()))
            .collect();
        assert_eq!(
            parsed,
            [
                ("C01", "Colon"),
                ("C02", "Hyphen"),
                ("C03", "En dash"),
                ("C04", "Em dash: with colon"),
                ("C05", "Spaced colon"),
            ]
        );
        // Bodies keep the exact source heading bytes.
        assert!(
            out.claims[3]
                .body
                .as_deref()
                .unwrap()
                .starts_with("## C04 \u{2014} Em dash")
        );
    }

    #[test]
    fn dashes_inside_ids_or_prose_do_not_create_claims() {
        for heading in [
            "C01-Title",
            "C01 -Title",
            "C01- Title",
            "C01\u{2014}Title",
            "C01 \u{2014}",
            "C01 - ",
            "C01-02 - Title",
            "c01 - Title",
            "Overview - C01 - Title",
            "C01 ~ Title",
            "C01 -- Title",
        ] {
            assert_eq!(claim_heading(heading), None, "{heading:?}");
        }
    }

    #[test]
    fn dropped_claim_like_headings_are_listed_with_lines() {
        let md = "# Claims\n## C01: Kept\n## C02\u{2014}Dropped\n## C03\n## Claims overview\n## C4x - not an id\n## C05 - Kept\n";
        assert_eq!(
            unparsed_claim_headings(md),
            [(3, "C02\u{2014}Dropped"), (4, "C03")]
        );
    }

    #[test]
    fn unclosed_code_fences_hiding_claim_headings_are_listed() {
        let md = "# Claims\n## C01: A\n```\n## C02: Shown\n```\n- x\n~~~\n# notes\n### c 03 - B\n## **C04**: D\n## Cx\n";
        assert_eq!(
            fenced_claim_headings(md),
            [(9, "### c 03 - B"), (10, "## **C04**: D")]
        );
        // Closed fences, and fences opened inside field continuations, hide nothing.
        assert!(fenced_claim_headings("## C01: A\n```\n## C02: B\n```\n").is_empty());
        assert!(
            fenced_claim_headings("## C01: A\n- **Statement**:\n  ```\n## C02: B\n").is_empty()
        );
        // A stray-recovered document is scanned after its opener.
        let stray = "---\n# Claims\n\n## C01: A\n- **Statement**: a\n```\n## C02: B\n";
        assert!(fenced_claim_headings(stray).is_empty());
        assert_eq!(hidden_claim_headings(stray, 4), [(7, "## C02: B")]);
    }

    #[test]
    fn duplicate_dash_and_colon_claims_remain_duplicates() {
        let out = parse_claims("## C01: First\n## C01 \u{2014} Second\n");
        assert_eq!(out.claims.len(), 1);
        assert_eq!(out.duplicate_ids, ["C01"]);
    }

    #[test]
    fn stray_fence_claims_are_read_and_metadata_stays_hidden() {
        let recovered = parse_claims("---\n# Claims\n\n## C01: A\n- **Statement**: x\n");
        assert_eq!(recovered.claims.len(), 1);
        let protected = parse_claims("---\ntitle: x\n# Claims\n## C01: A\n- **Statement**: x\n");
        assert!(protected.claims.is_empty());
        let closed = parse_claims("---\ntype: claims\n---\n## C01: A\n");
        assert_eq!(closed.claims.len(), 1);
    }

    #[test]
    fn proof_extracts_multiple_evidence_ids() {
        let md = "## C02: multi\n- **Proof**: [E01, E02]\n";
        let out = parse_claims(md);
        assert_eq!(out.claims[0].proof, vec!["E01", "E02"]);
    }
}

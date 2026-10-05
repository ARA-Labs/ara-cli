//! Conservative recovery of a stray, unclosed leading `---` line.
//!
//! [`crate::markdown::frontmatter_range`] treats an unclosed leading fence as
//! frontmatter running to EOF, so malformed metadata never becomes selectable
//! Markdown. That stays the rule for every document. The one exception is
//! `logic/claims.md` when the text after the opener is provably a claims
//! document rather than metadata: optional blank lines, the exact title
//! `# Claims`, blank lines, an unindented claim heading whose first content is
//! a known claim field with a value. After the opener every level-two section
//! must be a distinct claim and no other level-one title may appear. Anything
//! else, including YAML-looking content, comments or a heading-only block,
//! stays hidden. Source bytes are never rewritten.

use std::collections::BTreeSet;

use crate::markdown;
use crate::report::ParseReport;
use crate::rules::RuleCode;

/// The only document with a stray-fence exception.
pub const CLAIMS: &str = "logic/claims.md";

/// Claim fields the claims parser models; the first claim must open with one.
const CLAIM_FIELDS: [&str; 11] = [
    "statement",
    "status",
    "proof",
    "dependencies",
    "provenance",
    "falsification",
    "falsification criteria",
    "conditions",
    "sources",
    "tags",
    "last revised",
];

/// How a document's unclosed leading `---` is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnclosedFence {
    /// A recognized stray line in `logic/claims.md`: the content after it is read.
    Recovered,
    /// Possible metadata: the rest of the document stays hidden.
    Protected,
}

/// The one-based line and handling of `md`'s unclosed leading fence, if any.
pub fn unclosed_fence(path: &str, md: &str) -> Option<(usize, UnclosedFence)> {
    let (line, opener_end) = unclosed_opener(md)?;
    let handling = if path == CLAIMS && recovered(md, opener_end) {
        UnclosedFence::Recovered
    } else {
        UnclosedFence::Protected
    };
    Some((line, handling))
}

/// Byte offset where the native document `path`'s Markdown headings begin.
pub fn body_start(path: &str, md: &str) -> usize {
    if path == CLAIMS
        && let Some((_, opener_end)) = unclosed_opener(md)
        && recovered(md, opener_end)
    {
        return opener_end;
    }
    markdown::body_start(md)
}

/// Headings an unclosed leading `---` hides: those after its opener line.
pub(crate) fn hidden_headings(md: &str) -> Vec<markdown::MarkdownHeading<'_>> {
    unclosed_opener(md).map_or_else(Vec::new, |(_, end)| markdown::headings_from(md, end))
}

/// Records one warning naming `path`'s unclosed leading fence line.
pub(crate) fn report(path: &str, md: &str, report: &mut ParseReport) {
    let Some((line, handling)) = unclosed_fence(path, md) else {
        return;
    };
    let location = format!("{path}:{line}");
    match handling {
        UnclosedFence::Recovered => report.warn(
            RuleCode::RecoveredStrayFence,
            location,
            "unclosed leading `---` is a stray line before `# Claims`; the claims after it are read",
        ),
        UnclosedFence::Protected => report.warn(
            RuleCode::UnclosedFrontmatter,
            location,
            "unclosed leading `---` hides the rest of the document as frontmatter; close or remove it",
        ),
    }
}

/// `(line, end of opener line)` for a leading `---` with no closing fence,
/// using exactly [`markdown::frontmatter_range`]'s line rules.
fn unclosed_opener(md: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    let mut opener = None;
    for (index, line) in md.split_inclusive('\n').enumerate() {
        let text = line.trim_end_matches(['\r', '\n']);
        let text = if offset == 0 {
            text.strip_prefix('\u{feff}').unwrap_or(text)
        } else {
            text
        };
        match opener {
            Some(_) if text.trim() == "---" => return None,
            Some(_) => {}
            None if text.trim().is_empty() => {}
            None if text.trim() == "---" => opener = Some((index + 1, offset + line.len())),
            None => return None,
        }
        offset += line.len();
    }
    opener
}

/// Whether the text after the opener is the supported claims document.
fn recovered(md: &str, start: usize) -> bool {
    let headings = markdown::headings_from(md, start);
    let [title, first, rest @ ..] = headings.as_slice() else {
        return false;
    };
    let line = |heading: &markdown::MarkdownHeading<'_>| {
        md[heading.range.start..heading.body_range.start].trim_end_matches(['\r', '\n'])
    };
    let blank = |from: usize, to: usize| md[from..to].trim().is_empty();
    if title.level != 1
        || line(title) != "# Claims"
        || !blank(start, title.range.start)
        || first.level != 2
        || !line(first).starts_with("## ")
        || !blank(title.body_range.start, first.range.start)
    {
        return false;
    }
    let mut ids = BTreeSet::new();
    for heading in std::iter::once(first).chain(rest) {
        let distinct_claim =
            crate::claims::claim_heading(heading.heading).is_some_and(|(id, _)| ids.insert(id));
        if heading.level == 1 || heading.level == 2 && !distinct_claim {
            return false;
        }
    }
    let body = &md[first.body_range.clone()];
    let content = first.body_range.start + body.len() - body.trim_start().len();
    markdown::fields(md, first.body_range.clone())
        .first()
        .is_some_and(|field| {
            field.range.start == content
                && CLAIM_FIELDS.contains(&field.name.to_ascii_lowercase().as_str())
                && !markdown::decode_field(field).trim().is_empty()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAIM: &str = "## C01: Speedup\n- **Statement**: Training is faster.\n";

    fn handling(md: &str) -> Option<UnclosedFence> {
        unclosed_fence(CLAIMS, md).map(|(_, handling)| handling)
    }

    #[test]
    fn reported_stray_opener_is_recovered_with_its_line() {
        for md in [
            format!("---\n# Claims\n\n{CLAIM}"),
            format!("\u{feff}\n\n---\n\n# Claims\n\n{CLAIM}\n## C02 — Next\n- **Status**: open\n"),
            format!("---\r\n# Claims\r\n\r\n{}", CLAIM.replace('\n', "\r\n")),
        ] {
            assert_eq!(handling(&md), Some(UnclosedFence::Recovered), "{md:?}");
            let start = body_start(CLAIMS, &md);
            assert!(md[start..].trim_start().starts_with("# Claims"), "{md:?}");
        }
        let md = format!("\n\n---\n# Claims\n{CLAIM}");
        assert_eq!(unclosed_fence(CLAIMS, &md).unwrap().0, 3);
    }

    #[test]
    fn only_claims_md_can_recover() {
        let md = format!("---\n# Claims\n\n{CLAIM}");
        assert_eq!(
            unclosed_fence("logic/problem.md", &md),
            Some((1, UnclosedFence::Protected))
        );
        assert_eq!(body_start("logic/problem.md", &md), md.len());
    }

    #[test]
    fn closed_or_absent_frontmatter_is_not_an_unclosed_fence() {
        for md in [
            format!("---\ntype: claims\n---\n# Claims\n{CLAIM}"),
            format!("---\n# Claims\n{CLAIM}---\n"),
            format!("# Claims\n{CLAIM}"),
            String::new(),
        ] {
            assert_eq!(handling(&md), None, "{md:?}");
            assert_eq!(body_start(CLAIMS, &md), markdown::body_start(&md), "{md:?}");
        }
    }

    #[test]
    fn uncertain_metadata_stays_protected() {
        let cases = [
            // YAML mappings, including empty values and malformed colons.
            "---\ntitle:\n# Claims\n## C01: A\n- **Statement**: x\n",
            "---\ntitle: Paper\n# Claims\n## C01: A\n- **Statement**: x\n",
            "---\ntitle Paper\n# Claims\n## C01: A\n- **Statement**: x\n",
            "---\n: value\n# Claims\n## C01: A\n- **Statement**: x\n",
            "---\n\"quoted\": 1\n# Claims\n## C01: A\n- **Statement**: x\n",
            "---\n? explicit\n: key\n# Claims\n## C01: A\n- **Statement**: x\n",
            // Sequences, flow collections, anchors, aliases, tags, directives.
            "---\n- item\n# Claims\n## C01: A\n- **Statement**: x\n",
            "---\n[a, b]\n# Claims\n## C01: A\n- **Statement**: x\n",
            "---\n{a: 1}\n# Claims\n## C01: A\n- **Statement**: x\n",
            "---\nbase: &anchor 1\n# Claims\n## C01: A\n- **Statement**: x\n",
            "---\nref: *anchor\n# Claims\n## C01: A\n- **Statement**: x\n",
            "---\nnum: !!int 1\n# Claims\n## C01: A\n- **Statement**: x\n",
            "---\n%YAML 1.2\n# Claims\n## C01: A\n- **Statement**: x\n",
            // Block scalars and indentation.
            "---\nnote: |\n  # Claims\n## C01: A\n- **Statement**: x\n",
            "---\n  # Claims\n## C01: A\n- **Statement**: x\n",
            "---\n# Claims\n  ## C01: A\n- **Statement**: x\n",
            // Comments followed by metadata; comment headings without bodies.
            "---\n# Claims\ntitle: Paper\n## C01: A\n- **Statement**: x\n",
            "---\n# Claims\n## C01: A\ntitle: Paper\n",
            "---\n# Claims\n## C01: A\n",
            "---\n# Claims\n## C01: A\n# comment\n- **Statement**: x\n",
            "---\n# Claims\n## C01: A\n<!-- note -->\n- **Statement**: x\n",
            "---\n# Claims\n## C01: A\n- **Statement**:\n",
            "---\n# Claims\n## C01: A\n- **Unknown**: x\n",
            "---\n# Claims\n",
            // A second title, a near-miss title, a non-claim section, duplicates.
            "---\n# Claims\n# Paper\n## C01: A\n- **Statement**: x\n",
            "---\n# claims\n## C01: A\n- **Statement**: x\n",
            "---\n# Claims \n## C01: A\n- **Statement**: x\n",
            "---\n# Claims\n## Overview\n## C01: A\n- **Statement**: x\n",
            "---\n# Claims\n## C01: A\n- **Statement**: x\n## Notes\nprose\n",
            "---\n# Claims\n## C01: A\n- **Statement**: x\n# Appendix\n",
            "---\n# Claims\n## C01: A\n- **Statement**: x\n## C01: B\n- **Statement**: y\n",
        ];
        for md in cases {
            assert_eq!(handling(md), Some(UnclosedFence::Protected), "{md:?}");
            assert_eq!(body_start(CLAIMS, md), md.len(), "{md:?}");
            assert!(
                markdown::document_headings(CLAIMS, md).is_empty(),
                "metadata became headings: {md:?}"
            );
        }
    }

    #[test]
    fn one_warning_names_the_opener_line_and_handling() {
        let mut diagnostics = ParseReport::default();
        report(
            CLAIMS,
            &format!("\n---\n# Claims\n{CLAIM}"),
            &mut diagnostics,
        );
        report(CLAIMS, "---\ntitle: x\n", &mut diagnostics);
        report(CLAIMS, "---\ntitle: x\n---\n", &mut diagnostics);
        let found: Vec<_> = diagnostics
            .warnings()
            .iter()
            .map(|d| (d.code, d.path.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                (RuleCode::RecoveredStrayFence, "logic/claims.md:2"),
                (RuleCode::UnclosedFrontmatter, "logic/claims.md:1"),
            ]
        );
        assert!(diagnostics.is_ok());
    }
}

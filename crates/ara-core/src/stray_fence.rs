//! Conservative recovery of a stray, unclosed leading `---` line.
//!
//! [`crate::markdown::frontmatter_range`] treats an unclosed leading fence as
//! frontmatter running to EOF, so malformed metadata never becomes selectable
//! Markdown. That stays the rule for every document except the typed logic
//! documents below, and only when the text after the opener is provably that
//! document rather than metadata.
//!
//! - `logic/claims.md`: optional blank lines, the exact title `# Claims`,
//!   blank lines, an unindented claim heading whose first content is a known
//!   claim field with a value. After the opener every level-two section must
//!   be a distinct claim, no other level-one title may appear, and no code
//!   fence left open at EOF may hide a claim-like heading.
//! - `logic/experiments.md`, `logic/solution/heuristics.md` and
//!   `logic/problem.md`: optional blank lines, one level-one title, then only
//!   blank lines and headings down to the first typed entry (`## E01: …`,
//!   `## H01: …`, `### O1: …`), whose first content is a known field of that
//!   document with a value. Every heading at the entry level must be a
//!   distinct typed entry, no other level-one title may appear, and no code
//!   fence may be left open at EOF.
//! - Every other `logic/` Markdown document (`concepts.md`,
//!   `related_work.md`, untyped `solution/` documents): optional blank lines,
//!   one level-one title, blank lines, a level-two section, no other
//!   level-one title and no code fence left open at EOF, and the text after
//!   the opener must not load as a YAML mapping (metadata) or null (a
//!   heading-only block is all YAML comments).
//!
//! Anything else, including YAML-looking content, comments or a heading-only
//! block, stays hidden. Source bytes are never rewritten.

use std::collections::BTreeSet;

use crate::manifest::SourceValue;
use crate::markdown;
use crate::report::ParseReport;
use crate::rules::RuleCode;

/// The claims document, whose stray-fence exception predates the typed ones.
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

/// A typed logic document whose stray opener can be recovered.
struct Typed {
    path: &'static str,
    /// Heading level of the document's entries.
    entry_level: usize,
    /// Letters that start an entry ID, followed by digits (`E01`, `O3`).
    prefixes: &'static [char],
    /// Lowercase field names an entry may open with.
    fields: &'static [&'static str],
}

const TYPED: [Typed; 3] = [
    Typed {
        path: "logic/experiments.md",
        entry_level: 2,
        prefixes: &['E'],
        fields: &[
            "verifies",
            "evidence",
            "run",
            "setup",
            "procedure",
            "metrics",
            "expected outcome",
            "baselines",
            "dependencies",
            "question",
            "prediction (directional)",
            "predictions (directional)",
            "falsification condition",
            "status",
            "sources",
            "provenance",
        ],
    },
    Typed {
        path: "logic/solution/heuristics.md",
        entry_level: 2,
        prefixes: &['H'],
        fields: &[
            "rationale",
            "source",
            "sources",
            "status",
            "provenance",
            "sensitivity",
            "bounds",
            "code ref",
            "tags",
        ],
    },
    Typed {
        path: "logic/problem.md",
        entry_level: 3,
        prefixes: &['O', 'G'],
        fields: &[
            "statement",
            "evidence",
            "implication",
            "caused by",
            "existing attempts",
            "why they fail",
            "why it matters",
            "insight",
            "derived from",
            "enables",
            "conditions",
            "sources",
            "provenance",
        ],
    },
];

/// How a document's unclosed leading `---` is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnclosedFence {
    /// A recognized stray line in a typed logic document: the content after it is read.
    Recovered,
    /// Possible metadata: the rest of the document stays hidden.
    Protected,
}

/// The one-based line and handling of `md`'s unclosed leading fence, if any.
pub fn unclosed_fence(path: &str, md: &str) -> Option<(usize, UnclosedFence)> {
    let (line, opener_end) = unclosed_opener(md)?;
    let handling = if recovered(path, md, opener_end) {
        UnclosedFence::Recovered
    } else {
        UnclosedFence::Protected
    };
    Some((line, handling))
}

/// Byte offset where the native document `path`'s Markdown headings begin.
pub fn body_start(path: &str, md: &str) -> usize {
    if let Some((_, opener_end)) = unclosed_opener(md)
        && recovered(path, md, opener_end)
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
        UnclosedFence::Recovered if path == CLAIMS => report.warn(
            RuleCode::RecoveredStrayFence,
            location,
            "unclosed leading `---` is a stray line before `# Claims`; the claims after it are read",
        ),
        UnclosedFence::Recovered => report.warn(
            RuleCode::RecoveredStrayFence,
            location,
            "unclosed leading `---` is a stray line before the document title; the content after it is read",
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

/// Whether the text after the opener is the logic document `path`.
fn recovered(path: &str, md: &str, start: usize) -> bool {
    if path == CLAIMS {
        return claims_recovered(md, start);
    }
    if let Some(typed) = TYPED.iter().find(|typed| typed.path == path) {
        return typed_recovered(typed, md, start);
    }
    path.starts_with("logic/") && path.ends_with(".md") && prose_recovered(md, start)
}

/// Whether the text after the opener is a titled Markdown document of prose
/// sections rather than YAML metadata.
fn prose_recovered(md: &str, start: usize) -> bool {
    let headings = markdown::headings_from(md, start);
    let [title, first, rest @ ..] = headings.as_slice() else {
        return false;
    };
    let line = |heading: &markdown::MarkdownHeading<'_>| {
        md[heading.range.start..heading.body_range.start].trim_end_matches(['\r', '\n'])
    };
    let blank = |from: usize, to: usize| md[from..to].trim().is_empty();
    if title.level != 1
        || !line(title).starts_with("# ")
        || title.heading.trim().is_empty()
        || !blank(start, title.range.start)
        || first.level != 2
        || !blank(title.body_range.start, first.range.start)
        || rest.iter().any(|heading| heading.level == 1)
        || !markdown::unclosed_code_fence_lines(md, start).is_empty()
    {
        return false;
    }
    // Headings are YAML comments. Frontmatter metadata is a mapping (as
    // `PAPER.md`'s must be) and a heading-only block loads as null; Markdown
    // bullet lists load as YAML lists, and prose fails to load.
    !matches!(
        serde_saphyr::from_str::<SourceValue>(&md[start..]),
        Ok(SourceValue::Mapping(_) | SourceValue::Null)
    )
}

/// Whether the text after the opener is the typed document `typed`.
fn typed_recovered(typed: &Typed, md: &str, start: usize) -> bool {
    let headings = markdown::headings_from(md, start);
    let Some((title, rest)) = headings.split_first() else {
        return false;
    };
    let line = |heading: &markdown::MarkdownHeading<'_>| {
        md[heading.range.start..heading.body_range.start].trim_end_matches(['\r', '\n'])
    };
    let blank = |from: usize, to: usize| md[from..to].trim().is_empty();
    if title.level != 1
        || !line(title).starts_with("# ")
        || title.heading.trim().is_empty()
        || !blank(start, title.range.start)
        || !markdown::unclosed_code_fence_lines(md, start).is_empty()
    {
        return false;
    }
    // Only blank lines and headings may precede the first entry.
    let Some(first) = rest.iter().position(|h| h.level == typed.entry_level) else {
        return false;
    };
    let mut gap_start = title.body_range.start;
    for heading in &rest[..=first] {
        if !blank(gap_start, heading.range.start) {
            return false;
        }
        gap_start = heading.body_range.start;
    }
    let mut ids = BTreeSet::new();
    for heading in rest {
        let distinct_entry = heading.level != typed.entry_level
            || entry_id(heading.heading, typed.prefixes).is_some_and(|id| ids.insert(id));
        if heading.level == 1 || !distinct_entry {
            return false;
        }
    }
    let entry = &rest[first];
    let body = &md[entry.body_range.clone()];
    let content = entry.body_range.start + body.len() - body.trim_start().len();
    markdown::fields(md, entry.body_range.clone())
        .first()
        .is_some_and(|field| {
            field.range.start == content
                && typed
                    .fields
                    .contains(&field.name.to_ascii_lowercase().as_str())
                && !markdown::decode_field(field).trim().is_empty()
        })
}

/// The ID of an entry heading `<prefix><digits>: <title>`.
fn entry_id<'a>(heading: &'a str, prefixes: &[char]) -> Option<&'a str> {
    let (id, title) = heading.split_once(':')?;
    let id = id.trim();
    let digits = id.strip_prefix(prefixes)?;
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) && !title.trim().is_empty())
        .then_some(id)
}

/// Whether the text after the opener is the supported claims document.
fn claims_recovered(md: &str, start: usize) -> bool {
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
    // A code fence left open at EOF hides later claims: not recoverable.
    if !crate::claims::hidden_claim_headings(md, start).is_empty() {
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

    const EXPERIMENTS: &str = "logic/experiments.md";
    const HEURISTICS: &str = "logic/solution/heuristics.md";
    const PROBLEM: &str = "logic/problem.md";
    const EXPERIMENT: &str =
        "## E01: Benchmark posterior approximation\n- **Verifies**: C01, C02\n";
    const HEURISTIC: &str =
        "## H01: Random condition mask sampling\n- **Rationale**: Mixed masks.\n";
    const OBSERVATION: &str =
        "## Observations\n\n### O1: Tabular data only\n- **Statement**: Fixed-size pairs.\n";

    fn typed_handling(path: &str, md: &str) -> Option<UnclosedFence> {
        unclosed_fence(path, md).map(|(_, handling)| handling)
    }

    /// The corpus shape (plan 22): typed logic documents opening with a stray
    /// `---` keep their entries addressable.
    #[test]
    fn stray_opener_before_typed_entries_is_recovered() {
        for (path, md) in [
            (
                EXPERIMENTS,
                format!("---\n# Experiments\n\n{EXPERIMENT}\n## E02: Ablation\n- **Setup**: x\n"),
            ),
            (
                EXPERIMENTS,
                format!("\n---\n\n# Experiment Plans\n\n{EXPERIMENT}"),
            ),
            (
                HEURISTICS,
                format!("---\n# Heuristics and Design Tricks\n\n{HEURISTIC}"),
            ),
            (
                PROBLEM,
                format!(
                    "---\n# Problem Specification\n\n{OBSERVATION}\n## Gaps\n\n### G1: None\n- **Statement**: y\n\n## Key Insight\nprose\n"
                ),
            ),
            (
                PROBLEM,
                format!(
                    "---\r\n# Problem Specification\r\n\r\n{}",
                    OBSERVATION.replace('\n', "\r\n")
                ),
            ),
        ] {
            assert_eq!(
                typed_handling(path, &md),
                Some(UnclosedFence::Recovered),
                "{md:?}"
            );
            let start = body_start(path, &md);
            assert!(md[start..].trim_start().starts_with("# "), "{md:?}");
            assert!(!markdown::document_headings(path, &md).is_empty(), "{md:?}");
        }
    }

    #[test]
    fn uncertain_typed_documents_stay_protected() {
        let cases = [
            // Metadata or prose before the title, or between title and entry.
            (EXPERIMENTS, format!("---\ntitle: x\n# Experiments\n\n{EXPERIMENT}")),
            (EXPERIMENTS, format!("---\n# Experiments\ntitle: x\n{EXPERIMENT}")),
            (PROBLEM, "---\n# Problem Specification\n\n## Observations\nnote: x\n### O1: A\n- **Statement**: x\n".to_string()),
            // No title, a second title, an untyped or misnumbered entry.
            (EXPERIMENTS, format!("---\n{EXPERIMENT}")),
            (EXPERIMENTS, format!("---\n# Experiments\n\n{EXPERIMENT}# Appendix\n")),
            (EXPERIMENTS, format!("---\n# Experiments\n\n## Overview\n\n{EXPERIMENT}")),
            (EXPERIMENTS, format!("---\n# Experiments\n\n{EXPERIMENT}## Notes\nprose\n")),
            (HEURISTICS, "---\n# Heuristics\n\n## E01: Wrong prefix\n- **Rationale**: x\n".to_string()),
            (HEURISTICS, "---\n# Heuristics\n\n## H1a: Not an ID\n- **Rationale**: x\n".to_string()),
            (PROBLEM, "---\n# Problem Specification\n\n## Observations\n\n### Note: A\n- **Statement**: x\n".to_string()),
            // Duplicate IDs; entries without a known, valued first field.
            (HEURISTICS, format!("---\n# Heuristics\n\n{HEURISTIC}{HEURISTIC}")),
            (HEURISTICS, "---\n# Heuristics\n\n## H01: A\n- **Unknown**: x\n".to_string()),
            (HEURISTICS, "---\n# Heuristics\n\n## H01: A\n- **Rationale**:\n".to_string()),
            (HEURISTICS, "---\n# Heuristics\n\n## H01: A\nprose\n- **Rationale**: x\n".to_string()),
            (EXPERIMENTS, "---\n# Experiments\n".to_string()),
            // An unclosed code fence.
            (EXPERIMENTS, format!("---\n# Experiments\n\n{EXPERIMENT}```\n## E02: B\n")),
        ];
        for (path, md) in cases {
            assert_eq!(
                typed_handling(path, &md),
                Some(UnclosedFence::Protected),
                "{path}: {md:?}"
            );
            assert_eq!(body_start(path, &md), md.len(), "{path}: {md:?}");
            assert!(
                markdown::document_headings(path, &md).is_empty(),
                "{path}: {md:?}"
            );
        }
    }

    const ALGORITHM: &str = "logic/solution/algorithm.md";

    /// Untyped logic documents in the corpus shape recover under the prose rule.
    #[test]
    fn stray_opener_before_prose_sections_is_recovered() {
        for (path, md) in [
            (
                ALGORITHM,
                "---\n# Algorithm\n\n## Mathematical Formulation\n\nThe loss is $L = a + b$: a sum.\n\n### Objective\n- term one\n",
            ),
            (
                "logic/solution/constraints.md",
                "---\n# Constraints and Limitations\n\n## Boundary Conditions\n\n- **BC1**: Needs a simulator.\n",
            ),
            (
                "logic/solution/constraints.md",
                "---\n# Constraints\n\n## Known Limitations\n\n### Computational\n- Slow sampling\n- C1: Needs a good initialization.\n",
            ),
            (
                "logic/concepts.md",
                "---\n# Concepts\n\n## Simformer\n- **Definition**: A diffusion model.\n",
            ),
            (
                "logic/related_work.md",
                "\n---\n\n# Related Work\n\n## RW01: Weilbach et al., 2023\n- **Type**: baseline\n",
            ),
        ] {
            assert_eq!(
                typed_handling(path, md),
                Some(UnclosedFence::Recovered),
                "{md:?}"
            );
            assert!(
                md[body_start(path, md)..].trim_start().starts_with("# "),
                "{md:?}"
            );
            assert!(!markdown::document_headings(path, md).is_empty(), "{md:?}");
        }
    }

    #[test]
    fn metadata_like_prose_documents_stay_protected() {
        let cases = [
            // YAML mappings behind headings (YAML comments).
            (
                ALGORITHM,
                "---\n# Algorithm\n\n## Overview\ntitle: Paper\nyear: 2024\n",
            ),
            (
                ALGORITHM,
                "---\n# Algorithm\n\n## Overview\nauthors:\n  - A\n",
            ),
            // A heading-only block, a missing title or section, a second title.
            (ALGORITHM, "---\n# Algorithm\n\n## Overview\n"),
            (ALGORITHM, "---\n## Overview\n\nprose\n"),
            (ALGORITHM, "---\n# Algorithm\n\nprose\n"),
            (
                ALGORITHM,
                "---\n# Algorithm\n\n## Overview\nprose\n# Appendix\nmore\n",
            ),
            // Content before the title or between the title and the first section.
            (ALGORITHM, "---\nnote\n# Algorithm\n\n## Overview\nprose\n"),
            (ALGORITHM, "---\n# Algorithm\nintro\n## Overview\nprose\n"),
            // An unclosed code fence; documents outside `logic/`.
            (
                ALGORITHM,
                "---\n# Algorithm\n\n## Overview\nprose\n```\n## Hidden\n",
            ),
            ("PAPER.md", "---\n# Paper\n\n## Overview\nprose\n"),
            ("staging/notes.md", "---\n# Notes\n\n## Overview\nprose\n"),
        ];
        for (path, md) in cases {
            assert_eq!(
                typed_handling(path, md),
                Some(UnclosedFence::Protected),
                "{path}: {md:?}"
            );
            assert_eq!(body_start(path, md), md.len(), "{path}: {md:?}");
        }
    }

    #[test]
    fn a_claims_body_recovers_only_in_claims_md() {
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
            // An unclosed code fence swallowing a later claim.
            "---\n# Claims\n## C01: A\n- **Statement**: x\n```\n## C02: B\n",
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

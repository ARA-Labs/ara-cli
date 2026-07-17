//! Lenient reader for `PAPER.md` YAML frontmatter.
//!
//! `PAPER.md` opens with a `---` fenced YAML block carrying paper metadata,
//! followed by prose. Some artifacts (e.g. the `ls20` demo) skip the fence and
//! open with a `# H1` heading; those still yield a [`PaperMeta`] whose only
//! field is the title. The frontmatter is deserialized through a tolerant
//! permissive struct (`serde-saphyr` confined here, mirroring `schema.rs`):
//! unknown keys (`domain`, `claims_summary`, rebench's `task`/`sources`/`scores`)
//! land in `extra` and are ignored, never errors. Malformed YAML pushes a
//! warning and yields `None` — never a panic.

use serde::Deserialize;
use serde::de::IgnoredAny;
use std::collections::BTreeMap;

use crate::manifest::PaperMeta;

/// Parses `PAPER.md`. Returns the metadata (when recoverable) plus any warnings.
///
/// - A leading `---` fence delimits the frontmatter; its YAML is parsed.
/// - With no fence, only the first `# H1` becomes the title.
/// - Malformed frontmatter YAML yields `(None, [warning])`.
pub fn parse_paper(md: &str) -> (Option<PaperMeta>, Vec<String>) {
    match extract_frontmatter(md) {
        Some(yaml) => match serde_saphyr::from_str::<RawPaper>(yaml) {
            Ok(raw) => (Some(raw.into_meta()), Vec::new()),
            Err(e) => (
                None,
                vec![format!("malformed frontmatter YAML: {e}")],
            ),
        },
        None => {
            // No frontmatter fence: recover the title from the first `# H1`.
            let title = first_h1(md);
            if title.is_none() {
                (None, Vec::new())
            } else {
                (
                    Some(PaperMeta {
                        title,
                        ..PaperMeta::default()
                    }),
                    Vec::new(),
                )
            }
        }
    }
}

/// Returns the text between a leading `---` fence and the next `---` line, or
/// `None` when the document does not open with a fence. A leading UTF-8 BOM and
/// surrounding blank lines are tolerated.
fn extract_frontmatter(md: &str) -> Option<&str> {
    let md = md.strip_prefix('\u{feff}').unwrap_or(md);
    let mut lines = md.lines();
    // The first non-empty line must be exactly `---`.
    let first = lines.by_ref().find(|l| !l.trim().is_empty())?;
    if first.trim() != "---" {
        return None;
    }
    // Find the byte range from just after the opening fence to the closing one.
    let after_open = md.find("---")? + 3;
    let rest = &md[after_open..];
    let rest = rest.strip_prefix('\r').unwrap_or(rest);
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    // The closing fence is a line that is exactly `---`.
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']).trim() == "---" {
            return Some(&rest[..offset]);
        }
        offset += line.len();
    }
    None
}

/// The text of the first `# H1` heading, when present.
pub(crate) fn first_h1(md: &str) -> Option<String> {
    md.lines()
        .map(str::trim_start)
        .find_map(|l| l.strip_prefix("# "))
        .map(|t| t.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Tolerant frontmatter mirror. Known keys are modeled; everything else (e.g.
/// `domain`, `claims_summary`, `ara_version`) falls into `extra` and is dropped.
#[derive(Debug, Deserialize)]
struct RawPaper {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    year: Option<IntOrStr>,
    #[serde(default)]
    venue: Option<String>,
    #[serde(default)]
    doi: Option<String>,
    #[serde(default, rename = "abstract")]
    abstract_: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(flatten)]
    #[allow(dead_code)]
    extra: BTreeMap<String, IgnoredAny>,
}

impl RawPaper {
    fn into_meta(self) -> PaperMeta {
        PaperMeta {
            title: self.title.filter(|s| !s.trim().is_empty()),
            authors: self.authors,
            year: self.year.map(|y| y.into_string()),
            venue: self.venue,
            doi: self.doi.filter(|s| !s.trim().is_empty()),
            abstract_: self.abstract_,
            keywords: self.keywords,
        }
    }
}

/// `year:` may be an integer (`2024`) or a string (`"2024"`); both normalize to
/// a `String`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum IntOrStr {
    Int(i64),
    Str(String),
}

impl IntOrStr {
    fn into_string(self) -> String {
        match self {
            IntOrStr::Int(i) => i.to_string(),
            IntOrStr::Str(s) => s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FENCED: &str = "\
---
title: \"Attention Is All You Need\"
authors: [\"Ashish Vaswani\", \"Noam Shazeer\"]
year: 2017
venue: \"NeurIPS 2017\"
doi: \"arXiv:1706.03762\"
domain: \"Machine Translation\"
abstract: \"We propose the Transformer.\"
keywords:
  - attention
  - transformer
---

# Attention Is All You Need

Body prose.
";

    #[test]
    fn fenced_frontmatter_parses_all_fields() {
        let (paper, warns) = parse_paper(FENCED);
        assert!(warns.is_empty(), "no warnings expected: {warns:?}");
        let p = paper.expect("some");
        assert_eq!(p.title.as_deref(), Some("Attention Is All You Need"));
        assert_eq!(p.authors, vec!["Ashish Vaswani", "Noam Shazeer"]);
        assert_eq!(p.year.as_deref(), Some("2017")); // int normalized to string
        assert_eq!(p.venue.as_deref(), Some("NeurIPS 2017"));
        assert_eq!(p.doi.as_deref(), Some("arXiv:1706.03762"));
        assert_eq!(p.abstract_.as_deref(), Some("We propose the Transformer."));
        assert_eq!(p.keywords, vec!["attention", "transformer"]);
    }

    #[test]
    fn year_as_string_is_kept() {
        let md = "---\ntitle: T\nyear: \"2019\"\n---\n";
        let (paper, _) = parse_paper(md);
        assert_eq!(paper.unwrap().year.as_deref(), Some("2019"));
    }

    #[test]
    fn doi_null_becomes_none() {
        let md = "---\ntitle: T\ndoi: null\n---\n";
        let (paper, warns) = parse_paper(md);
        assert!(warns.is_empty());
        assert!(paper.unwrap().doi.is_none());
    }

    #[test]
    fn authors_inline_and_block_equivalent() {
        let inline = "---\ntitle: T\nauthors: [\"A\", \"B\"]\n---\n";
        let block = "---\ntitle: T\nauthors:\n  - A\n  - B\n---\n";
        let (a, _) = parse_paper(inline);
        let (b, _) = parse_paper(block);
        assert_eq!(a.unwrap().authors, vec!["A", "B"]);
        assert_eq!(b.unwrap().authors, vec!["A", "B"]);
    }

    #[test]
    fn no_fence_recovers_title_from_h1() {
        let md = "# World-Model ARA for ARC-AGI-3 ls20\n\nSome prose, no frontmatter.\n";
        let (paper, warns) = parse_paper(md);
        assert!(warns.is_empty());
        let p = paper.expect("some");
        assert_eq!(p.title.as_deref(), Some("World-Model ARA for ARC-AGI-3 ls20"));
        assert!(p.authors.is_empty());
        assert!(p.year.is_none());
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let md = "---\ntitle: T\ntask: something\nsources: [a, b]\nscores: {x: 1}\n---\n";
        let (paper, warns) = parse_paper(md);
        assert!(warns.is_empty(), "unknown keys must not warn: {warns:?}");
        assert_eq!(paper.unwrap().title.as_deref(), Some("T"));
    }

    #[test]
    fn folded_abstract_scalar_is_plain_string() {
        let md = "---\ntitle: T\nabstract: >\n  Line one\n  line two.\n---\n";
        let (paper, _) = parse_paper(md);
        let abs = paper.unwrap().abstract_.unwrap();
        assert!(abs.contains("Line one"), "got: {abs}");
    }

    #[test]
    fn malformed_frontmatter_warns_not_panics() {
        // A block-sequence item where a mapping is expected → YAML type error.
        let md = "---\ntitle: [unterminated\n---\n";
        let (paper, warns) = parse_paper(md);
        assert!(paper.is_none());
        assert_eq!(warns.len(), 1);
        assert!(warns[0].contains("malformed"), "got: {warns:?}");
    }

    #[test]
    fn empty_document_is_none_no_warning() {
        let (paper, warns) = parse_paper("");
        assert!(paper.is_none());
        assert!(warns.is_empty());
    }
}

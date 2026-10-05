//! Tiered heading lookup over one Markdown source and its section rows.
//!
//! Exact tiers come first: the full vector, then a literal suffix, then the
//! native-ID shorthand suffix. Tolerant tiers compare [`normalize`]d
//! segments: equality, then a prefix, and last a real trailing `...` that
//! stands for a longer request. Every tier rejects multiple matches before a
//! weaker tier runs.
use super::address;
use super::candidates::{self, Candidates, normalize};
use crate::output::{AgentError, excerpt};
use ara_core::markdown::MarkdownHeading;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub enum Lookup {
    Found(usize),
    Missing,
    Ambiguous(Vec<usize>),
}
type Same<'f> = &'f dyn Fn(&str, &str) -> bool;

pub struct Sections<'a> {
    document: &'a str,
    text: &'a str,
    items: Vec<MarkdownHeading<'a>>,
    /// One-based source-order occurrence among identical full vectors, when
    /// the vector repeats.
    occurrences: Vec<Option<usize>>,
}
impl<'a> Sections<'a> {
    pub fn new(document: &'a str, text: &'a str) -> Self {
        let items = ara_core::markdown::document_headings(document, text);
        let mut counts = BTreeMap::<&[&str], usize>::new();
        for item in &items {
            *counts.entry(&item.path).or_default() += 1;
        }
        let mut seen = BTreeMap::<&[&str], usize>::new();
        let occurrences = items
            .iter()
            .map(|item| {
                let ordinal = seen.entry(&item.path).or_default();
                *ordinal += 1;
                (counts[item.path.as_slice()] > 1).then_some(*ordinal)
            })
            .collect();
        Self {
            document,
            text,
            items,
            occurrences,
        }
    }
    fn select(&self, keep: impl Fn(&MarkdownHeading<'_>) -> bool) -> Lookup {
        let found: Vec<usize> = (0..self.items.len())
            .filter(|index| keep(&self.items[*index]))
            .collect();
        match found.as_slice() {
            [] => Lookup::Missing,
            [index] => Lookup::Found(*index),
            _ => Lookup::Ambiguous(found),
        }
    }
    fn suffix_tiers(&self, wanted: &[String], tiers: &[Same<'_>]) -> Lookup {
        for same in tiers {
            let lookup = self.select(|section| {
                section.path.len() >= wanted.len()
                    && section.path[section.path.len() - wanted.len()..]
                        .iter()
                        .zip(wanted)
                        .all(|(actual, wanted)| same(actual, wanted))
            });
            if !matches!(lookup, Lookup::Missing) {
                return lookup;
            }
        }
        Lookup::Missing
    }
    pub fn exact(&self, wanted: &[String]) -> Lookup {
        let full = self.select(|section| section.path == wanted);
        if !matches!(full, Lookup::Missing) {
            return full;
        }
        self.suffix_tiers(
            wanted,
            &[&|actual, wanted| actual == wanted, &heading_matches],
        )
    }
    pub fn tolerant(&self, wanted: &[String]) -> Lookup {
        let wanted: Vec<String> = wanted.iter().map(|segment| normalize(segment)).collect();
        let equal = |actual: &str, wanted: &str| normalize(actual) == wanted;
        let prefix = |actual: &str, wanted: &str| {
            let actual = normalize(actual);
            actual == wanted || !wanted.is_empty() && actual.starts_with(wanted)
        };
        let ellipsis = |actual: &str, wanted: &str| {
            prefix(actual, wanted)
                || normalize(actual).strip_suffix("...").is_some_and(|stem| {
                    !stem.trim().is_empty() && wanted.len() > stem.len() && wanted.starts_with(stem)
                })
        };
        self.suffix_tiers(&wanted, &[&equal, &prefix, &ellipsis])
    }
    /// Canonical heading addresses resolve the exact full vector only.
    pub fn canonical(&self, heading: &[String], occurrence: Option<usize>) -> Lookup {
        let lookup = self.select(|section| section.path == heading);
        match (lookup, occurrence) {
            (Lookup::Found(index), None | Some(1)) => Lookup::Found(index),
            (Lookup::Ambiguous(found), Some(n)) => found
                .get(n - 1)
                .map_or(Lookup::Missing, |index| Lookup::Found(*index)),
            (Lookup::Ambiguous(found), None) => Lookup::Ambiguous(found),
            _ => Lookup::Missing,
        }
    }
    /// Legacy flattened `path#A/B` input: every section with a heading
    /// suffix whose `/`-joined spelling equals `display`, each segment
    /// spelled literally or by its native-ID shorthand.
    pub fn display(&self, display: &str) -> Lookup {
        self.select(|section| {
            (0..section.path.len()).any(|start| spelled(&section.path[start..], display))
        })
    }
    pub fn address(&self, index: usize) -> String {
        address::heading(
            self.document,
            &self.items[index].path,
            self.occurrences[index],
        )
    }
    pub fn path(&self, index: usize) -> Vec<String> {
        self.items[index]
            .path
            .iter()
            .map(|h| h.to_string())
            .collect()
    }
    /// Every section, ranked against `label`.
    pub fn ranked(&self, label: &str) -> Candidates {
        Candidates::ranked(
            label,
            (0..self.items.len()).map(|index| (self.address(index), self.items[index].heading)),
        )
    }
    /// The matched sections, in source order.
    pub fn matched(&self, found: &[usize]) -> Candidates {
        Candidates::ordered(found.iter().map(|index| self.address(*index)))
    }
    /// Resolve a lookup to its source row or a read-facing error.
    pub fn row(
        &self,
        lookup: Lookup,
        id: &str,
        label: &str,
        heading: &[String],
        full: bool,
    ) -> Result<Value, AgentError> {
        match lookup {
            Lookup::Found(index) => Ok(self.section_row(index, heading, full)),
            Lookup::Ambiguous(found) => Err(candidates::ambiguous(id, self.matched(&found))),
            Lookup::Missing => Err(candidates::unknown(id, self.ranked(label))),
        }
    }
    /// A selected section. `heading` is the requested (or redirected) vector;
    /// `heading_path` and `address` name the section's full source vector.
    pub fn section_row(&self, index: usize, heading: &[String], full: bool) -> Value {
        let section = &self.items[index];
        let mut row = source_row(self.document, &self.text[section.body_range.clone()], full);
        let object = row.as_object_mut().expect("row object");
        object.insert("heading".into(), json!(heading));
        object.insert("heading_path".into(), json!(section.path));
        object.insert("address".into(), json!(self.address(index)));
        row
    }
}
/// Literal equality or the native-ID shorthand (`C04` names `C04: Title`).
pub fn heading_matches(actual: &str, wanted: &str) -> bool {
    actual == wanted || native_shorthand(actual, wanted)
}
/// Whether `display` spells `segments` joined by `/`, trying each segment's
/// literal text and its native-ID shorthand.
fn spelled(segments: &[&str], display: &str) -> bool {
    let Some((first, rest)) = segments.split_first() else {
        return false;
    };
    let shorthand = heading_id(first).filter(|id| native_shorthand(first, id));
    [Some(*first), shorthand]
        .into_iter()
        .flatten()
        .filter_map(|form| display.strip_prefix(form))
        .any(|tail| match tail.strip_prefix('/') {
            _ if rest.is_empty() => tail.is_empty(),
            Some(tail) => spelled(rest, tail),
            None => false,
        })
}
fn native_shorthand(actual: &str, wanted: &str) -> bool {
    wanted
        .as_bytes()
        .first()
        .is_some_and(|prefix| matches!(prefix, b'N' | b'C' | b'H' | b'E' | b'O' | b'T'))
        && wanted.len() > 1
        && wanted[1..].bytes().all(|byte| byte.is_ascii_digit())
        && heading_id(actual) == Some(wanted)
}
/// A claim heading's ID (any native separator), else the text before `:`.
pub(super) fn heading_id(heading: &str) -> Option<&str> {
    ara_core::claim_heading(heading)
        .map(|(id, _)| id)
        .or_else(|| heading.split_once(':').map(|(id, _)| id.trim()))
}
/// A whole-document or selected-range row; the digest covers exactly the
/// selected source bytes.
pub fn source_row(document: &str, content: &str, full: bool) -> Value {
    use sha2::{Digest, Sha256};
    let digest = format!("sha256:{:x}", Sha256::digest(content.as_bytes()));
    json!({"key":document,"kind":"source_document","document":document,"heading":[],"source":document,"address":address::document(document),"content":if full{content.to_owned()}else{excerpt(content)},"digest":digest})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }
    fn found(lookup: Lookup) -> Option<usize> {
        match lookup {
            Lookup::Found(index) => Some(index),
            _ => None,
        }
    }

    #[test]
    fn exact_tiers_reject_duplicates_before_weaker_tiers() {
        let sections = Sections::new("d.md", "# A\n## X\n## X\n## x: one\n");
        assert!(matches!(
            sections.exact(&vector(&["X"])),
            Lookup::Ambiguous(_)
        ));
        assert_eq!(found(sections.exact(&vector(&["A"]))), Some(0));
        assert_eq!(sections.address(1), "d.md#h/A/X;occurrence=1");
        assert_eq!(sections.address(3), "d.md#h/A/x%3A%20one");
        assert_eq!(
            found(sections.canonical(&vector(&["A", "X"]), Some(2))),
            Some(2)
        );
        assert!(matches!(
            sections.canonical(&vector(&["A", "X"]), Some(3)),
            Lookup::Missing
        ));
        assert!(matches!(
            sections.canonical(&vector(&["a"]), None),
            Lookup::Missing
        ));
    }

    #[test]
    fn shorthand_mixes_with_literal_segments() {
        let sections = Sections::new("d.md", "# Top\n## C04: Alpha\n## C04b extra\n");
        assert_eq!(found(sections.exact(&vector(&["Top", "C04"]))), Some(1));
        assert_eq!(found(sections.display("Top/C04")), Some(1));
        assert_eq!(found(sections.display("Top/C04: Alpha")), Some(1));
        assert!(matches!(sections.display("Top/C0"), Lookup::Missing));
    }

    #[test]
    fn tolerant_tiers_follow_equality_prefix_then_ellipsis() {
        let sections = Sections::new("d.md", "# Top\n## Long run...\n## Long\n## Other\n");
        assert_eq!(found(sections.tolerant(&vector(&[" LONG "]))), Some(2));
        assert_eq!(found(sections.tolerant(&vector(&["long run"]))), Some(1));
        assert_eq!(
            found(sections.tolerant(&vector(&["long running jobs"]))),
            Some(1)
        );
        assert!(matches!(
            sections.tolerant(&vector(&["lon"])),
            Lookup::Ambiguous(_)
        ));
        assert!(matches!(sections.tolerant(&vector(&[""])), Lookup::Missing));
    }
}

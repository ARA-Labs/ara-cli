//! Borrowing graph queries and a shared byte-range reference scanner.
//! No filesystem, clocks, or native dependencies.
use crate::{Claim, LinkKind, Manifest, Node};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

pub struct QueryIndex<'a> {
    pub manifest: &'a Manifest,
    nodes: BTreeMap<&'a str, &'a Node>,
    claims: BTreeMap<&'a str, &'a Claim>,
    parents: BTreeMap<&'a str, &'a str>,
    children: BTreeMap<&'a str, Vec<&'a str>>,
    dependencies: BTreeMap<&'a str, Vec<&'a str>>,
}
impl<'a> QueryIndex<'a> {
    pub fn new(manifest: &'a Manifest) -> Self {
        let mut index = Self {
            manifest,
            nodes: manifest.nodes.iter().map(|n| (n.id.as_str(), n)).collect(),
            claims: manifest.claims.iter().map(|c| (c.id.as_str(), c)).collect(),
            parents: BTreeMap::new(),
            children: BTreeMap::new(),
            dependencies: BTreeMap::new(),
        };
        for link in &manifest.links {
            match link.kind {
                LinkKind::Child => {
                    index.parents.insert(link.to.as_str(), link.from.as_str());
                    index
                        .children
                        .entry(link.from.as_str())
                        .or_default()
                        .push(link.to.as_str());
                }
                LinkKind::DependsOn => {
                    index
                        .dependencies
                        .entry(link.from.as_str())
                        .or_default()
                        .push(link.to.as_str());
                }
            }
        }
        index
    }
    pub fn node(&self, id: &str) -> Option<&'a Node> {
        self.nodes.get(id).copied()
    }
    pub fn claim(&self, id: &str) -> Option<&'a Claim> {
        self.claims.get(id).copied()
    }
    pub fn parent(&self, id: &str) -> Option<&'a str> {
        self.parents.get(id).copied()
    }
    pub fn children(&self, id: &str) -> &[&'a str] {
        self.children.get(id).map_or(&[], Vec::as_slice)
    }
    pub fn dependencies(&self, id: &str) -> &[&'a str] {
        self.dependencies.get(id).map_or(&[], Vec::as_slice)
    }
    /// Child descendants only; the anchor is excluded.
    pub fn descendants(&self, id: &str) -> Option<BTreeSet<&'a str>> {
        self.node(id)?;
        let mut found = BTreeSet::new();
        let mut pending: Vec<&str> = self.children(id).to_vec();
        while let Some(next) = pending.pop() {
            if found.insert(next) {
                pending.extend_from_slice(self.children(next));
            }
        }
        Some(found)
    }
    /// Root through target, including both endpoints.
    pub fn path(&self, id: &str) -> Option<Vec<&'a Node>> {
        let mut node = self.node(id)?;
        let mut result = vec![node];
        let mut seen = BTreeSet::new();
        while let Some(parent) = self.parent(node.id.as_str()) {
            if !seen.insert(parent) {
                return None;
            }
            node = self.node(parent)?;
            result.push(node);
        }
        result.reverse();
        Some(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    Certain,
    Possible,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TokenMatch<'a> {
    pub literal: &'a str,
    pub range: Range<usize>,
    pub certainty: Certainty,
}

/// Scan complete case-sensitive native IDs. Scoped refs, URLs, filenames,
/// alphanumeric suffixes, and fenced code are not local references.
pub fn scan_tokens(text: &str) -> Vec<TokenMatch<'_>> {
    let mut matches = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let marker = trimmed.as_bytes().first().copied();
        if let Some(marker @ (b'`' | b'~')) = marker {
            let length = trimmed.bytes().take_while(|&b| b == marker).count();
            if length >= 3 && indent <= 3 {
                if let Some((open, count)) = fence {
                    if marker == open && length >= count {
                        fence = None;
                    }
                } else {
                    fence = Some((marker, length));
                }
                offset += line.len();
                continue;
            }
        }
        if fence.is_some() {
            offset += line.len();
            continue;
        }
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if !matches!(bytes[i], b'N' | b'C' | b'H' | b'E' | b'O' | b'T')
                || (i > 0 && local_boundary_block(bytes[i - 1]))
            {
                i += 1;
                continue;
            }
            let start = i;
            i += 1;
            let digits = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let sentence_period = bytes.get(i) == Some(&b'.')
                && bytes.get(i + 1).is_none_or(|next| {
                    next.is_ascii_whitespace() || matches!(next, b'"' | b'\'' | b')' | b']' | b'}')
                });
            if i == digits
                || (i < bytes.len() && local_boundary_block(bytes[i]) && !sentence_period)
            {
                continue;
            }
            // Non-ASCII letters adjacent to an ASCII ID are part of a word.
            if line[..start]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
                || line[i..].chars().next().is_some_and(char::is_alphanumeric)
            {
                continue;
            }
            matches.push(TokenMatch {
                literal: &line[start..i],
                range: offset + start..offset + i,
                certainty: Certainty::Possible,
            });
        }
        offset += line.len();
    }
    matches
}
fn local_boundary_block(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(byte, b'_' | b'/' | b'\\' | b'.' | b':' | b'#' | b'~' | b'-')
}

/// Exact spelling is authoritative. A differently padded number is only a
/// possible short mention; it never resolves or rewrites automatically.
pub fn token_may_refer(literal: &str, target: &str) -> bool {
    if literal == target {
        return true;
    }
    let (Some(a), Some(b)) = (literal.as_bytes().first(), target.as_bytes().first()) else {
        return false;
    };
    a.is_ascii_alphabetic()
        && a == b
        && literal
            .get(1..)
            .and_then(|value| value.parse::<u64>().ok())
            .zip(target.get(1..).and_then(|value| value.parse::<u64>().ok()))
            .is_some_and(|(a, b)| a == b)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sentence_period_keeps_native_reference_but_filename_suffix_stays_opaque() {
        let text = "雪 N124. Next N124.md N124.5 path.N124 and C05.\n";
        let tokens = scan_tokens(text);
        assert_eq!(
            tokens.iter().map(|token| token.literal).collect::<Vec<_>>(),
            ["N124", "C05"]
        );
        for token in tokens {
            assert_eq!(&text[token.range], token.literal);
        }
    }

    #[test]
    fn child_queries_do_not_follow_dependencies() {
        let (manifest, _) = crate::parse_sources("tree:\n  - id: N01\n    type: question\n    children:\n      - id: N02\n        type: experiment\n  - id: N03\n    type: question\n    also_depends_on: [N02]\n", None).unwrap();
        let index = QueryIndex::new(&manifest);
        assert_eq!(index.descendants("N01").unwrap(), BTreeSet::from(["N02"]));
        assert!(index.descendants("N03").unwrap().is_empty());
        assert_eq!(
            index
                .path("N02")
                .unwrap()
                .iter()
                .map(|n| n.id.as_str())
                .collect::<Vec<_>>(),
            ["N01", "N02"]
        );
    }
    #[test]
    fn resumed_parent_is_a_child_relation_and_conflicting_or_missing_parents_fail() {
        let (manifest,report)=crate::parse_sources("tree:\n  - id: N01\n    type: question\n  - id: N02\n    type: experiment\n    parent: N01\n",None).unwrap();
        assert!(report.is_ok());
        let index = QueryIndex::new(&manifest);
        assert_eq!(index.parent("N02"), Some("N01"));
        assert_eq!(
            index
                .path("N02")
                .unwrap()
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            ["N01", "N02"]
        );
        for (source, expected) in [
            (
                "tree:\n  - id: N01\n    type: question\n    parent: N99\n",
                crate::RuleCode::UnknownParentNode,
            ),
            (
                "tree:\n  - id: N01\n    type: question\n    children:\n      - id: N02\n        type: question\n        parent: N03\n  - id: N03\n    type: question\n",
                crate::RuleCode::ConflictingParent,
            ),
            (
                "tree:\n  - id: N01\n    type: question\n    parent: N02\n    also_depends_on: [N03]\n  - id: N02\n    type: question\n    parent: N01\n  - id: N03\n    type: question\n",
                crate::RuleCode::DependencyCycle,
            ),
        ] {
            assert!(
                crate::parse_sources(source, None)
                    .unwrap_err()
                    .errors()
                    .iter()
                    .any(|diagnostic| diagnostic.code == expected)
            );
        }
    }
    #[test]
    fn token_ranges_preserve_spelling_and_utf8_boundaries() {
        let text = "雪 N1 N10 N01 bob:N1 N1.md URL/N1 N1suffix\n```yaml\nN01\n```\nE2 E02";
        let matches = scan_tokens(text);
        assert_eq!(
            matches.iter().map(|m| m.literal).collect::<Vec<_>>(),
            ["N1", "N10", "N01", "E2", "E02"]
        );
        for found in matches {
            assert_eq!(&text[found.range], found.literal);
        }
        assert!(token_may_refer("E2", "E02"));
        assert!(!token_may_refer("N1", "N10"));
    }
}

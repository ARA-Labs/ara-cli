//! Source-mapped lexical hits: the lines of a search result's native source
//! span that contain one of the result's matched query terms, under the same
//! tokenization search uses, with optional merged context.
use super::spans::Span;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Matching lines reported per result; `match_count` keeps the total.
pub const MAX_MATCHES: usize = 20;

/// Byte offsets where each line of one document starts.
pub struct LineIndex(Vec<usize>);
impl LineIndex {
    pub fn new(text: &str) -> Self {
        Self(
            std::iter::once(0)
                .chain(text.match_indices('\n').map(|(offset, _)| offset + 1))
                .filter(|start| *start < text.len() || *start == 0)
                .collect(),
        )
    }
    /// Zero-based line holding `offset`.
    pub fn line(&self, offset: usize) -> usize {
        self.0.partition_point(|start| *start <= offset) - 1
    }
    /// A line's text without its line ending.
    fn text<'a>(&self, text: &'a str, line: usize) -> &'a str {
        let end = self.0.get(line + 1).copied().unwrap_or(text.len());
        text[self.0[line]..end].trim_end_matches(['\n', '\r'])
    }
    /// Whether a span holds a line: its last non-whitespace byte (its start,
    /// when blank) lies in one of the span's ranges. A parent YAML line that
    /// only introduces an excluded child (`- id: N02`) is not held.
    fn held(&self, span: &Span<'_>, line: usize) -> bool {
        let start = self.0[line];
        let content = self.text(span.text, line).trim_end();
        let probe = start + content.len().saturating_sub(1);
        span.ranges.iter().any(|range| range.contains(&probe))
    }
}

/// The additive `line`, `matches`, `match_count` and (when requested)
/// `context` fields of one search result. A result with no literal hit gets
/// `match_count: 0` and no `line`; its excerpt stays the only display.
pub fn fields(
    span: &Span<'_>,
    lines: &LineIndex,
    terms: &[String],
    context: Option<usize>,
) -> serde_json::Map<String, Value> {
    let terms: BTreeSet<&str> = terms.iter().map(String::as_str).collect();
    let mut matched = BTreeSet::new();
    for range in &span.ranges {
        for (term, start, _) in crate::search::tokens(&span.text[range.clone()]) {
            if terms.contains(term.as_str()) {
                matched.insert(lines.line(range.start + start));
            }
        }
    }
    let mut fields = serde_json::Map::new();
    fields.insert("match_count".into(), json!(matched.len()));
    let shown: Vec<usize> = matched.into_iter().take(MAX_MATCHES).collect();
    let Some(first) = shown.first() else {
        return fields;
    };
    fields.insert("line".into(), json!(first + 1));
    fields.insert(
        "matches".into(),
        json!(
            shown
                .iter()
                .map(|line| json!({"line": line + 1, "text": lines.text(span.text, *line)}))
                .collect::<Vec<_>>()
        ),
    );
    if let Some(extra) = context {
        let bounds = span_lines(span, lines);
        let held = |line: usize| shown.contains(&line) || lines.held(span, line);
        let blocks = clipped(merged(&shown, extra, bounds), held)
            .into_iter()
            .map(|(start, end)| {
                json!({
                    "start": start + 1,
                    "end": end + 1,
                    "lines": (start..=end).map(|line| lines.text(span.text, line)).collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>();
        fields.insert("context".into(), json!(blocks));
    }
    fields
}

/// First and last zero-based lines of a span; context never leaves it.
fn span_lines(span: &Span<'_>, lines: &LineIndex) -> (usize, usize) {
    let start = span.ranges.first().map_or(0, |range| range.start);
    let end = span.ranges.last().map_or(0, |range| range.end);
    (
        lines.line(start),
        lines.line(end.saturating_sub(1).max(start)),
    )
}

/// Inclusive line ranges around sorted `lines`, merged where they overlap or
/// touch, clipped to `bounds`.
fn merged(lines: &[usize], extra: usize, bounds: (usize, usize)) -> Vec<(usize, usize)> {
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    for &line in lines {
        let start = line.saturating_sub(extra).max(bounds.0);
        let end = line.saturating_add(extra).min(bounds.1).max(line);
        match blocks.last_mut() {
            Some(last) if start <= last.1 + 1 => last.1 = last.1.max(end),
            _ => blocks.push((start, end)),
        }
    }
    blocks
}

/// Split blocks around lines `keep` rejects, dropping those lines.
fn clipped(blocks: Vec<(usize, usize)>, keep: impl Fn(usize) -> bool) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    for (start, end) in blocks {
        let mut open = None;
        for line in start..=end {
            match (keep(line), open) {
                (true, None) => open = Some(line),
                (false, Some(first)) => {
                    result.push((first, line - 1));
                    open = None;
                }
                _ => {}
            }
        }
        if let Some(first) = open {
            result.push((first, end));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    fn span(text: &str) -> Span<'_> {
        Span {
            path: Cow::Borrowed("d.md"),
            text,
            ranges: super::super::spans::whole(text.len()),
        }
    }

    #[test]
    fn lines_are_one_based_and_case_insensitive() {
        let text = "alpha\r\nBeta gamma\nnone\nbeta\n";
        let fields = fields(&span(text), &LineIndex::new(text), &["beta".into()], None);
        assert_eq!(fields["line"], 2);
        assert_eq!(
            fields["matches"],
            json!([{"line":2,"text":"Beta gamma"},{"line":4,"text":"beta"}])
        );
        assert!(!fields.contains_key("context"));
    }

    #[test]
    fn context_merges_overlaps_and_zero_adds_nothing() {
        assert_eq!(merged(&[2, 4, 9], 1, (0, 20)), [(1, 5), (8, 10)]);
        assert_eq!(merged(&[2, 4], 0, (0, 20)), [(2, 2), (4, 4)]);
        assert_eq!(merged(&[2, 3], 0, (0, 20)), [(2, 3)]);
        assert_eq!(merged(&[0, 9], 3, (0, 10)), [(0, 3), (6, 10)]);
    }

    #[test]
    fn context_skips_lines_of_excluded_nested_entries() {
        let text = "tree:\n  - id: N01\n    title: alpha\n    children:\n      - id: N02\n        title: beta\n    after: alpha\n";
        let child = text.find("id: N02").unwrap();
        let child_end = text.find("beta").unwrap() + 4;
        let start = text.find("id: N01").unwrap();
        let span = Span {
            path: Cow::Borrowed("t.yaml"),
            text,
            ranges: vec![start..child, child_end..text.len() - 1],
        };
        let fields = fields(&span, &LineIndex::new(text), &["alpha".into()], Some(5));
        let shown: Vec<u64> = fields["context"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|block| block["start"].as_u64().unwrap()..=block["end"].as_u64().unwrap())
            .collect();
        assert_eq!(shown, [2, 3, 4, 7]);
        assert_eq!(fields["context"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn no_literal_hit_reports_no_line() {
        let text = "nothing here\n";
        let fields = fields(
            &span(text),
            &LineIndex::new(text),
            &["absent".into()],
            Some(2),
        );
        assert_eq!(fields["match_count"], 0);
        assert!(!fields.contains_key("line") && !fields.contains_key("matches"));
    }
}

//! Byte-level token helpers for the citation inventory: native-ID token
//! boundaries, delimiter-terminated spellings, list items, word-bounded
//! occurrences and decoded-to-source offsets.
use crate::markdown::MarkdownField;
use std::ops::Range;

/// One-based line numbers for byte offsets of one source text.
pub(super) struct LineIndex(Vec<usize>);
impl LineIndex {
    pub(super) fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Self(starts)
    }
    pub(super) fn line(&self, offset: usize) -> usize {
        match self.0.binary_search(&offset) {
            Ok(index) => index + 1,
            Err(index) => index,
        }
    }
}

pub(super) fn contains(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

pub(super) fn path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'.')
}

/// Whether `rest` starts with `spelled` and the spelling ends there: at the
/// end, a list/quote/bracket delimiter, a sentence period, or (for a
/// spelling without whitespace) whitespace. A multi-word spelling followed
/// by more words is ambiguous prose, not a parsed reference.
pub(super) fn terminated(rest: &str, spelled: &str) -> bool {
    if spelled.is_empty() || !rest.starts_with(spelled) {
        return false;
    }
    let after = &rest[spelled.len()..];
    let mut chars = after.chars();
    match chars.next() {
        None => true,
        Some(',' | ';' | ']' | ')' | '}' | '`' | '"' | '\'' | '|' | '\n' | '\r') => true,
        Some('.') => chars.next().is_none_or(char::is_whitespace),
        Some(c) if c.is_whitespace() => !spelled.contains(char::is_whitespace),
        _ => false,
    }
}

/// Native-ID-shaped tokens (`C01`, `RW2`) with the read model's boundaries:
/// scoped spellings, filenames and alphanumeric neighbors are not tokens.
pub(super) fn id_tokens(value: &str) -> Vec<Range<usize>> {
    let bytes = value.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        if !bytes[i].is_ascii_uppercase()
            || i > 0 && crate::query::local_boundary_block(bytes[i - 1])
            || value[..i]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
        {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        if end < bytes.len() && bytes[end].is_ascii_uppercase() {
            end += 1;
        }
        let digits = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        i = end.max(start + 1);
        if end == digits {
            continue;
        }
        let sentence = bytes.get(end) == Some(&b'.')
            && bytes
                .get(end + 1)
                .is_none_or(|next| next.is_ascii_whitespace() || b"\"')]}".contains(next));
        if end < bytes.len() && crate::query::local_boundary_block(bytes[end]) && !sentence
            || value[end..]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        result.push(start..end);
    }
    result
}

/// The trimmed item of one comma-separated piece starting at `offset`.
pub(super) fn item_range(piece: &str, offset: usize) -> Option<Range<usize>> {
    let mut start = 0;
    let mut end = piece.len();
    let trim = |start: &mut usize, end: &mut usize| {
        while *start < *end && piece[*start..].starts_with(char::is_whitespace) {
            *start += piece[*start..].chars().next().map_or(1, char::len_utf8);
        }
        while *end > *start && piece[..*end].ends_with(char::is_whitespace) {
            *end -= piece[..*end].chars().next_back().map_or(1, char::len_utf8);
        }
    };
    trim(&mut start, &mut end);
    if piece[start..end].starts_with('[') {
        start += 1;
    }
    if piece[start..end].ends_with(']') {
        end -= 1;
    }
    trim(&mut start, &mut end);
    for quote in ['"', '\'', '`'] {
        if end >= start + 2
            && piece[start..end].starts_with(quote)
            && piece[start..end].ends_with(quote)
        {
            start += 1;
            end -= 1;
            break;
        }
    }
    (start < end).then(|| offset + start..offset + end)
}

/// Word-bounded occurrences of any name (the dangling-reference guard's rule).
pub(super) fn occurrences<'n>(text: &str, names: &'n [String]) -> Vec<(Range<usize>, &'n str)> {
    let mut result = Vec::new();
    for name in names {
        if name.is_empty() {
            continue;
        }
        for (i, _) in text.match_indices(name.as_str()) {
            let left = text[..i].chars().next_back();
            let right = text[i + name.len()..].chars().next();
            if !left.is_some_and(|c| c.is_alphanumeric() || c == '_')
                && !right.is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                result.push((i..i + name.len(), name.as_str()));
            }
        }
    }
    result.sort_by_key(|(range, _)| range.start);
    result
}

/// Source byte offset of a decoded field offset; see
/// [`crate::markdown::decoded_to_source`].
pub(super) fn source_offset(field: &MarkdownField<'_>, decoded: usize) -> usize {
    crate::markdown::decoded_to_source(field, decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literals(value: &str) -> Vec<&str> {
        id_tokens(value).into_iter().map(|r| &value[r]).collect()
    }

    #[test]
    fn id_tokens_use_read_model_boundaries() {
        assert_eq!(
            literals("[C01, C02] \"C03\" (RW2) C04. E01-x C05v2 logic/claims.md:C06 雪C07"),
            ["C01", "C02", "C03", "RW2", "C04"]
        );
    }

    #[test]
    fn multiword_spellings_end_only_at_delimiters() {
        assert!(terminated("Old term, next", "Old term"));
        assert!(terminated("Old term", "Old term"));
        assert!(!terminated("Old term for details", "Old term"));
        assert!(terminated("C01 and more", "C01"));
        assert!(terminated("C01. Next", "C01"));
        assert!(!terminated("C01: Title", "C01"));
        assert!(!terminated("Term", ""));
    }

    #[test]
    fn related_items_strip_list_and_quote_delimiters() {
        let value = "[\"A/B #1\", `Term` , Group A/Term]";
        let items: Vec<&str> = value
            .split(',')
            .scan(0, |offset, piece| {
                let range = item_range(piece, *offset);
                *offset += piece.len() + 1;
                Some(range)
            })
            .flatten()
            .map(|range| &value[range])
            .collect();
        assert_eq!(items, ["A/B #1", "Term", "Group A/Term"]);
    }
}

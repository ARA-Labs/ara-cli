//! Typed entry headings shared by the readers, merge identity and CLI lookup.
//!
//! A level-two heading names an entry with a canonical `{prefix}##` ID, a
//! separator and a nonempty title. The separator is `:` or a spaced ASCII
//! hyphen, en dash or em dash (`## E01 — Title`). Typed entries other than
//! claims may also put a bracketed status between the ID and the separator
//! (`## E02 [in progress]: Title`). Every returned part is a trimmed slice of
//! the heading; the source spelling is never rewritten.

use crate::manifest::is_canonical_id;

/// Separators accepted between an entry ID and its title, besides `:`. Each
/// needs whitespace on both sides, so a hyphen inside an ID or a hyphenated
/// word never splits a heading.
const DASH_SEPARATORS: [char; 3] = ['-', '\u{2013}', '\u{2014}'];

/// Prefixes of native entry IDs whose headings may carry a bracketed status.
const BRACKET_PREFIXES: [char; 5] = ['N', 'H', 'E', 'O', 'T'];

/// The parts of a typed entry heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryHeading<'a> {
    pub id: &'a str,
    pub title: &'a str,
    /// Bracketed status text between the ID and the separator, if any.
    pub status: Option<&'a str>,
}

/// The `(id, title)` of `ID: Title` or `ID <dash> Title` for `prefix`.
pub fn entry_heading(heading: &str, prefix: char) -> Option<(&str, &str)> {
    if let Some((id, title)) = heading.split_once(':')
        && is_canonical_id(id.trim(), prefix)
    {
        let title = title.trim();
        return (!title.is_empty()).then(|| (id.trim(), title));
    }
    let heading = heading.trim_start();
    let id_end = heading.find(char::is_whitespace)?;
    let (id, rest) = heading.split_at(id_end);
    let title = dash_title(rest)?;
    is_canonical_id(id, prefix).then_some((id, title))
}

/// [`entry_heading`], or `ID [status] <separator> Title`.
pub fn typed_entry_heading(heading: &str, prefix: char) -> Option<EntryHeading<'_>> {
    if let Some((id, title)) = entry_heading(heading, prefix) {
        return Some(EntryHeading {
            id,
            title,
            status: None,
        });
    }
    let heading = heading.trim_start();
    let open = heading.find('[')?;
    let id = heading[..open].trim_end();
    if !is_canonical_id(id, prefix) {
        return None;
    }
    let (status, rest) = heading[open + 1..].split_once(']')?;
    let status = status.trim();
    if status.is_empty() {
        return None;
    }
    let title = match rest.trim_start().strip_prefix(':') {
        Some(title) => title.trim(),
        None => dash_title(rest)?,
    };
    (!title.is_empty()).then_some(EntryHeading {
        id,
        title,
        status: Some(status),
    })
}

/// A heading for whichever native prefix its ID starts with. Claims (`C`)
/// accept no bracketed status.
pub fn native_entry_heading(heading: &str) -> Option<EntryHeading<'_>> {
    let prefix = heading.trim_start().chars().next()?;
    if prefix == 'C' {
        return entry_heading(heading, 'C').map(|(id, title)| EntryHeading {
            id,
            title,
            status: None,
        });
    }
    BRACKET_PREFIXES
        .contains(&prefix)
        .then(|| typed_entry_heading(heading, prefix))
        .flatten()
}

/// The title after a spaced dash: `rest` is whitespace, a dash separator,
/// whitespace, then a nonempty title.
fn dash_title(rest: &str) -> Option<&str> {
    let rest = rest.strip_prefix(char::is_whitespace)?.trim_start();
    let separator = rest.chars().next()?;
    if !DASH_SEPARATORS.contains(&separator) {
        return None;
    }
    let title = rest[separator.len_utf8()..]
        .strip_prefix(char::is_whitespace)?
        .trim();
    (!title.is_empty()).then_some(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_separator_parses_for_any_prefix() {
        for (heading, prefix, id, title) in [
            ("E01: Colon", 'E', "E01", "Colon"),
            ("E01 : Spaced colon", 'E', "E01", "Spaced colon"),
            ("E01 - Hyphen", 'E', "E01", "Hyphen"),
            ("H03 \u{2013} En dash", 'H', "H03", "En dash"),
            (
                "E01 \u{2014} Baseline vs. seed (upper bound)",
                'E',
                "E01",
                "Baseline vs. seed (upper bound)",
            ),
            ("E01: Two-stage - warmup", 'E', "E01", "Two-stage - warmup"),
            ("C01 \u{2014} Claim", 'C', "C01", "Claim"),
        ] {
            assert_eq!(
                entry_heading(heading, prefix),
                Some((id, title)),
                "{heading:?}"
            );
        }
    }

    #[test]
    fn unspaced_dashes_wrong_prefixes_and_empty_titles_do_not_parse() {
        for heading in [
            "E01-Title",
            "E01 -Title",
            "E01- Title",
            "E01\u{2014}Title",
            "E01 - ",
            "E01:",
            "E01 -- Title",
            "e01 - Title",
            "H01 - Title",
            "E01-ablation: Title",
            "Experiments overview",
        ] {
            assert_eq!(entry_heading(heading, 'E'), None, "{heading:?}");
            assert_eq!(typed_entry_heading(heading, 'E'), None, "{heading:?}");
        }
    }

    #[test]
    fn bracketed_status_precedes_the_separator() {
        for (heading, title, status) in [
            (
                "E02 [in progress]: Reception analysis",
                "Reception analysis",
                "in progress",
            ),
            ("E02 [planned] \u{2014} Dash", "Dash", "planned"),
            ("E02[done]: Unspaced", "Unspaced", "done"),
        ] {
            assert_eq!(
                typed_entry_heading(heading, 'E'),
                Some(EntryHeading {
                    id: "E02",
                    title,
                    status: Some(status)
                }),
                "{heading:?}"
            );
        }
        for heading in [
            "E02 []: Empty",
            "E02 [x]",
            "E02 [x]:",
            "E02 [x] Title",
            "E02 [x: Title",
        ] {
            assert_eq!(typed_entry_heading(heading, 'E'), None, "{heading:?}");
        }
        // A bracket in the title is title text, not a status.
        assert_eq!(
            typed_entry_heading("E01: Title [draft]", 'E'),
            Some(EntryHeading {
                id: "E01",
                title: "Title [draft]",
                status: None
            })
        );
    }

    #[test]
    fn native_headings_pick_their_prefix_and_claims_take_no_status() {
        assert_eq!(
            native_entry_heading("H03 \u{2013} Track").map(|h| h.id),
            Some("H03")
        );
        assert_eq!(
            native_entry_heading("E02 [x]: T").map(|h| h.id),
            Some("E02")
        );
        assert_eq!(
            native_entry_heading("C01 - Claim").map(|h| h.id),
            Some("C01")
        );
        assert_eq!(native_entry_heading("C01 [x]: Claim"), None);
        assert_eq!(native_entry_heading("RW01: Related"), None);
        assert_eq!(native_entry_heading("Overview"), None);
    }
}

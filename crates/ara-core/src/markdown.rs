//! Fence-aware byte ranges over Markdown. This indexes source; it never rewrites it.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownSection<'a> {
    pub heading: &'a str,
    pub range: Range<usize>,
    pub body_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownField<'a> {
    pub name: &'a str,
    pub range: Range<usize>,
    /// Exact value bytes, including continuation indentation and trailing newlines.
    pub raw_value: &'a str,
    /// Semantic value, with outer whitespace trimmed.
    pub value: &'a str,
    pub value_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownHeading<'a> {
    pub heading: &'a str,
    pub level: usize,
    pub range: Range<usize>,
    pub body_range: Range<usize>,
    /// Ancestor headings followed by this heading.
    pub path: Vec<&'a str>,
}

/// Initial YAML frontmatter source span, including fences and their newline.
/// Leading blank lines/BOM are tolerated. An unterminated opening fence extends
/// to EOF so metadata comments cannot become selectable Markdown sections.
pub fn frontmatter_range(md: &str) -> Option<Range<usize>> {
    let mut opening = None;
    let mut offset = 0;
    for line in md.split_inclusive('\n') {
        let text = line.trim_end_matches(['\r', '\n']);
        let text = if offset == 0 {
            text.strip_prefix('\u{feff}').unwrap_or(text)
        } else {
            text
        };
        if let Some(start) = opening {
            if text.trim() == "---" {
                return Some(start..offset + line.len());
            }
        } else if !text.trim().is_empty() {
            if text.trim() != "---" {
                return None;
            }
            opening = Some(offset);
        }
        offset += line.len();
    }
    opening.map(|start| start..md.len())
}

/// ATX headings (up to three leading spaces), with hierarchical source ranges.
/// Fenced code and canonical field continuations cannot create headings.
pub fn headings(md: &str) -> Vec<MarkdownHeading<'_>> {
    index_headings(md, true)
}

fn index_headings(md: &str, include_paths: bool) -> Vec<MarkdownHeading<'_>> {
    let mut result: Vec<MarkdownHeading<'_>> = Vec::new();
    let mut ancestors: Vec<(usize, &str)> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut fence = None;
    let mut continuation = false;
    let mut offset = 0;
    let body_start = frontmatter_range(md).map_or(0, |range| range.end);
    for line in md.split_inclusive('\n') {
        if offset < body_start {
            offset += line.len();
            continue;
        }
        let text = line.trim_end_matches(['\r', '\n']);
        if continuation && text.starts_with("  ") {
            offset += line.len();
            continue;
        }
        continuation = false;
        if outside_fence(text, &mut fence) {
            continuation = field_label(text).is_some_and(|(_, start)| start == text.len());
            let indentation = text.bytes().take_while(|b| *b == b' ').count();
            let heading_line = if indentation <= 3 {
                &text[indentation..]
            } else {
                ""
            };
            let level = heading_line.bytes().take_while(|b| *b == b'#').count();
            if (1..=6).contains(&level) && heading_line.as_bytes().get(level) == Some(&b' ') {
                while open.last().is_some_and(|i| result[*i].level >= level) {
                    let index = open.pop().unwrap();
                    result[index].range.end = offset;
                    result[index].body_range.end = offset;
                }
                if include_paths {
                    while ancestors.last().is_some_and(|(n, _)| *n >= level) {
                        ancestors.pop();
                    }
                }
                let heading = heading_line[level + 1..].trim();
                if include_paths {
                    ancestors.push((level, heading));
                }
                open.push(result.len());
                result.push(MarkdownHeading {
                    heading,
                    level,
                    range: offset..md.len(),
                    body_range: offset + line.len()..md.len(),
                    path: if include_paths {
                        ancestors.iter().map(|(_, h)| *h).collect()
                    } else {
                        Vec::new()
                    },
                });
            }
        }
        offset += line.len();
    }
    result
}

/// Level-two sections in source order; ranges include deeper subsections.
pub fn sections(md: &str) -> Vec<MarkdownSection<'_>> {
    index_headings(md, false)
        .into_iter()
        .filter(|h| h.level == 2)
        .map(|h| MarkdownSection {
            heading: h.heading,
            range: h.range,
            body_range: h.body_range,
        })
        .collect()
}

/// Bold source fields (bulleted or bare), including continuation lines and
/// nested lists. Only unindented field labels start a new field; fenced labels
/// never do. Value slices are trimmed only at their outer whitespace boundary.
pub fn fields(md: &str, range: Range<usize>) -> Vec<MarkdownField<'_>> {
    let mut result: Vec<MarkdownField<'_>> = Vec::new();
    let mut fence = None;
    let mut continuation = false;
    let mut html_comment = false;
    let mut offset = range.start;
    for line in md[range.clone()].split_inclusive('\n') {
        let text = line.trim_end_matches(['\r', '\n']);
        if html_comment {
            html_comment = !text.contains("-->");
            offset += line.len();
            continue;
        }
        if continuation && text.starts_with("  ") {
            offset += line.len();
            continue;
        }
        continuation = false;
        if outside_fence(text, &mut fence) {
            if text.starts_with("<!--") {
                if let Some(last) = result.last_mut().filter(|last| last.range.end == range.end) {
                    finish_field(md, last, offset);
                }
                html_comment = !text.contains("-->");
                offset += line.len();
                continue;
            }
            if let Some((name, value_start)) = field_label(text) {
                continuation = value_start == text.len();
                if let Some(last) = result.last_mut().filter(|last| last.range.end == range.end) {
                    finish_field(md, last, offset);
                }
                result.push(MarkdownField {
                    name,
                    value: "",
                    raw_value: "",
                    range: offset..range.end,
                    value_range: offset + value_start..range.end,
                });
            }
        }
        offset += line.len();
    }
    if let Some(last) = result.last_mut().filter(|last| last.range.end == range.end) {
        finish_field(md, last, range.end);
    }
    result
}

fn finish_field<'a>(md: &'a str, field: &mut MarkdownField<'a>, end: usize) {
    let initial = &md[field.value_range.start..end];
    let end = if let Some(rest) = initial
        .strip_prefix('\n')
        .filter(|rest| rest.starts_with("  "))
    {
        field.value_range.start
            + 1
            + rest
                .split_inclusive('\n')
                .take_while(|line| line.starts_with("  "))
                .map(str::len)
                .sum::<usize>()
    } else {
        end
    };
    field.range.end = end;
    let raw = &md[field.value_range.start..end];
    field.raw_value = raw;
    let trimmed = raw.trim();
    let start = field.value_range.start + raw.len() - raw.trim_start().len();
    field.value_range = start..start + trimmed.len();
    field.value = &md[field.value_range.clone()];
}

/// Decode the writer's two-space continuation representation without trimming
/// caller whitespace. Ordinary published inline fields retain their familiar
/// outer-trimmed semantics. Raw bytes and spans remain available independently.
pub fn decode_field<'a>(field: &MarkdownField<'a>) -> std::borrow::Cow<'a, str> {
    let raw = field.raw_value;
    if let Some(rest) = raw.strip_prefix('\n') {
        let rest = rest.trim_end_matches('\n');
        // Keep explicitly indented empty lines: they encode caller newlines.
        if !rest.is_empty() && rest.split('\n').all(|line| line.starts_with("  ")) {
            let mut decoded = String::with_capacity(rest.len());
            for (index, line) in rest.split('\n').enumerate() {
                if index > 0 {
                    decoded.push('\n');
                }
                decoded.push_str(&line[2..]);
            }
            return std::borrow::Cow::Owned(decoded);
        }
    }
    std::borrow::Cow::Borrowed(field.value)
}

fn field_label(line: &str) -> Option<(&str, usize)> {
    // Nested bullets belong to the preceding field, not a sibling field.
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .unwrap_or(line);
    let rest = rest.strip_prefix("**")?;
    let (name, after) = rest.split_once("**")?;
    let after = after.trim_start();
    let name = name.trim();
    let value = match after.strip_prefix(':') {
        Some(value) => value.trim_start(),
        None if name.ends_with('.') => after,
        None => return None,
    };
    Some((
        name.strip_suffix('.').unwrap_or(name).trim_end(),
        line.len() - value.len(),
    ))
}

fn outside_fence(line: &str, fence: &mut Option<(u8, usize)>) -> bool {
    let indentation = line.bytes().take_while(|b| *b == b' ').count();
    if indentation > 3 || line.starts_with('\t') {
        return fence.is_none();
    }
    let t = &line[indentation..];
    let marker = t.as_bytes().first().copied();
    if matches!(marker, Some(b'`' | b'~')) {
        let marker = marker.unwrap();
        let count = t.bytes().take_while(|b| *b == marker).count();
        if count >= 3 {
            match *fence {
                None => *fence = Some((marker, count)),
                Some((open, length))
                    if marker == open && count >= length && t[count..].trim().is_empty() =>
                {
                    *fence = None
                }
                _ => {}
            }
            return false;
        }
    }
    fence.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_fields_have_exact_independent_byte_ranges() {
        let md = "# Préface\n\n## C01: café\n- **Statement**: α first\n  continuation β\n- **Proof**: E01 γ\n";
        let section = sections(md).remove(0);
        let indexed = fields(md, section.body_range);
        assert_eq!(
            &md[indexed[0].range.clone()],
            "- **Statement**: α first\n  continuation β\n"
        );
        assert_eq!(&md[indexed[1].range.clone()], "- **Proof**: E01 γ\n");
        assert_eq!(
            &md[indexed[0].value_range.clone()],
            "α first\n  continuation β"
        );
        assert_eq!(indexed[1].value, "E01 γ");
    }

    #[test]
    fn fences_do_not_create_claims_or_fields() {
        let md = "   ~~~~md\n## C99: fake\n   ~~~~\n## C01: real\n- **Proof**: prose\n```md\n- **Status**: fake\n## C98: fake\n```\n- **Status**: supported\n";
        let indexed = sections(md);
        assert_eq!(
            indexed.iter().map(|s| s.heading).collect::<Vec<_>>(),
            ["C01: real"]
        );
        let parsed = fields(md, indexed[0].body_range.clone());
        assert_eq!(
            parsed.iter().map(|f| f.name).collect::<Vec<_>>(),
            ["Proof", "Status"]
        );
        assert!(parsed[0].value.contains("## C98: fake"));
    }

    #[test]
    fn canonical_multiline_payload_retains_exact_whitespace_and_prose() {
        let content =
            "  leading\r\n## C99: not a heading\n```md\n- **Status**: nested\n```\ntrailing  \n";
        let mut md = String::from("## C01: real\n- **Statement**:\n");
        for line in content.split('\n') {
            md.push_str("  ");
            md.push_str(line);
            md.push('\n');
        }
        let body_start = md.find("- **Statement**").unwrap();
        let field_end = md.len();
        md.push_str("\nUnrelated body prose.\n\n- **Proof**: E01\n");
        let section = sections(&md).remove(0);
        let parsed = fields(&md, section.body_range);
        assert_eq!(decode_field(&parsed[0]), content);
        assert_eq!(parsed[0].range, body_start..field_end);
        assert_eq!(parsed[1].value, "E01");
    }

    #[test]
    fn nested_heading_ranges_follow_ancestry() {
        let md = "# Doc\n## Parent\ntext\n### Child\nchild text\n## Other\n";
        let parsed = headings(md);
        assert_eq!(parsed[2].path, ["Doc", "Parent", "Child"]);
        assert_eq!(
            &md[parsed[1].range.clone()],
            "## Parent\ntext\n### Child\nchild text\n"
        );
        assert_eq!(&md[parsed[2].body_range.clone()], "child text\n");
    }
}

#[cfg(test)]
mod annotation_tests {
    use super::*;

    #[test]
    fn native_conflict_comments_do_not_change_last_field_values_or_spans() {
        let md = "## C01: Claim\n- **Proof**: E01 with prose\n<!-- CONFLICT: see C02 -->\n<!-- payload\n- **Status**: not a field\n-->\nUnrelated prose.\n- **Status**: supported\n";
        let parsed = fields(md, sections(md)[0].body_range.clone());
        assert_eq!(
            parsed.iter().map(|field| field.name).collect::<Vec<_>>(),
            ["Proof", "Status"]
        );
        assert_eq!(parsed[0].value, "E01 with prose");
        assert_eq!(
            &md[parsed[0].range.clone()],
            "- **Proof**: E01 with prose\n"
        );
        assert_eq!(parsed[1].value, "supported");
    }

    #[test]
    fn indented_caller_comments_remain_in_canonical_payloads() {
        let md = "## C01: Claim\n- **Proof**:\n  <!-- caller comment -->\n  E01\n<!-- CONFLICT: see C02 -->\n";
        let parsed = fields(md, sections(md)[0].body_range.clone());
        assert_eq!(decode_field(&parsed[0]), "<!-- caller comment -->\nE01");
    }
}

#[cfg(test)]
mod frontmatter_tests {
    use super::*;

    #[test]
    fn frontmatter_comments_never_become_markdown_heading_selectors() {
        let md = "\u{feff}\r\n---\r\ntitle: Paper\r\n## metadata comment\r\n# another comment\r\n---\r\n# Paper\r\n## Actual body\r\nText.\r\n";
        let span = frontmatter_range(md).unwrap();
        assert_eq!(
            &md[span],
            "---\r\ntitle: Paper\r\n## metadata comment\r\n# another comment\r\n---\r\n"
        );
        let indexed = headings(md);
        assert_eq!(
            indexed.iter().map(|h| h.heading).collect::<Vec<_>>(),
            ["Paper", "Actual body"]
        );
        assert_eq!(&md[indexed[1].body_range.clone()], "Text.\r\n");
        assert!(headings("---\ntitle: Broken\n## metadata only\n").is_empty());
    }
}

//! Relocation is confined to the incoming spans explicitly selected by a layer.
use super::types::*;
use crate::query::scan_tokens;

fn display_target(token: &str, target: &str) -> String {
    let display = super::markdown::display_address(target);
    if let Some((document, _)) = token.split_once(':')
        && !document.contains('#')
        && (document.ends_with(".md")
            || document.starts_with("trace/")
            || document.starts_with("staging/"))
        && let Some((path, heading)) = display.split_once('#')
    {
        return format!("{path}:{heading}");
    }
    display
}

pub(crate) fn incoming(
    text: &str,
    path: &str,
    selector: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
    structured: bool,
) -> Result<String, MergeError> {
    incoming_mode::<false>(text, path, selector, map, report, structured)
}
fn incoming_mode<const HISTORICAL: bool>(
    text: &str,
    path: &str,
    selector: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
    structured: bool,
) -> Result<String, MergeError> {
    let mut patches = Vec::new();
    let protected = if structured {
        Vec::new()
    } else {
        quoted_ranges(text)
    };
    for token in scan_tokens(text) {
        if protected_span(&protected, &token.range) {
            continue;
        }
        if let Some(stable) = map.get(token.literal) {
            let target = if HISTORICAL {
                stable.as_str()
            } else {
                map.local
                    .get(stable)
                    .map_or(stable.as_str(), String::as_str)
            };
            if target == token.literal {
                continue;
            }
            let fact = RewriteFact {
                path: path.into(),
                selector: selector.into(),
                old: token.literal.into(),
                new: target.into(),
                confidence: "certain".into(),
            };
            report.rewritten.push(fact.clone());
            if !structured {
                report.needs_review.push(fact);
            }
            patches.push((token.range, target.to_owned()));
        } else if structured {
            return Err(MergeError::content(
                "merge.unresolved_reference",
                format!(
                    "incoming structured reference `{}` at {path}:{selector} has no destination identity",
                    token.literal
                ),
            ));
        } else {
            report.needs_review.push(RewriteFact {
                path: path.into(),
                selector: selector.into(),
                old: token.literal.into(),
                new: token.literal.into(),
                confidence: "ambiguous".into(),
            });
        }
    }
    // Native session references and document paths are distinct namespaces;
    // never reinterpret a problem observation O1 as staging O01.
    let mut offset = 0;
    for part in text.split_inclusive(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | '<' | '>' | '='
            )
    }) {
        let start = part
            .char_indices()
            .find(|(_, c)| {
                !c.is_whitespace()
                    && !matches!(
                        c,
                        '"' | '\''
                            | '`'
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | ','
                            | '<'
                            | '>'
                            | '='
                    )
            })
            .map(|(i, _)| i);
        if let Some(start) = start {
            let token = part[start..].trim_end_matches(|c: char| {
                c.is_whitespace()
                    || matches!(
                        c,
                        '"' | '\''
                            | '`'
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | ','
                            | '<'
                            | '>'
                            | '='
                    )
            });
            // A complete structured native token is authoritative. Descendants
            // are already mapped from the actual heading inventory; do not infer
            // their components by splitting a display path.
            let normalized = super::identity::normalize_local(token);
            let exact = (token.contains([':', '#'])
                || super::identity::numeric_prefix(token).is_none())
            .then(|| {
                map.tokens.get(token).or_else(|| {
                    super::identity::numeric_prefix(&normalized)
                        .is_none()
                        .then(|| map.tokens.get(&normalized))
                        .flatten()
                })
            })
            .flatten();
            if !token.contains("://") {
                if let Some(choice) = exact {
                    if let Some(stable) = choice {
                        let target = if HISTORICAL {
                            stable.as_str()
                        } else {
                            map.local
                                .get(stable)
                                .map_or(stable.as_str(), String::as_str)
                        };
                        let target = display_target(token, target);
                        let range = offset + start..offset + start + token.len();
                        if target != token && !protected_span(&protected, &range) {
                            patches.push((range, target.clone()));
                            let fact = RewriteFact {
                                path: path.into(),
                                selector: selector.into(),
                                old: token.into(),
                                new: target,
                                confidence: "certain".into(),
                            };
                            report.rewritten.push(fact.clone());
                            if !structured {
                                report.needs_review.push(fact);
                            }
                        }
                    } else if structured {
                        return Err(MergeError::content(
                            "merge.ambiguous_reference",
                            format!(
                                "native display reference `{token}` matches different literal heading vectors"
                            ),
                        ));
                    }
                    offset += part.len();
                    continue;
                }
                if let Some(stable) = map.get(token).or_else(|| {
                    super::identity::numeric_prefix(&normalized)
                        .is_none()
                        .then(|| map.get(&normalized))
                        .flatten()
                }) {
                    // Numeric bare tokens were handled by scan_tokens above.
                    if token.contains([':', '#'])
                        || super::identity::numeric_prefix(token).is_none()
                    {
                        let target = if HISTORICAL {
                            stable.as_str()
                        } else {
                            map.local
                                .get(stable)
                                .map_or(stable.as_str(), String::as_str)
                        };
                        let target = display_target(token, target);
                        let range = offset + start..offset + start + token.len();
                        if target != token && !protected_span(&protected, &range) {
                            patches.push((range, target.clone()));
                            let fact = RewriteFact {
                                path: path.into(),
                                selector: selector.into(),
                                old: token.into(),
                                new: target,
                                confidence: "certain".into(),
                            };
                            report.rewritten.push(fact.clone());
                            if !structured {
                                report.needs_review.push(fact);
                            }
                        }
                        offset += part.len();
                        continue;
                    }
                }
            }
            if let Some((prefix, id)) = token.rsplit_once([':', '#']) {
                let local = prefix == "trace"
                    || prefix.starts_with("logic/")
                    || prefix.starts_with("trace/")
                    || prefix == "PAPER.md"
                    || map.contains_key(prefix);
                if local && super::identity::native_numeric(prefix, id) {
                    if let Some(stable) = map.get(id) {
                        let target = if HISTORICAL {
                            stable.as_str()
                        } else {
                            map.local
                                .get(stable)
                                .map_or(stable.as_str(), String::as_str)
                        };
                        if target != id {
                            let range =
                                offset + start + prefix.len() + 1..offset + start + token.len();
                            if !protected_span(&protected, &range) {
                                patches.push((range, target.to_owned()));
                                let fact = RewriteFact {
                                    path: path.into(),
                                    selector: selector.into(),
                                    old: id.into(),
                                    new: target.into(),
                                    confidence: "certain".into(),
                                };
                                report.rewritten.push(fact.clone());
                                if !structured {
                                    report.needs_review.push(fact);
                                }
                            }
                        }
                    } else if structured {
                        return Err(MergeError::content(
                            "merge.unresolved_reference",
                            format!(
                                "incoming local qualified reference `{token}` has no destination"
                            ),
                        ));
                    }
                }
            }
            let candidate = if let Some((session, _rest)) = token.split_once([':', '#']) {
                if super::identity::session_parts(session).is_some() {
                    Some(session)
                } else {
                    None
                }
            } else if super::identity::session_parts(token).is_some()
                || token.starts_with("logic/")
                || token.starts_with("trace/")
                || token == "PAPER.md"
            {
                Some(token)
            } else {
                None
            };
            if let Some(address) = candidate
                && let Some(stable) = map.get(address)
            {
                let target = if HISTORICAL {
                    stable.as_str()
                } else {
                    map.local
                        .get(stable)
                        .map_or(stable.as_str(), String::as_str)
                };
                if address != target {
                    let range = offset + start..offset + start + address.len();
                    if !protected_span(&protected, &range) {
                        patches.push((range, target.to_owned()));
                        let fact = RewriteFact {
                            path: path.into(),
                            selector: selector.into(),
                            old: address.into(),
                            new: target.into(),
                            confidence: "certain".into(),
                        };
                        report.rewritten.push(fact.clone());
                        if !structured {
                            report.needs_review.push(fact);
                        }
                    }
                }
            }
        }
        offset += part.len();
    }
    patches.sort_by_key(|(range, _)| range.start);
    let mut result = String::with_capacity(text.len());
    let mut end = 0;
    for (range, replacement) in patches {
        if range.start < end {
            return Err(MergeError::content(
                "merge.ambiguous_reference",
                "overlapping source reference relocation",
            ));
        }
        result.push_str(&text[end..range.start]);
        result.push_str(&replacement);
        end = range.end;
    }
    result.push_str(&text[end..]);
    Ok(result)
}
/// YAML scalar delimiters express lexical style, not a quoted historical
/// address. Remove only the outer style wrapper while scanning its contents.
pub(crate) fn yaml_value(
    text: &str,
    path: &str,
    selector: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
    structured: bool,
) -> Result<String, MergeError> {
    yaml_value_mode::<false>(text, path, selector, map, report, structured)
}
pub(crate) fn yaml_value_historical(
    text: &str,
    path: &str,
    selector: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
    structured: bool,
) -> Result<String, MergeError> {
    yaml_value_mode::<true>(text, path, selector, map, report, structured)
}
fn yaml_value_mode<const HISTORICAL: bool>(
    text: &str,
    path: &str,
    selector: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
    structured: bool,
) -> Result<String, MergeError> {
    if !structured {
        let trimmed = text.trim();
        if trimmed.len() >= 2 {
            let quote = trimmed.as_bytes()[0];
            if matches!(quote, b'\'' | b'"') && trimmed.as_bytes().last() == Some(&quote) {
                let start = text.find(trimmed).expect("trimmed subslice") + 1;
                let end = start + trimmed.len() - 2;
                let inner = incoming_mode::<HISTORICAL>(
                    &text[start..end],
                    path,
                    selector,
                    map,
                    report,
                    false,
                )?;
                return Ok(format!("{}{inner}{}", &text[..start], &text[end..]));
            }
        }
    }
    incoming_mode::<HISTORICAL>(text, path, selector, map, report, structured)
}
/// Known structured values compare by their destination identity, without
/// applying the incoming allocation map to text already owned by ours.
pub(crate) fn references_equal(
    left: &serde_json::Value,
    right: &serde_json::Value,
    map: &IdentityMap,
) -> bool {
    use serde_json::Value;
    fn target<'a>(value: &'a str, map: &'a IdentityMap) -> std::borrow::Cow<'a, str> {
        if let Some(target) = map.references.get(value) {
            return std::borrow::Cow::Borrowed(target);
        }
        let normalized = if let Some((path, id)) = value.rsplit_once([':', '#']) {
            let local = path == "trace"
                || path == "PAPER.md"
                || path.starts_with("logic/")
                || path.starts_with("trace/")
                || (map.contains_key(path)
                    && !path.starts_with("src/")
                    && !path.starts_with("evidence/"));
            if local && super::identity::native_numeric(path, id) {
                std::borrow::Cow::Borrowed(id)
            } else if local && value.contains(':') {
                std::borrow::Cow::Owned(format!("{path}#{id}"))
            } else {
                std::borrow::Cow::Borrowed(value)
            }
        } else {
            std::borrow::Cow::Borrowed(value)
        };
        if let Some(target) = map.references.get(normalized.as_ref()) {
            std::borrow::Cow::Borrowed(target)
        } else {
            normalized
        }
    }
    match (left, right) {
        (Value::String(left), Value::String(right)) => target(left, map) == target(right, map),
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| references_equal(left, right, map))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, left)| {
                    right.get(key).is_some_and(|right| {
                        if matches!(
                            key.as_str(),
                            "id" | "target"
                                | "entry"
                                | "node"
                                | "claim"
                                | "pointer"
                                | "from"
                                | "to"
                                | "promoted_to"
                                | "depends_on"
                                | "bound_to"
                                | "also_depends_on"
                                | "same_as"
                                | "parent"
                        ) {
                            references_equal(left, right, map)
                        } else {
                            left == right
                        }
                    })
                })
        }
        _ => left == right,
    }
}
fn protected_span(ranges: &[std::ops::Range<usize>], target: &std::ops::Range<usize>) -> bool {
    let index = ranges.partition_point(|range| range.end <= target.start);
    ranges
        .get(index)
        .is_some_and(|range| range.start <= target.start && target.end <= range.end)
}
fn quoted_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let quote = bytes[i];
        if !matches!(quote, b'`' | b'"' | b'\'') {
            i += 1;
            continue;
        }
        if quote == b'\'' && i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() && bytes[i] != quote && bytes[i] != b'\n' {
            if bytes[i] == b'\\' {
                i += 1;
            }
            i += 1;
        }
        if i < bytes.len() && bytes[i] == quote {
            i += 1;
            ranges.push(start..i);
        }
    }
    let mut fence: Option<(u8, usize, usize)> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if let Some(marker @ (b'`' | b'~')) = trimmed.as_bytes().first().copied() {
            let count = trimmed.bytes().take_while(|byte| *byte == marker).count();
            if indent <= 3 && count >= 3 {
                if let Some((open, length, start)) = fence {
                    if open == marker && count >= length {
                        ranges.push(start..offset + line.len());
                        fence = None;
                    }
                } else {
                    fence = Some((marker, count, offset));
                }
            }
        }
        offset += line.len();
    }
    if let Some((_, _, start)) = fence {
        ranges.push(start..text.len());
    }
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
            continue;
        }
        merged.push(range);
    }
    merged
}

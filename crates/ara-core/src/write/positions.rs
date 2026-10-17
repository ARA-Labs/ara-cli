//! Lossless-value YAML event index using native UTF-8 spans. Collection assembly
//! is streaming and stack-bounded, including destruction of rejected inputs.
use super::WriteError;
use super::positions_tree::{
    attach_source, attach_yaml, copy_tree, equal_trees, json_tree, semantic_node_equal,
    semantic_shell,
};
use serde_json::Value;
use serde_saphyr::granit_parser::{Event, Parser, ScalarStyle, Span, StructureStyle};
use std::ops::Range;
use yaml_rust2::scanner::TScalarStyle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathPart {
    Key(String),
    Index(usize),
}
impl From<&str> for PathPart {
    fn from(value: &str) -> Self {
        Self::Key(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YamlKind {
    Scalar { value: String, plain: bool },
    Sequence(Vec<YamlNode>),
    Mapping(Vec<(YamlNode, YamlNode)>),
    Alias(usize),
}
#[derive(Debug)]
pub struct YamlNode {
    pub kind: YamlKind,
    pub start: usize,
    pub end: usize,
    pub flow: bool,
    pub anchor: usize,
    pub tag: Option<(String, String)>,
    pub style: Option<TScalarStyle>,
}
#[derive(Debug, Clone)]
pub struct YamlDocument {
    pub root: YamlNode,
}

impl YamlNode {
    pub fn scalar(&self) -> Option<&str> {
        if let YamlKind::Scalar { value, .. } = &self.kind {
            Some(value)
        } else {
            None
        }
    }
    pub fn mapping(&self) -> Result<&[(YamlNode, YamlNode)], WriteError> {
        self.editable()?;
        match &self.kind {
            YamlKind::Mapping(entries) => Ok(entries),
            _ => Err(unsupported("expected a YAML mapping")),
        }
    }
    pub fn sequence(&self) -> Result<&[YamlNode], WriteError> {
        self.editable()?;
        match &self.kind {
            YamlKind::Sequence(items) => Ok(items),
            _ => Err(unsupported("expected a YAML sequence")),
        }
    }
    pub fn get(&self, name: &str) -> Result<Option<&YamlNode>, WriteError> {
        let mut matches = self
            .mapping()
            .map_err(|error| {
                unsupported(format!(
                    "{} while looking up `{name}` at byte {}",
                    error.message, self.start
                ))
            })?
            .iter()
            .filter(|(key, _)| key.scalar() == Some(name));
        let value = matches.next().map(|(_, value)| value);
        if matches.next().is_some() {
            return Err(unsupported(format!("duplicate YAML key `{name}`")));
        }
        Ok(value)
    }
    pub fn at(&self, path: &[PathPart]) -> Result<&YamlNode, WriteError> {
        let mut value = self;
        for part in path {
            value = match part {
                PathPart::Key(key) => value
                    .get(key)?
                    .ok_or_else(|| unsupported(format!("missing YAML key `{key}`")))?,
                PathPart::Index(index) => value
                    .sequence()?
                    .get(*index)
                    .ok_or_else(|| unsupported("YAML index out of bounds"))?,
            };
        }
        value.editable()?;
        Ok(value)
    }
    pub fn editable(&self) -> Result<(), WriteError> {
        if self.anchor != 0 || self.tag.is_some() || matches!(self.kind, YamlKind::Alias(_)) {
            return Err(unsupported(
                "targeted YAML aliases, anchors, and tags are unsupported; source is unchanged",
            ));
        }
        Ok(())
    }
    /// Complete semantic representation preserves unknown keys, duplicate keys,
    /// scalar styles, tags, anchors, and aliases rather than normalizing them.
    pub fn semantic(&self) -> SourceValue {
        copy_tree(self, semantic_shell, attach_source)
    }
    pub fn same_source_value(&self, other: &Self) -> bool {
        equal_trees(self, other, semantic_node_equal)
    }
    pub fn to_json(&self) -> Result<Value, WriteError> {
        json_tree(self)
    }
}
#[derive(Debug)]
pub struct SourceValue {
    pub value: SourceKind,
    pub anchor: usize,
    pub tag: Option<(String, String)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceKind {
    Scalar(String, bool, Option<TScalarStyle>),
    Sequence(Vec<SourceValue>),
    Mapping(Vec<(SourceValue, SourceValue)>),
    Alias(usize),
}

impl YamlDocument {
    pub fn parse(source: &str) -> Result<Self, WriteError> {
        if source.len() > crate::flat_yaml::MAX_INPUT_BYTES {
            return Err(unsupported("YAML input exceeds the source byte budget"));
        }
        let options = serde_saphyr::granit_parser::options! {
            block_nesting_limit: crate::flat_yaml::MAX_YAML_DEPTH + 1,
            flow_nesting_limit: crate::flat_yaml::MAX_YAML_DEPTH + 1,
            emit_comments: false,
        };
        let mut frames: Vec<YamlFrame> = Vec::new();
        let mut root = None;
        let mut documents = 0;
        let mut previous_end = 0;
        for item in Parser::new_from_str_with_options(source, options) {
            let (event, span) =
                item.map_err(|error| WriteError::semantic("write.syntax", error.to_string()))?;
            let range = source_range(source, span)?;
            match event {
                Event::StreamStart | Event::StreamEnd => {}
                Event::DocumentStart(..) => {
                    documents += 1;
                    if documents > 1 {
                        return Err(unsupported("exactly one YAML document is required"));
                    }
                }
                Event::DocumentEnd => {
                    if !frames.is_empty() {
                        return Err(unsupported("unclosed YAML collection"));
                    }
                }
                Event::Scalar(value, style, anchor, tag) => {
                    let block = matches!(style, ScalarStyle::Literal | ScalarStyle::Folded);
                    let start = if block {
                        block_scalar_start(
                            source,
                            previous_end.min(range.start),
                            range.start,
                            style,
                        )
                    } else {
                        range.start
                    };
                    // A block scanner may consume the next token's indentation.
                    // Keep all prose and blank lines, but not that unrelated prefix.
                    let end = if block
                        && range.end < source.len()
                        && source[line_start(source, range.end)..range.end]
                            .bytes()
                            .all(|byte| matches!(byte, b' ' | b'\t'))
                    {
                        line_start(source, range.end)
                    } else {
                        range.end
                    };
                    let node = YamlNode {
                        kind: YamlKind::Scalar {
                            value: value.into_owned(),
                            plain: style == ScalarStyle::Plain,
                        },
                        start,
                        end,
                        flow: false,
                        anchor,
                        tag: tag.map(|tag| (tag.handle().into(), tag.suffix().into())),
                        style: Some(scalar_style(style)),
                    };
                    attach_parsed(&mut frames, &mut root, node)?;
                }
                Event::Alias(anchor) => attach_parsed(
                    &mut frames,
                    &mut root,
                    YamlNode {
                        kind: YamlKind::Alias(anchor),
                        start: range.start,
                        end: range.end,
                        flow: false,
                        anchor: 0,
                        tag: None,
                        style: None,
                    },
                )?,
                Event::SequenceStart(style, anchor, ref tag)
                | Event::MappingStart(style, anchor, ref tag) => {
                    if frames.len() >= crate::flat_yaml::MAX_YAML_DEPTH {
                        return Err(unsupported(
                            "YAML structural depth exceeds the exploration tree budget",
                        ));
                    }
                    let kind = if matches!(event, Event::SequenceStart(..)) {
                        YamlKind::Sequence(Vec::new())
                    } else {
                        YamlKind::Mapping(Vec::new())
                    };
                    frames.push(YamlFrame {
                        node: YamlNode {
                            kind,
                            start: range.start,
                            end: range.end,
                            flow: style == StructureStyle::Flow,
                            anchor,
                            tag: tag
                                .as_ref()
                                .map(|tag| (tag.handle().into(), tag.suffix().into())),
                            style: None,
                        },
                        key: None,
                    });
                }
                Event::SequenceEnd | Event::MappingEnd => {
                    let mut frame = frames
                        .pop()
                        .ok_or_else(|| unsupported("unexpected YAML collection end"))?;
                    if frame.key.is_some()
                        || !matches!(
                            (&event, &frame.node.kind),
                            (Event::SequenceEnd, YamlKind::Sequence(_))
                                | (Event::MappingEnd, YamlKind::Mapping(_))
                        )
                    {
                        return Err(unsupported("mismatched YAML collection end"));
                    }
                    frame.node.end = range.end;
                    attach_parsed(&mut frames, &mut root, frame.node)?;
                }
                Event::Comment(..) => {}
                _ => return Err(unsupported("unexpected YAML event")),
            }
            previous_end = range.end;
        }
        if documents != 1 || !frames.is_empty() {
            return Err(unsupported(
                "exactly one complete YAML document is required",
            ));
        }
        Ok(Self {
            root: root.ok_or_else(|| unsupported("YAML document has no root value"))?,
        })
    }
}

fn source_range(source: &str, span: Span) -> Result<Range<usize>, WriteError> {
    let range = span
        .byte_range()
        .ok_or_else(|| unsupported("YAML source byte offsets are unavailable"))?;
    if source.get(range.clone()).is_none() {
        return Err(unsupported("invalid YAML source byte span"));
    }
    Ok(range)
}

fn scalar_style(style: ScalarStyle) -> TScalarStyle {
    match style {
        ScalarStyle::Plain => TScalarStyle::Plain,
        ScalarStyle::SingleQuoted => TScalarStyle::SingleQuoted,
        ScalarStyle::DoubleQuoted => TScalarStyle::DoubleQuoted,
        ScalarStyle::Literal => TScalarStyle::Literal,
        ScalarStyle::Folded => TScalarStyle::Folded,
    }
}

// Granit's block scalar span covers content, not the `|` / `>` header. Recover
// only that header from the gap after the preceding event, never from prose.
fn block_scalar_start(source: &str, from: usize, content: usize, style: ScalarStyle) -> usize {
    let indicator = if style == ScalarStyle::Literal {
        '|'
    } else {
        '>'
    };
    let gap = &source[from..content];
    let mut offset = from;
    for line in gap.split_inclusive('\n') {
        let header = line.split_once('#').map_or(line, |(header, _)| header);
        if let Some(index) = header.rfind(indicator) {
            let before = &header[..index];
            let suffix = header[index + 1..].trim();
            if (before.is_empty() || before.ends_with([' ', '\t', ':', '-']))
                && suffix
                    .bytes()
                    .all(|byte| matches!(byte, b'1'..=b'9' | b'+' | b'-'))
            {
                return offset + index;
            }
        }
        offset += line.len();
    }
    content
}

struct YamlFrame {
    node: YamlNode,
    key: Option<YamlNode>,
}
fn attach_parsed(
    frames: &mut [YamlFrame],
    root: &mut Option<YamlNode>,
    node: YamlNode,
) -> Result<(), WriteError> {
    if let Some(parent) = frames.last_mut() {
        attach_yaml(&mut parent.node, &mut parent.key, node);
    } else if root.replace(node).is_some() {
        return Err(unsupported("multiple YAML root values"));
    }
    Ok(())
}
pub fn line_start(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .rfind('\n')
        .map_or(0, |i| i + 1)
}
pub fn line_end(source: &str, offset: usize) -> usize {
    source[offset.min(source.len())..]
        .find('\n')
        .map_or(source.len(), |i| offset + i + 1)
}
pub fn eol(source: &str) -> &'static str {
    if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}
pub fn field_range(
    source: &str,
    mapping: &YamlNode,
    key: &str,
) -> Result<Option<Range<usize>>, WriteError> {
    if mapping.flow {
        return Err(unsupported("editing a flow mapping is unsupported"));
    }
    let entries = mapping.mapping()?;
    let mut matches = entries
        .iter()
        .enumerate()
        .filter(|(_, (k, _))| k.scalar() == Some(key));
    let Some((index, _)) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Err(unsupported(format!("duplicate YAML key `{key}`")));
    }
    let start = line_start(source, entries[index].0.start);
    let end = entries.get(index + 1).map_or_else(
        || {
            if mapping.end >= source.len() {
                source.len()
            } else {
                line_start(source, mapping.end)
            }
        },
        |(k, _)| line_start(source, k.start),
    );
    if end < start {
        return Err(unsupported("ambiguous YAML field range"));
    }
    Ok(Some(start..end))
}
pub(super) fn unsupported(message: impl Into<String>) -> WriteError {
    WriteError::semantic("write.unsupported_source", message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn block_sequence_compact_json_mapping_retains_mapping_and_exact_span() {
        let row = r#"{"action":"rename","from_selector":{"document":"logic/claims.md","entry":"C77"},"to":null}"#;
        let text = format!("mutations:\n  - {row}\n");
        let doc = YamlDocument::parse(&text).unwrap();
        let entry = &doc
            .root
            .get("mutations")
            .unwrap()
            .unwrap()
            .sequence()
            .unwrap()[0];
        assert_eq!(
            entry.get("action").unwrap().unwrap().scalar(),
            Some("rename")
        );
        assert_eq!(&text[entry.start..entry.end], row);
    }

    #[test]
    fn unicode_crlf_block_offsets_are_structural() {
        let text = "meta: café\r\ntree:\r\n  - id: N01\r\n    title: |\r\n      id: N999\r\n      你好\r\n    children: []\r\n";
        let doc = YamlDocument::parse(text).unwrap();
        let node = &doc.root.get("tree").unwrap().unwrap().sequence().unwrap()[0];
        let id = node.get("id").unwrap().unwrap();
        assert_eq!(&text[id.start..id.end], "N01");
        assert_eq!(
            node.get("title").unwrap().unwrap().scalar(),
            Some("id: N999\n你好\n")
        );
        assert_eq!(
            node.get("children")
                .unwrap()
                .unwrap()
                .sequence()
                .unwrap()
                .len(),
            0
        );
    }
    #[test]
    fn complete_representation_keeps_duplicate_unknown_values() {
        let a = YamlDocument::parse("known: ok\nunknown: {a: [1, 2]}\nunknown: tagged\n").unwrap();
        let b = YamlDocument::parse("known: ok\nunknown: {a: [1, 3]}\nunknown: tagged\n").unwrap();
        assert_ne!(a.root.semantic(), b.root.semantic());
        assert!(a.root.get("unknown").is_err());
    }
    #[test]
    fn aliases_tags_anchors_are_indexed_but_not_editable() {
        for source in ["x: &a {v: 1}\ny: *a\n", "x: !custom value\n"] {
            let doc = YamlDocument::parse(source).unwrap();
            assert!(doc.root.get("x").unwrap().unwrap().to_json().is_err());
        }
    }
    #[test]
    fn folded_and_flow_sources_retain_values() {
        let doc = YamlDocument::parse("x: >-\n  one\n  two\ny: [a, {b: c}]\n").unwrap();
        assert_eq!(
            doc.root.get("x").unwrap().unwrap().scalar(),
            Some("one two")
        );
        assert!(doc.root.get("y").unwrap().unwrap().flow);
    }

    #[test]
    fn block_records_retain_their_first_key_and_all_unknown_root_evidence() {
        let text = "session:\r\n  events_logged:\r\n    - id: N01\r\n      summary: café\r\n      turn: 2\r\nextension:\r\n  nested: [1, {message: 'all incoming bytes'}]\r\n  absent_is_not_null: null\r\n";
        let doc = YamlDocument::parse(text).unwrap();
        let event = &doc
            .root
            .get("session")
            .unwrap()
            .unwrap()
            .get("events_logged")
            .unwrap()
            .unwrap()
            .sequence()
            .unwrap()[0];
        assert_eq!(
            &text[event.start..event.end],
            "id: N01\r\n      summary: café\r\n      turn: 2\r\n"
        );
        assert_eq!(
            event.to_json().unwrap(),
            serde_json::json!({"id":"N01","summary":"café","turn":2})
        );
        let extension = doc.root.get("extension").unwrap().unwrap();
        assert_eq!(
            &text[extension.start..extension.end],
            "nested: [1, {message: 'all incoming bytes'}]\r\n  absent_is_not_null: null\r\n"
        );
        assert_eq!(
            extension.to_json().unwrap(),
            serde_json::json!({"nested":[1,{"message":"all incoming bytes"}],"absent_is_not_null":null})
        );
    }

    #[test]
    fn literal_and_folded_spans_cover_headers_and_first_hash_prose_not_next_comments() {
        let text = "literal: |2- # header comment\r\n  # first literal prose\r\n  café 你好\r\n# outside literal\r\nfolded: >-\r\n  # first folded prose\r\n  snow 雪\r\nnext: [\"你好\", {'quoted': 'café'}] # outside flow\r\n";
        let doc = YamlDocument::parse(text).unwrap();
        let literal = doc.root.get("literal").unwrap().unwrap();
        assert_eq!(
            &text[literal.start..literal.end],
            "|2- # header comment\r\n  # first literal prose\r\n  café 你好\r\n"
        );
        assert_eq!(literal.scalar(), Some("# first literal prose\ncafé 你好"));
        let folded = doc.root.get("folded").unwrap().unwrap();
        assert_eq!(
            &text[folded.start..folded.end],
            ">-\r\n  # first folded prose\r\n  snow 雪\r\n"
        );
        assert_eq!(folded.scalar(), Some("# first folded prose snow 雪"));
        let next = doc.root.get("next").unwrap().unwrap();
        assert_eq!(
            &text[next.start..next.end],
            "[\"你好\", {'quoted': 'café'}]"
        );
        assert_eq!(
            next.to_json().unwrap(),
            serde_json::json!(["你好",{"quoted":"café"}])
        );
    }

    #[test]
    fn multiline_plain_and_quoted_scalars_have_complete_lexical_ranges() {
        let text = "plain: café\r\n  你好\r\nquoted: \"first\\n雪\" # not scalar\r\nsingle: 'it''s café'\r\nempty:\r\nnext: value\r\n";
        let doc = YamlDocument::parse(text).unwrap();
        for (key, raw, value) in [
            ("plain", "café\r\n  你好", "café 你好"),
            ("quoted", "\"first\\n雪\"", "first\n雪"),
            ("single", "'it''s café'", "it's café"),
        ] {
            let node = doc.root.get(key).unwrap().unwrap();
            assert_eq!(&text[node.start..node.end], raw);
            assert_eq!(node.scalar(), Some(value));
        }
        assert_eq!(
            doc.root.get("empty").unwrap().unwrap().to_json().unwrap(),
            Value::Null
        );
        assert_eq!(
            doc.root.get("next").unwrap().unwrap().scalar(),
            Some("value")
        );
    }
}

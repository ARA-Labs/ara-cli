//! Flat event storage and shallow serde views: tree depth never consumes the call stack.
use crate::schema::{RawDoc, RawNode};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_saphyr::granit_parser::{Event, Parser, ScalarStyle, Span, Tag};
use std::borrow::Cow;
use std::cell::{Cell, OnceCell};
use std::collections::{BTreeMap, BTreeSet};

/// One mapping and one children sequence per node, plus the document mapping.
pub(crate) const MAX_TREE_DEPTH: usize = 10_000;
pub(crate) const MAX_YAML_DEPTH: usize = MAX_TREE_DEPTH * 2 + 2;
pub(crate) const MAX_INPUT_BYTES: usize = 1024 * 1024 * 1024;
const MAX_METADATA_DEPTH: usize = 64;
const MAX_NODES: usize = 250_000;
type Error = de::value::Error;
fn error(message: impl std::fmt::Display) -> Error {
    de::Error::custom(message)
}

#[derive(Debug)]
enum Kind<'a> {
    Scalar(Cow<'a, str>, ScalarStyle),
    Sequence(Vec<usize>),
    Mapping(Vec<usize>),
    Alias(usize),
}
#[derive(Debug)]
enum Fields {
    Source,
    Expanded(Vec<usize>),
}
#[derive(Debug)]
struct Entry<'a> {
    kind: Kind<'a>,
    span: Span,
    tag: Option<Cow<'a, Tag>>,
    fields: OnceCell<Fields>,
}
struct Arena<'a> {
    source: &'a str,
    entries: Vec<Entry<'a>>,
    anchors: BTreeMap<usize, usize>,
    visits: Cell<usize>,
}
impl<'a> Arena<'a> {
    fn parse(source: &'a str, shallow_tags: bool) -> Result<Self, String> {
        let options = serde_saphyr::granit_parser::options! { block_nesting_limit: MAX_YAML_DEPTH + 1, flow_nesting_limit: MAX_YAML_DEPTH, emit_comments: false };
        let mut arena = Self {
            source,
            entries: Vec::new(),
            anchors: BTreeMap::new(),
            visits: Cell::new(0),
        };
        let mut pending: Vec<usize> = Vec::new();
        let mut roots = 0;
        for item in Parser::new_from_str_with_options(source, options) {
            let (event, span) = item.map_err(|e| e.to_string())?;
            let (kind, anchor, container, tag) = match event {
                Event::Scalar(value, style, anchor, tag) => {
                    (Kind::Scalar(value, style), anchor, false, tag)
                }
                Event::SequenceStart(_, anchor, tag) => {
                    if tag.is_some() && !shallow_tags {
                        return Err("tagged YAML collections are unsupported".into());
                    }
                    (Kind::Sequence(Vec::new()), anchor, true, tag)
                }
                Event::MappingStart(_, anchor, tag) => {
                    if tag.is_some() && !shallow_tags {
                        return Err("tagged YAML collections are unsupported".into());
                    }
                    (Kind::Mapping(Vec::new()), anchor, true, tag)
                }
                Event::Alias(anchor) => (Kind::Alias(anchor), 0, false, None),
                Event::SequenceEnd | Event::MappingEnd => {
                    pending.pop().ok_or("unbalanced YAML collections")?;
                    continue;
                }
                Event::StreamStart
                | Event::StreamEnd
                | Event::DocumentStart(..)
                | Event::DocumentEnd
                | Event::Comment(..) => continue,
                _ => return Err("unsupported YAML event".into()),
            };
            let index = arena.entries.len();
            if let Some(core) = tag.as_ref().and_then(|tag| tag.core_suffix()) {
                let valid = match core {
                    "map" => matches!(kind, Kind::Mapping(_)),
                    "seq" => matches!(kind, Kind::Sequence(_)),
                    "str" | "bool" | "null" | "int" | "float" => matches!(kind, Kind::Scalar(..)),
                    _ => true,
                };
                if !valid {
                    return Err(format!("YAML tag {core} does not match node kind"));
                }
            }
            arena.entries.push(Entry {
                kind,
                span,
                tag,
                fields: OnceCell::new(),
            });
            if anchor != 0 {
                arena.anchors.insert(anchor, index);
            }
            if let Some(&parent) = pending.last() {
                match &mut arena.entries[parent].kind {
                    Kind::Sequence(items) | Kind::Mapping(items) => items.push(index),
                    _ => return Err("invalid YAML container".into()),
                }
            } else {
                roots += 1;
            }
            if container {
                pending.push(index);
            }
        }
        if roots != 1 || !pending.is_empty() {
            return Err("exactly one YAML document is required".into());
        }
        Ok(arena)
    }
    fn resolve(&self, mut index: usize) -> Result<usize, Error> {
        for _ in 0..=self.anchors.len() {
            match self.entries[index].kind {
                Kind::Alias(anchor) => {
                    index = *self
                        .anchors
                        .get(&anchor)
                        .ok_or_else(|| error("undefined YAML alias"))?
                }
                _ => return Ok(index),
            }
        }
        Err(error("cyclic YAML alias"))
    }
    fn key(&self, index: usize) -> Result<&str, Error> {
        let index = self.resolve(index)?;
        match &self.entries[index].kind {
            Kind::Scalar(value, _) => Ok(value),
            _ => Err(error("mapping keys must be scalars")),
        }
    }
    fn merge_key(&self, index: usize) -> Result<bool, Error> {
        let index = self.resolve(index)?;
        let entry = &self.entries[index];
        if let Some(tag) = &entry.tag {
            return Ok(tag.suffix_in_namespace("tag:yaml.org,2002:").as_deref() == Some("merge"));
        }
        Ok(matches!(&entry.kind, Kind::Scalar(value, ScalarStyle::Plain) if value == "<<"))
    }
    fn fields(&self, index: usize) -> Result<&[usize], Error> {
        self.fields_bounded(index, 0)
    }
    fn fields_bounded(&self, index: usize, depth: usize) -> Result<&[usize], Error> {
        if depth > MAX_METADATA_DEPTH {
            return Err(error("YAML merge depth exceeds 64"));
        }
        let index = self.resolve(index)?;
        let entry = &self.entries[index];
        if let Some(Fields::Expanded(fields)) = entry.fields.get() {
            return Ok(fields);
        }
        let Kind::Mapping(items) = &entry.kind else {
            return Err(error("expected a YAML mapping"));
        };
        if matches!(entry.fields.get(), Some(Fields::Source)) {
            return Ok(items);
        }
        if items.len() % 2 != 0 {
            return Err(error("mapping has a missing value"));
        }
        let mut direct = BTreeSet::new();
        let mut merges = Vec::new();
        for pair in items.chunks_exact(2) {
            let key = self.key(pair[0])?;
            if !direct.insert(key) {
                return Err(error(format!("duplicate YAML key `{key}`")));
            }
            if self.merge_key(pair[0])? {
                merges.push(pair[1]);
            }
        }
        if merges.is_empty() {
            entry
                .fields
                .set(Fields::Source)
                .map_err(|_| error("YAML field cache already populated"))?;
            return Ok(items);
        }
        let mut output = Vec::new();
        for pair in items.chunks_exact(2) {
            if !self.merge_key(pair[0])? {
                output.extend_from_slice(pair);
            }
        }
        let mut seen = direct;
        for merge in merges {
            let index = self.resolve(merge)?;
            let sources: &[usize] = match &self.entries[index].kind {
                Kind::Mapping(_) => std::slice::from_ref(&index),
                Kind::Sequence(items) => items,
                _ => {
                    return Err(error(
                        "YAML merge requires a mapping or sequence of mappings",
                    ));
                }
            };
            for &source in sources {
                for pair in self.fields_bounded(source, depth + 1)?.chunks_exact(2) {
                    if seen.insert(self.key(pair[0])?) {
                        if output.len() >= MAX_NODES * 2 {
                            return Err(error("expanded YAML mapping budget exceeded"));
                        }
                        output.extend_from_slice(pair);
                    }
                }
            }
        }
        entry
            .fields
            .set(Fields::Expanded(output))
            .map_err(|_| error("YAML field cache already populated"))?;
        match entry.fields.get().expect("field cache populated") {
            Fields::Expanded(fields) => Ok(fields),
            Fields::Source => Ok(items),
        }
    }
    fn field(&self, index: usize, name: &str) -> Result<Option<usize>, Error> {
        for pair in self.fields(index)?.chunks_exact(2) {
            if self.key(pair[0])? == name {
                return Ok(Some(pair[1]));
            }
        }
        Ok(None)
    }
    fn sequence(&self, index: usize) -> Result<&[usize], Error> {
        let index = self.resolve(index)?;
        match &self.entries[index].kind {
            Kind::Sequence(items) => Ok(items),
            Kind::Scalar(value, ScalarStyle::Plain)
                if matches!(value.as_ref(), "" | "~" | "null" | "Null" | "NULL") =>
            {
                Ok(&[])
            }
            _ => Err(error("expected a YAML sequence")),
        }
    }
    fn view(&self, index: usize, omit: &'static [&'static str], depth: usize) -> View<'_, 'a> {
        View {
            arena: self,
            index,
            omit,
            depth,
        }
    }
    fn nodes(&self, roots: &[usize]) -> Result<Vec<RawNode>, Error> {
        struct Frame<'a> {
            node: RawNode,
            children: &'a [usize],
            next: usize,
        }
        let mut output = Vec::new();
        let mut frames: Vec<Frame> = Vec::new();
        let mut todo = roots.iter().copied();
        let mut count = 0;
        loop {
            let index = if let Some(frame) = frames.last_mut() {
                if let Some(&index) = frame.children.get(frame.next) {
                    frame.next += 1;
                    Some(index)
                } else {
                    let frame = frames.pop().expect("frame exists");
                    if let Some(parent) = frames.last_mut() {
                        parent.node.children.push(frame.node);
                    } else {
                        output.push(frame.node);
                    }
                    continue;
                }
            } else {
                todo.next()
            };
            let Some(index) = index else {
                break;
            };
            if frames.len() >= MAX_TREE_DEPTH {
                return Err(error(format!(
                    "exploration tree depth exceeds {MAX_TREE_DEPTH}"
                )));
            }
            count += 1;
            if count > MAX_NODES {
                return Err(error("expanded exploration node budget exceeded"));
            }
            let node = RawNode::deserialize(self.view(index, &["children"], 0)).map_err(|e| {
                let span = self.entries[index].span;
                error(format!(
                    "{e} (node at line {}, column {})",
                    span.start.line(),
                    span.start.col() + 1
                ))
            })?;
            let children = self
                .field(index, "children")?
                .map(|i| self.sequence(i))
                .transpose()
                .map_err(|e| {
                    let span = self.entries[index].span;
                    error(format!(
                        "{e} (node at line {}, column {})",
                        span.start.line(),
                        span.start.col() + 1
                    ))
                })?
                .unwrap_or(&[]);
            frames.push(Frame {
                node,
                children,
                next: 0,
            });
        }
        Ok(output)
    }
}

#[derive(Clone, Copy)]
struct View<'v, 'a> {
    arena: &'v Arena<'a>,
    index: usize,
    omit: &'static [&'static str],
    depth: usize,
}
impl<'v, 'a> View<'v, 'a> {
    fn resolved(self) -> Result<Self, Error> {
        let visits = self.arena.visits.get() + 1;
        if visits > 1_000_000 {
            return Err(error("expanded YAML value budget exceeded"));
        }
        self.arena.visits.set(visits);
        if self.depth > MAX_METADATA_DEPTH {
            return Err(error(format!(
                "YAML metadata depth exceeds {MAX_METADATA_DEPTH}"
            )));
        }
        Ok(Self {
            index: self.arena.resolve(self.index)?,
            ..self
        })
    }
    fn scalar(self) -> Result<&'a str, Error> {
        let span = self.arena.entries[self.index].span;
        let mut range = span
            .byte_range()
            .ok_or_else(|| error("YAML scalar byte offsets unavailable"))?;
        if let Some(tag) = span.tag_start() {
            range.start = tag
                .byte_offset()
                .ok_or_else(|| error("YAML tag byte offset unavailable"))?;
        }
        self.arena
            .source
            .get(range)
            .ok_or_else(|| error("invalid YAML scalar span"))
    }
    fn child(self, index: usize) -> Self {
        Self {
            index,
            omit: &[],
            depth: self.depth + 1,
            ..self
        }
    }
}
macro_rules! scalar_method {
    ($($name:ident),* $(,)?) => {$ (
        fn $name<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, Error> {
            let this = self.resolved()?;
            if let Kind::Scalar(value, ScalarStyle::Literal | ScalarStyle::Folded) = &this.arena.entries[this.index].kind {
                de::value::StringDeserializer::<Error>::new(value.clone().into_owned()).$name(visitor)
            } else if matches!(this.arena.entries[this.index].kind, Kind::Scalar(..)) {
                serde_saphyr::with_deserializer_from_str(this.scalar()?, |de| de.$name(visitor)).map_err(error)
            } else { this.deserialize_any(visitor) }
        }
    )*};
}
impl<'v, 'a> Deserializer<'a> for View<'v, 'a> {
    type Error = Error;
    fn deserialize_any<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, Error> {
        let this = self.resolved()?;
        match &this.arena.entries[this.index].kind {
            Kind::Scalar(value, ScalarStyle::Literal | ScalarStyle::Folded) => {
                visitor.visit_string(value.clone().into_owned())
            }
            Kind::Scalar(..) => serde_saphyr::with_deserializer_from_str(this.scalar()?, |de| {
                de.deserialize_any(visitor)
            })
            .map_err(error),
            Kind::Sequence(items) => visitor.visit_seq(Sequence {
                view: this,
                items: items.iter(),
            }),
            Kind::Mapping(_) => visitor.visit_map(Mapping {
                view: this,
                items: this.arena.fields(this.index)?.chunks_exact(2),
                value: None,
            }),
            Kind::Alias(_) => Err(error("unresolved YAML alias")),
        }
    }
    fn deserialize_option<V: Visitor<'a>>(self, visitor: V) -> Result<V::Value, Error> {
        let this = self.resolved()?;
        if matches!(
            this.arena.entries[this.index].kind,
            Kind::Scalar(_, ScalarStyle::Literal | ScalarStyle::Folded)
        ) {
            visitor.visit_some(this)
        } else if matches!(this.arena.entries[this.index].kind, Kind::Scalar(..)) {
            serde_saphyr::with_deserializer_from_str(this.scalar()?, |de| {
                de.deserialize_option(visitor)
            })
            .map_err(error)
        } else {
            visitor.visit_some(this)
        }
    }
    fn deserialize_newtype_struct<V: Visitor<'a>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_struct<V: Visitor<'a>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_tuple<V: Visitor<'a>>(self, _: usize, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_tuple_struct<V: Visitor<'a>>(
        self,
        _: &'static str,
        _: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_any(visitor)
    }
    fn deserialize_enum<V: Visitor<'a>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        let this = self.resolved()?;
        serde_saphyr::with_deserializer_from_str(this.scalar()?, |de| {
            de.deserialize_enum(name, variants, visitor)
        })
        .map_err(error)
    }
    scalar_method!(
        deserialize_bool,
        deserialize_i8,
        deserialize_i16,
        deserialize_i32,
        deserialize_i64,
        deserialize_u8,
        deserialize_u16,
        deserialize_u32,
        deserialize_u64,
        deserialize_f32,
        deserialize_f64,
        deserialize_char,
        deserialize_str,
        deserialize_string,
        deserialize_bytes,
        deserialize_byte_buf,
        deserialize_unit,
        deserialize_seq,
        deserialize_map,
        deserialize_identifier,
        deserialize_ignored_any
    );
    fn deserialize_unit_struct<V: Visitor<'a>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_unit(visitor)
    }
}
struct Sequence<'v, 'a> {
    view: View<'v, 'a>,
    items: std::slice::Iter<'v, usize>,
}
impl<'v, 'a> SeqAccess<'a> for Sequence<'v, 'a> {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'a>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        self.items
            .next()
            .map(|&index| seed.deserialize(self.view.child(index)))
            .transpose()
    }
}
struct Mapping<'v, 'a> {
    view: View<'v, 'a>,
    items: std::slice::ChunksExact<'v, usize>,
    value: Option<usize>,
}
impl<'v, 'a> MapAccess<'a> for Mapping<'v, 'a> {
    type Error = Error;
    fn next_key_seed<T: DeserializeSeed<'a>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        for pair in self.items.by_ref() {
            if self.view.omit.contains(&self.view.arena.key(pair[0])?) {
                continue;
            }
            self.value = Some(pair[1]);
            return seed.deserialize(self.view.child(pair[0])).map(Some);
        }
        Ok(None)
    }
    fn next_value_seed<T: DeserializeSeed<'a>>(&mut self, seed: T) -> Result<T::Value, Error> {
        seed.deserialize(
            self.view.child(
                self.value
                    .take()
                    .ok_or_else(|| error("missing mapping value"))?,
            ),
        )
    }
}

fn inspected_depth(source: &str) -> Result<usize, String> {
    if source.len() > MAX_INPUT_BYTES {
        return Err(format!("YAML input exceeds {MAX_INPUT_BYTES} bytes"));
    }
    let budget = serde_saphyr::budget! { max_depth: MAX_YAML_DEPTH, flow_nesting_limit: MAX_YAML_DEPTH + 1, max_documents: 1 };
    let report = serde_saphyr::budget::check_yaml_budget(
        source,
        budget.expect("explicit YAML budget"),
        serde_saphyr::budget::EnforcingPolicy::AllContent,
    )
    .map_err(|e| e.to_string())?;
    if let Some(breach) = report.breached {
        return Err(format!("YAML resource budget exceeded: {breach:?}"));
    }
    Ok(report.max_depth)
}

pub(crate) fn parse(source: &str) -> Result<RawDoc, String> {
    let depth = inspected_depth(source)?;
    // Preserve the existing bounded YAML semantics without deeply recursive serde.
    if depth <= MAX_METADATA_DEPTH {
        return serde_saphyr::from_str::<RawDoc>(source).map_err(|e| e.to_string());
    }
    let arena = Arena::parse(source, false)?;
    let mut doc =
        RawDoc::deserialize(arena.view(0, &["tree", "root"], 0)).map_err(|e| e.to_string())?;
    if let Some(tree) = arena.field(0, "tree").map_err(|e| e.to_string())? {
        let index = arena.resolve(tree).map_err(|e| e.to_string())?;
        if !matches!(arena.entries[index].kind, Kind::Scalar(..)) {
            doc.tree = Some(
                arena
                    .nodes(arena.sequence(tree).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
            );
        } else {
            doc.tree = Option::<Vec<RawNode>>::deserialize(arena.view(tree, &[], 0))
                .map_err(|e| e.to_string())?;
        }
    }
    if let Some(root) = arena.field(0, "root").map_err(|e| e.to_string())? {
        let index = arena.resolve(root).map_err(|e| e.to_string())?;
        if !matches!(arena.entries[index].kind, Kind::Scalar(..)) {
            doc.root = arena
                .nodes(&[root])
                .map_err(|e| e.to_string())?
                .pop()
                .map(Box::new);
        } else {
            doc.root = Option::<Box<RawNode>>::deserialize(arena.view(root, &[], 0))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(doc)
}

/// Decode the complete source-spelled fields of selected exploration nodes.
///
/// `children` is omitted at each selected node rather than recursively copied.
/// Unknown fields and bounded metadata are retained; IDs are matched after
/// trimming, like normalization. Missing or ambiguous selected IDs are errors.
/// This is pure and filesystem-independent, with the same input/event/depth
/// budgets as exploration parsing. No whole-tree dynamic value is constructed.
pub fn source_node_fields(
    source: &str,
    requested: &[&str],
) -> Result<BTreeMap<String, BTreeMap<String, crate::manifest::SourceValue>>, String> {
    let depth = inspected_depth(source)?;
    let arena = Arena::parse(source, depth <= MAX_METADATA_DEPTH)?;
    let tree = arena.field(0, "tree").map_err(|e| e.to_string())?;
    let root = arena.field(0, "root").map_err(|e| e.to_string())?;
    let mut pending: Vec<_> = match (tree, root) {
        (Some(tree), None) => arena
            .sequence(tree)
            .map_err(|e| e.to_string())?
            .iter()
            .rev()
            .map(|&index| (index, 1usize))
            .collect(),
        (None, Some(root)) => vec![(root, 1usize)],
        (Some(_), Some(_)) => return Err("both tree and root are present".into()),
        (None, None) => return Err("neither tree nor root is present".into()),
    };
    let wanted: BTreeSet<&str> = requested.iter().map(|id| id.trim()).collect();
    let mut seen = BTreeSet::new();
    let mut output = BTreeMap::new();
    let mut count = 0;
    while let Some((index, depth)) = pending.pop() {
        if depth > MAX_TREE_DEPTH {
            return Err(format!("exploration tree depth exceeds {MAX_TREE_DEPTH}"));
        }
        count += 1;
        if count > MAX_NODES {
            return Err("expanded exploration node budget exceeded".into());
        }
        let identity = arena
            .field(index, "id")
            .map_err(|e| e.to_string())?
            .ok_or("node is missing id")?;
        let raw_identity =
            String::deserialize(arena.view(identity, &[], 0)).map_err(|e| e.to_string())?;
        let identity = raw_identity.trim();
        if seen.contains(identity) {
            return Err(format!("duplicate node id `{identity}`"));
        }
        if wanted.contains(identity) {
            let fields = NodeProjection::deserialize(arena.view(index, &["children"], 0))
                .map_err(|e| e.to_string())?
                .0;
            output.insert(identity.to_owned(), fields);
        }
        let identity = if identity.len() == raw_identity.len() {
            raw_identity
        } else {
            identity.to_owned()
        };
        seen.insert(identity);
        if let Some(children) = arena.field(index, "children").map_err(|e| e.to_string())? {
            let children = arena.sequence(children).map_err(|e| e.to_string())?;
            pending.extend(children.iter().rev().map(|&child| (child, depth + 1)));
        }
    }
    for identity in wanted {
        if !output.contains_key(identity) {
            return Err(format!("unknown selected node `{identity}`"));
        }
    }
    Ok(output)
}

struct NodeProjection(BTreeMap<String, crate::manifest::SourceValue>);
impl<'de> Deserialize<'de> for NodeProjection {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct ProjectionVisitor;
        impl<'de> Visitor<'de> for ProjectionVisitor {
            type Value = NodeProjection;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a node field mapping")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut fields = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, ProjectedValue>()? {
                    fields.insert(key, value.0);
                }
                Ok(NodeProjection(fields))
            }
        }
        de.deserialize_map(ProjectionVisitor)
    }
}

// Use an ordinary value visitor, not Serde's private untagged-enum content
// buffer: opaque YAML tags are presentation, not synthetic source-field keys.
struct ProjectedValue(crate::manifest::SourceValue);
impl<'de> Deserialize<'de> for ProjectedValue {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        use crate::manifest::SourceValue;
        struct ValueVisitor;
        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = ProjectedValue;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a source value")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(ProjectedValue(SourceValue::Null))
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(ProjectedValue(SourceValue::Bool(value)))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(ProjectedValue(SourceValue::Integer(value)))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(ProjectedValue(i64::try_from(value).map_or(
                    SourceValue::Unsigned(value),
                    SourceValue::Integer,
                )))
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(ProjectedValue(SourceValue::Float(value)))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                self.visit_string(value.to_owned())
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(ProjectedValue(SourceValue::String(value)))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element::<ProjectedValue>()? {
                    values.push(value.0);
                }
                Ok(ProjectedValue(SourceValue::Sequence(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, ProjectedValue>()? {
                    values.insert(key, value.0);
                }
                Ok(ProjectedValue(SourceValue::Mapping(values)))
            }
        }
        de.deserialize_any(ValueVisitor)
    }
}

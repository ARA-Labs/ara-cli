//! Mutable Markdown layers, indexed once and merged by exact source spans.
//! This adapter implements the concrete proposal; protocol review is separate.
use super::types::{
    ConflictLocator, EntryIdentity, IdentityMap, MergeConflict, MergeError, MergeOptions,
    MergeReport, MergeValue, RewriteFact, conflict,
};
use crate::{
    markdown as index,
    write::{
        ArtifactSnapshot, EntrySelector, WorkingArtifact, documents, fields,
        positions::{YamlDocument, field_range},
    },
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Identity {
    Native(String),
    Heading {
        document: String,
        heading: Vec<String>,
    },
}
impl Identity {
    fn key(address: &str) -> Self {
        if !matches!(address.trim_start().as_bytes().first(), Some(b'{' | b'[')) {
            return Self::Native(address.into());
        }
        match serde_json::from_str::<EntrySelector>(address) {
            Ok(EntrySelector::Document {
                document,
                heading,
                entry: None,
            }) => Self::Heading { document, heading },
            _ => Self::Native(address.into()),
        }
    }
}
pub(crate) struct Inventory {
    pub entries: Vec<EntryIdentity>,
    pub paths: BTreeSet<String>,
    docs: BTreeMap<String, Document>,
    addresses: BTreeMap<Identity, usize>,
}
struct Document {
    text: String,
    entries: Vec<Entry>,
    by_address: BTreeMap<Identity, usize>,
    by_start: BTreeMap<usize, usize>,
    by_heading: BTreeMap<Vec<String>, Option<usize>>,
    by_entry: BTreeMap<String, Option<usize>>,
}
struct Entry {
    address: String,
    parent: Option<String>,
    range: Range<usize>,
    own: Range<usize>,
    numeric: Option<char>,
    atoms: Vec<Atom>,
    level: usize,
    literal_path: Vec<String>,
}
struct Atom {
    key: String,
    range: Range<usize>,
    semantic: Option<Value>,
    structured: bool,
    writable: bool,
    kind: AtomKind,
}
#[derive(Clone, Copy)]
enum AtomKind {
    Title { identified: bool },
    Field,
    Residual,
    Yaml,
}
struct Patch {
    range: Range<usize>,
    text: String,
}

fn numeric(path: &str, heading: &str, level: usize) -> Option<char> {
    if level != 2 {
        return None;
    }
    let (prefix, spelling) = match path {
        "logic/claims.md" => ('C', "C"),
        "logic/solution/heuristics.md" => ('H', "H"),
        "logic/experiments.md" => ('E', "E"),
        _ => return None,
    };
    let id = heading.split([':', ' ', '\t']).next().unwrap_or(heading);
    fields::typed_id(id, spelling).then_some(prefix)
}
fn heading_component(path: &str, heading: &str, level: usize) -> String {
    if numeric(path, heading, level).is_some() {
        return heading.split([':', ' ', '\t']).next().unwrap().into();
    }
    heading.to_owned()
}
/// Internal identity keys retain literal components. The existing document
/// selector serialization is an opaque ledger key, never a new user locator.
/// Ordinary addresses keep their historical spelling when it is lossless.
pub(crate) fn heading_address(path: &str, components: &[String]) -> String {
    if components.iter().any(|component| component.contains('/')) {
        exact_heading_address(path, components)
    } else {
        heading_display_address(path, components)
    }
}
fn heading_display_address(path: &str, components: &[String]) -> String {
    if path == "logic/concepts.md" && components.len() == 2 {
        format!("{path}#{}", components[1])
    } else {
        format!("{path}#{}", components.join("/"))
    }
}
pub(crate) fn exact_heading_address(path: &str, components: &[String]) -> String {
    serde_json::to_string(&EntrySelector::Document {
        document: path.into(),
        heading: components.to_vec(),
        entry: None,
    })
    .expect("serializable native selector")
}
pub(crate) fn exact_selector_key(selector: &EntrySelector) -> Option<String> {
    match selector {
        EntrySelector::Document {
            document,
            heading,
            entry: None,
        } if !heading.is_empty() => {
            let components = heading
                .iter()
                .enumerate()
                .map(|(ordinal, heading)| heading_component(document, heading, ordinal + 1))
                .collect::<Vec<_>>();
            Some(exact_heading_address(document, &components))
        }
        _ => selector_key(selector),
    }
}
pub(crate) fn display_address(address: &str) -> String {
    if !matches!(address.trim_start().as_bytes().first(), Some(b'{' | b'[')) {
        return address.into();
    }
    match serde_json::from_str::<EntrySelector>(address) {
        Ok(EntrySelector::Document {
            document,
            heading,
            entry: None,
        }) => {
            let components = if document == "logic/concepts.md" && heading.len() == 2 {
                &heading[1..]
            } else {
                &heading[..]
            };
            format!("{document}#{}", components.join("/"))
        }
        _ => address.into(),
    }
}
/// Exact archived keys are built from the authoritative vector, never from
/// the native display string. Numeric roots retain their typed namespace.
pub(crate) fn selector_key(selector: &EntrySelector) -> Option<String> {
    match selector {
        EntrySelector::Id { id } => Some(id.clone()),
        EntrySelector::Document {
            document,
            heading,
            entry,
        } => {
            if let Some(entry) = entry {
                return Some(super::identity::normalize_local(&format!(
                    "{document}#{entry}"
                )));
            }
            if heading.is_empty() {
                return None;
            }
            let components: Vec<String> = heading
                .iter()
                .enumerate()
                .map(|(ordinal, heading)| heading_component(document, heading, ordinal + 1))
                .collect();
            if components.len() == 2 && numeric(document, &heading[1], 2).is_some() {
                Some(components[1].clone())
            } else if document == "logic/related_work.md"
                && components.len() == 2
                && rw_id(&heading[1]).is_some()
            {
                Some(rw_id(&heading[1]).expect("checked related-work ID").into())
            } else {
                Some(heading_address(document, &components))
            }
        }
    }
}
/// The title of an identified heading, split as the claims parser splits a
/// claim heading (`:` or a spaced dash), else after the first `:`.
fn identified_title(heading: &str) -> &str {
    crate::claims::claim_heading(heading).map_or_else(
        || {
            heading
                .split_once(':')
                .map_or(heading, |(_, title)| title)
                .trim()
        },
        |(_, title)| title,
    )
}
fn rw_id(heading: &str) -> Option<&str> {
    let id = heading.split_once(':')?.0.trim();
    (id.starts_with("RW") && id.len() > 2 && !id.chars().any(char::is_whitespace)).then_some(id)
}
fn paper_key(key: &str) -> bool {
    matches!(
        key,
        "title"
            | "authors"
            | "year"
            | "venue"
            | "doi"
            | "ara_version"
            | "domain"
            | "keywords"
            | "claims_summary"
            | "abstract"
            | "knowledge_paths"
    )
}
fn frontmatter(text: &str) -> Result<Option<(Range<usize>, usize)>, MergeError> {
    let mut offset = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\r', '\n']);
        if let Some(start) = start {
            if bare == "---" {
                return Ok(Some((start..offset, offset + line.len())));
            }
        } else if bare.trim_start_matches('\u{feff}').trim().is_empty() {
            offset += line.len();
            continue;
        } else if bare.trim_start_matches('\u{feff}') == "---" {
            start = Some(offset + line.len());
        } else {
            return Ok(None);
        }
        offset += line.len();
    }
    if start.is_some() {
        Err(MergeError::content(
            "merge.markdown_frontmatter",
            "Unclosed PAPER frontmatter",
        ))
    } else {
        Ok(None)
    }
}
fn structured(key: &str) -> bool {
    matches!(
        key,
        "dependencies"
            | "proof"
            | "sources"
            | "code ref"
            | "appears in"
            | "related"
            | "tests"
            | "evidence"
            | "evidence output"
            | "last revised"
            | "claims affected"
    )
}
pub(crate) fn structured_field(field: &str) -> bool {
    structured(&fields::canonical(field)) || field == "claims_summary"
}
fn field_semantic(key: &str, value: &str) -> Value {
    if matches!(
        key,
        "dependencies"
            | "proof"
            | "sources"
            | "tags"
            | "code ref"
            | "appears in"
            | "related"
            | "authors"
            | "tests"
    ) && value.trim_start().starts_with('[')
        && let Ok(doc) = YamlDocument::parse(value)
        && let Ok(Value::Array(items)) = doc.root.to_json()
    {
        return Value::Array(items);
    }
    Value::String(value.to_owned())
}
fn atoms(path: &str, text: &str, body: Range<usize>) -> Result<Vec<Atom>, MergeError> {
    let kind = if path == "PAPER.md" {
        None
    } else if documents::allowed(path) {
        Some(fields::kind(path)?)
    } else {
        Some(fields::EntryKind::Solution)
    };
    let mut result = Vec::new();
    let mut cursor = body.start;
    let mut residual = 0;
    let mut seen = BTreeSet::new();
    let mut extensions = BTreeMap::<String, usize>::new();
    for field in index::fields(text, body.clone()) {
        // The public reader accepts prose continuations. The merge boundary is
        // stricter: unindented prose is its own opaque value, never a field edit.
        let first_end = field.range.start
            + text[field.range.clone()]
                .split_inclusive('\n')
                .next()
                .map_or(0, str::len);
        let mut end = first_end;
        for line in text[first_end..field.range.end].split_inclusive('\n') {
            if line.trim().is_empty() || line.starts_with([' ', '\t']) {
                end += line.len();
            } else {
                break;
            }
        }
        if cursor < field.range.start {
            residual_atom(cursor..field.range.start, &mut result, &mut residual);
        }
        let canonical = fields::canonical(field.name);
        let known = kind.is_some_and(|k| fields::spelling(k, field.name).is_some());
        let key = if known {
            if !seen.insert(canonical.clone()) {
                return Err(MergeError::content(
                    "merge.markdown_duplicate_field",
                    format!("Duplicate source field {} in {path}", field.name),
                ));
            }
            canonical.clone()
        } else {
            let ordinal = extensions.entry(field.name.to_owned()).or_default();
            *ordinal += 1;
            if *ordinal == 1 {
                format!("extension:{}", field.name)
            } else {
                format!("extension:{}#{ordinal}", field.name)
            }
        };
        let fragment = &text[field.range.start..end];
        let semantic = if known {
            let parsed = index::fields(fragment, 0..fragment.len());
            Some(field_semantic(
                &canonical,
                index::decode_field(&parsed[0]).as_ref(),
            ))
        } else {
            None
        };
        result.push(Atom {
            key,
            range: field.range.start..end,
            semantic,
            structured: known && structured(&canonical),
            writable: true,
            kind: AtomKind::Field,
        });
        cursor = end;
    }
    if cursor < body.end {
        residual_atom(cursor..body.end, &mut result, &mut residual);
    }
    Ok(result)
}
fn residual_atom(range: Range<usize>, result: &mut Vec<Atom>, ordinal: &mut usize) {
    result.push(Atom {
        key: format!("$body:{ordinal}"),
        range,
        semantic: None,
        structured: false,
        writable: true,
        kind: AtomKind::Residual,
    });
    *ordinal += 1;
}
impl Document {
    fn parse(path: &str, text: String) -> Result<Self, MergeError> {
        Self::parse_inner(path, text).map_err(|error| error.at(path))
    }
    fn parse_inner(path: &str, text: String) -> Result<Self, MergeError> {
        let fm = if path == "PAPER.md" {
            frontmatter(&text)?
        } else {
            None
        };
        let content_start = fm.as_ref().map_or(0, |(_, end)| *end);
        let headings = index::headings(&text)
            .into_iter()
            .filter(|h| h.range.start >= content_start)
            .collect::<Vec<_>>();
        let preamble_end = headings.first().map_or(text.len(), |h| h.range.start);
        let mut entries = vec![Entry {
            address: path.into(),
            parent: None,
            range: content_start..preamble_end,
            own: content_start..preamble_end,
            numeric: None,
            level: 0,
            literal_path: Vec::new(),
            atoms: atoms(path, &text, content_start..preamble_end)?,
        }];
        if let Some((range, _)) = fm {
            let yaml_text = &text[range.clone()];
            let yaml = YamlDocument::parse(yaml_text)?;
            let mut source_atoms = Vec::new();
            let mut seen = BTreeSet::new();
            for (key, value) in yaml.root.mapping()? {
                let key = key.scalar().ok_or_else(|| {
                    MergeError::content("merge.paper_key", "PAPER keys must be strings")
                })?;
                if !seen.insert(key.to_owned()) {
                    return Err(MergeError::content(
                        "merge.paper_duplicate",
                        format!("Duplicate PAPER field {key}"),
                    ));
                }
                let span = field_range(yaml_text, &yaml.root, key)?.ok_or_else(|| {
                    MergeError::content("merge.paper_range", "Missing frontmatter source range")
                })?;
                source_atoms.push(Atom {
                    key: key.into(),
                    range: range.start + span.start..range.start + span.end,
                    semantic: paper_key(key).then(|| value.to_json()).transpose()?,
                    structured: key == "claims_summary",
                    writable: paper_key(key),
                    kind: AtomKind::Yaml,
                });
            }
            source_atoms.sort_by_key(|a| a.range.start);
            entries.push(Entry {
                address: format!("{path}#frontmatter"),
                parent: None,
                range: range.clone(),
                own: range,
                numeric: None,
                level: 0,
                literal_path: Vec::new(),
                atoms: source_atoms,
            });
        }
        let mut displays = BTreeMap::<String, usize>::new();
        for h in &headings {
            if numeric(path, h.heading, h.level).is_some() {
                continue;
            }
            let components = h
                .path
                .iter()
                .enumerate()
                .map(|(ordinal, heading)| heading_component(path, heading, ordinal + 1))
                .collect::<Vec<_>>();
            *displays
                .entry(heading_display_address(path, &components))
                .or_default() += 1;
        }
        let mut parents: Vec<(usize, String, String)> = Vec::new();
        let mut by_heading = BTreeMap::<Vec<String>, Option<usize>>::new();
        let mut by_entry = BTreeMap::<String, Option<usize>>::new();
        for (i, h) in headings.iter().enumerate() {
            while parents
                .last()
                .is_some_and(|(level, _, _)| *level >= h.level)
            {
                parents.pop();
            }
            let n = numeric(path, h.heading, h.level);
            let mut address = if n.is_some() {
                heading_component(path, h.heading, h.level)
            } else if path == "logic/concepts.md" && h.level == 2 {
                heading_address(
                    path,
                    &h.path
                        .iter()
                        .map(|heading| (*heading).to_owned())
                        .collect::<Vec<_>>(),
                )
            } else if path == "logic/related_work.md" && h.level == 2 && rw_id(h.heading).is_some()
            {
                rw_id(h.heading).unwrap().into()
            } else {
                let mut components: Vec<String> = parents
                    .iter()
                    .map(|(_, _, component)| component.clone())
                    .collect();
                components.push(heading_component(path, h.heading, h.level));
                heading_address(path, &components)
            };
            if n.is_none()
                && displays
                    .get(&display_address(&address))
                    .is_some_and(|count| *count > 1)
            {
                let components = h
                    .path
                    .iter()
                    .enumerate()
                    .map(|(ordinal, heading)| heading_component(path, heading, ordinal + 1))
                    .collect::<Vec<_>>();
                address = exact_heading_address(path, &components);
            }
            let own_end = headings
                .get(i + 1)
                .map_or(text.len(), |next| next.range.start);
            let mut source_atoms = atoms(path, &text, h.body_range.start..own_end)?;
            let title = if n.is_some() || rw_id(h.heading).is_some() {
                identified_title(h.heading)
            } else {
                h.heading
            };
            source_atoms.insert(
                0,
                Atom {
                    key: "$title".into(),
                    range: h.range.start..h.body_range.start,
                    semantic: Some(Value::String(title.into())),
                    structured: false,
                    writable: true,
                    kind: AtomKind::Title {
                        identified: n.is_some() || rw_id(h.heading).is_some(),
                    },
                },
            );
            let parent = parents.last().map(|(_, address, _)| address.clone());
            entries.push(Entry {
                address: address.clone(),
                parent,
                range: h.range.clone(),
                own: h.range.start..own_end,
                numeric: n,
                level: h.level,
                literal_path: h.path.iter().map(|heading| (*heading).to_owned()).collect(),
                atoms: source_atoms,
            });
            let position = entries.len() - 1;
            // Shared selectors permit an unambiguous literal heading suffix.
            // Cache every bounded suffix once; '/' remains part of a heading.
            for start in 0..h.path.len() {
                let suffix = h.path[start..]
                    .iter()
                    .map(|heading| (*heading).to_owned())
                    .collect::<Vec<_>>();
                by_heading
                    .entry(suffix)
                    .and_modify(|found| *found = None)
                    .or_insert(Some(position));
            }
            let id = h
                .heading
                .split([':', ' ', '\t'])
                .next()
                .unwrap_or(h.heading);
            by_entry
                .entry(id.to_owned())
                .and_modify(|found| *found = None)
                .or_insert(Some(position));
            parents.push((
                h.level,
                address,
                heading_component(path, h.heading, h.level),
            ));
        }
        let mut by_address = BTreeMap::new();
        let mut by_start = BTreeMap::new();
        for (i, entry) in entries.iter().enumerate() {
            if by_address
                .insert(Identity::key(&entry.address), i)
                .is_some()
            {
                return Err(MergeError::content(
                    "merge.markdown_identity",
                    format!("Ambiguous native selector {}", entry.address),
                ));
            }
            by_start.insert(entry.range.start, i);
        }
        Ok(Self {
            text,
            entries,
            by_address,
            by_start,
            by_heading,
            by_entry,
        })
    }
    fn entry(&self, address: &str) -> Option<&Entry> {
        self.by_address
            .get(&Identity::key(address))
            .map(|i| &self.entries[*i])
    }
    fn raw<'a>(&'a self, atom: &Atom) -> &'a str {
        &self.text[atom.range.clone()]
    }
    fn full<'a>(&'a self, entry: &Entry) -> &'a str {
        &self.text[entry.range.clone()]
    }
    fn rewrite(
        &self,
        range: Range<usize>,
        path: &str,
        selector: &str,
        map: &IdentityMap,
        report: &mut MergeReport,
    ) -> Result<String, MergeError> {
        let mut result = String::with_capacity(range.len());
        let mut cursor = range.start;
        for (_, position) in self.by_start.range(range.clone()) {
            for atom in &self.entries[*position].atoms {
                if atom.range.start < cursor || atom.range.end > range.end {
                    continue;
                }
                result.push_str(&super::rewrite::incoming(
                    &self.text[cursor..atom.range.start],
                    path,
                    selector,
                    map,
                    report,
                    false,
                )?);
                result.push_str(&rewrite_atom(
                    self,
                    &self.entries[*position],
                    atom,
                    path,
                    map,
                    report,
                )?);
                cursor = atom.range.end;
            }
        }
        result.push_str(&super::rewrite::incoming(
            &self.text[cursor..range.end],
            path,
            selector,
            map,
            report,
            false,
        )?);
        Ok(result)
    }
}
fn rewrite_atom(
    doc: &Document,
    entry: &Entry,
    atom: &Atom,
    path: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<String, MergeError> {
    let raw = doc.raw(atom);
    if matches!(atom.kind, AtomKind::Title { .. }) && entry.numeric.is_some() {
        let target = map.source_target(&entry.address);
        if target != entry.address {
            let start = raw.bytes().take_while(|byte| *byte == b'#').count() + 1;
            if !raw[start..].starts_with(&entry.address) {
                return Err(MergeError::content(
                    "merge.markdown_heading",
                    "Numeric heading identity lost its source span",
                ));
            }
            let mut relocated =
                String::with_capacity(raw.len() - entry.address.len() + target.len());
            relocated.push_str(&raw[..start]);
            relocated.push_str(target);
            relocated.push_str(&raw[start + entry.address.len()..]);
            report.rewritten.push(RewriteFact {
                path: path.into(),
                selector: entry.address.clone(),
                old: entry.address.clone(),
                new: target.into(),
                confidence: "certain".into(),
            });
            return super::rewrite::incoming(&relocated, path, &entry.address, map, report, false);
        }
    }
    if matches!(atom.kind, AtomKind::Title { .. }) && entry.numeric.is_none() {
        let mut review = scratch(report);
        super::rewrite::incoming(raw, path, &entry.address, map, &mut review, false)?;
        report.needs_review.extend(review.needs_review);
        return Ok(raw.into());
    }
    super::rewrite::incoming(raw, path, &entry.address, map, report, atom.structured)
}
pub(crate) fn rewrite_choice(
    text: &str,
    path: &str,
    selector: &str,
    field: &str,
    map: &IdentityMap,
    report: &mut MergeReport,
) -> Result<String, MergeError> {
    if matches!(field, "$title" | "$entry" | "$document" | "file") {
        let document = Document::parse(path, text.into())?;
        document.rewrite(0..text.len(), path, selector, map, report)
    } else {
        super::rewrite::incoming(text, path, selector, map, report, structured_field(field))
    }
}
pub(crate) fn inventory(snapshot: &ArtifactSnapshot) -> Result<Inventory, MergeError> {
    let mut result = Inventory {
        entries: Vec::new(),
        paths: BTreeSet::new(),
        docs: BTreeMap::new(),
        addresses: BTreeMap::new(),
    };
    let registered = if let Some(paper) = snapshot.files.get("PAPER.md").filter(|file| file.existed)
    {
        let paper = std::str::from_utf8(&paper.bytes).map_err(|_| {
            MergeError::content("merge.markdown_encoding", "PAPER.md is not UTF-8").at("PAPER.md")
        })?;
        crate::knowledge_paths(paper)
            .map_err(|error| MergeError::content("merge.knowledge_paths", error).at("PAPER.md"))?
            .into_iter()
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };
    for (path, file) in &snapshot.files {
        if !file.existed
            || !(documents::allowed(path) || path == "PAPER.md" || registered.contains(path))
        {
            continue;
        }
        let text = std::str::from_utf8(&file.bytes).map_err(|_| {
            MergeError::content("merge.markdown_encoding", format!("{path} is not UTF-8")).at(path)
        })?;
        insert_document(&mut result, path, text)?;
    }
    Ok(result)
}
pub(crate) fn inventory_document(path: &str, text: &str) -> Result<Inventory, MergeError> {
    let mut result = Inventory {
        entries: Vec::new(),
        paths: BTreeSet::new(),
        docs: BTreeMap::new(),
        addresses: BTreeMap::new(),
    };
    insert_document(&mut result, path, text)?;
    Ok(result)
}
fn insert_document(result: &mut Inventory, path: &str, text: &str) -> Result<(), MergeError> {
    let document = Document::parse(path, text.to_owned())?;
    result.paths.insert(path.into());
    for entry in &document.entries {
        if result
            .addresses
            .insert(Identity::key(&entry.address), result.entries.len())
            .is_some()
        {
            return Err(MergeError::content(
                "merge.markdown_identity",
                format!("Duplicate identity {}", entry.address),
            )
            .at(path));
        }
        let heading = entry
            .literal_path
            .iter()
            .enumerate()
            .map(|(index, title)| {
                let numeric = numeric(path, title, index + 1)
                    .map(|_| heading_component(path, title, index + 1));
                (numeric.clone().unwrap_or_else(|| title.clone()), numeric)
            })
            .collect();
        result.entries.push(EntryIdentity {
            address: entry.address.clone(),
            layer: if path == "PAPER.md" { "paper" } else { "logic" }.into(),
            path: path.into(),
            numeric: entry.numeric,
            session: false,
            heading,
        });
    }
    result.docs.insert(path.into(), document);
    Ok(())
}
pub(crate) fn concept_address(view: &Inventory, name: &str) -> Result<Option<String>, MergeError> {
    let path = "logic/concepts.md";
    let Some(document) = view.docs.get(path) else {
        return Ok(None);
    };
    let normalized = super::identity::normalize_local(name);
    let mut found = None;
    for entry in document.entries.iter().filter(|entry| entry.level == 2) {
        let full = format!("{path}#{}", entry.literal_path.join("/"));
        if entry
            .literal_path
            .last()
            .is_some_and(|heading| heading == name)
            || entry.address == normalized
            || display_address(&entry.address) == normalized
            || full == normalized
        {
            if found.is_some() {
                return Err(MergeError::content(
                    "merge.redirect_ambiguous",
                    "concept name has multiple exact native headings",
                )
                .at(path));
            }
            found = Some(entry.address.clone());
        }
    }
    Ok(found)
}

/// Literal heading vectors of every live section of `document`, for literal
/// locator resolution (`merge::resolve_locator`).
pub(crate) fn literal_paths<'a>(view: &'a Inventory, document: &str) -> Vec<&'a [String]> {
    view.docs.get(document).map_or_else(Vec::new, |document| {
        document
            .entries
            .iter()
            .filter(|entry| !entry.literal_path.is_empty())
            .map(|entry| entry.literal_path.as_slice())
            .collect()
    })
}
/// The literal heading vector of the live entry that owns `address` in
/// `document`, if there is exactly one.
pub(crate) fn live_literal_path<'a>(
    view: &'a Inventory,
    document: &str,
    address: &str,
) -> Option<&'a [String]> {
    let mut found = view
        .docs
        .get(document)?
        .entries
        .iter()
        .filter(|entry| entry.address == address);
    let first = found.next()?;
    found
        .next()
        .is_none()
        .then_some(first.literal_path.as_slice())
}
/// Address an exact, cached source selector. Missing archived headings return
/// None; ambiguous suffixes never resolve by guessing a parent or splitting '/'.
pub(crate) fn selector_address(
    view: &Inventory,
    selector: &EntrySelector,
) -> Result<Option<String>, MergeError> {
    fn address(
        document: &Document,
        found: Option<&Option<usize>>,
        path: &str,
    ) -> Result<Option<String>, MergeError> {
        match found {
            None => Ok(None),
            Some(Some(position)) => Ok(Some(document.entries[*position].address.clone())),
            Some(None) => Err(MergeError::content(
                "merge.selector_ambiguous",
                "Native selector matches multiple literal source headings",
            )
            .at(path)),
        }
    }
    match selector {
        EntrySelector::Id { id } => {
            let path = if fields::typed_id(id, "C") {
                "logic/claims.md"
            } else if fields::typed_id(id, "H") {
                "logic/solution/heuristics.md"
            } else {
                return Err(MergeError::content(
                    "merge.selector_namespace",
                    "Bare mutable ID selectors require C/H; E and RW use explicit document selectors",
                ));
            };
            match view.docs.get(path) {
                Some(document) => address(document, document.by_entry.get(id), path),
                None => Ok(None),
            }
        }
        EntrySelector::Document {
            document: path,
            heading,
            entry,
        } => {
            crate::write::source::safe_relative(path)
                .map_err(|error| MergeError::from(error).at(path))?;
            if !path.ends_with(".md")
                || crate::file_access_path(path)
                || ["trace", "staging"].contains(&path.split('/').next().unwrap_or(""))
                || (heading.is_empty() && entry.is_none())
                || (!heading.is_empty() && entry.is_some())
            {
                return Err(MergeError::content("merge.selector", "Native document selectors require exactly one literal heading path or entry in mutable knowledge").at(path));
            }
            let Some(document) = view.docs.get(path) else {
                return Ok(None);
            };
            if let Some(entry) = entry {
                address(document, document.by_entry.get(entry), path)
            } else {
                address(document, document.by_heading.get(heading), path)
            }
        }
    }
}

/// Recover the exact current writer selector from a canonical native identity.
/// Literal heading vectors come from the captured parse, never address splitting.
pub(crate) fn selector_for_address(
    view: &Inventory,
    address: &str,
) -> Result<Option<EntrySelector>, MergeError> {
    let suffixes = (!view.addresses.contains_key(&Identity::key(address)))
        .then(|| super::identity::heading_suffixes(view));
    let exact = if let Some(suffixes) = &suffixes {
        match suffixes.get(address) {
            Some(Some(target)) => target,
            Some(None) => {
                return Err(MergeError::content(
                    "merge.selector_ambiguous",
                    "Native display locator has multiple literal heading vectors",
                ));
            }
            None => return Ok(None),
        }
    } else {
        address
    };
    let Some(position) = view.addresses.get(&Identity::key(exact)) else {
        return Ok(None);
    };
    let identity = view.entries.get(*position).ok_or_else(|| {
        MergeError::content(
            "merge.selector_index",
            "Native identity index is inconsistent",
        )
    })?;
    let document = view.docs.get(&identity.path).ok_or_else(|| {
        MergeError::content(
            "merge.selector_index",
            "Native document index is inconsistent",
        )
        .at(&identity.path)
    })?;
    let position = document
        .by_address
        .get(&Identity::key(exact))
        .ok_or_else(|| {
            MergeError::content("merge.selector_index", "Native entry index is inconsistent")
                .at(&identity.path)
        })?;
    let entry = &document.entries[*position];
    if entry.literal_path.is_empty() {
        return Ok(None);
    }
    let heading = entry.literal_path.last().expect("nonempty literal path");
    let id = heading.split([':', ' ', '\t']).next().unwrap_or(heading);
    let bare = matches!(entry.numeric, Some('C' | 'H'));
    if bare && document.by_entry.get(id) == Some(&Some(*position)) {
        return Ok(Some(EntrySelector::Id { id: id.into() }));
    }
    Ok(Some(EntrySelector::Document {
        document: identity.path.clone(),
        heading: entry.literal_path.clone(),
        entry: None,
    }))
}
fn scratch(report: &MergeReport) -> MergeReport {
    MergeReport::new(
        &MergeOptions {
            source_key: report.source_key.clone(),
            label: String::new(),
            time: String::new(),
            git: None,
            predecessor: None,
        },
        report.source_revision.clone(),
    )
}
fn semantic(atom: &Atom, raw: &str) -> Result<Option<Value>, MergeError> {
    if atom.semantic.is_none() {
        return Ok(None);
    }
    Ok(Some(match atom.kind {
        AtomKind::Title { identified } => {
            let heading = raw.trim().trim_start_matches('#').trim();
            let title = if identified {
                identified_title(heading)
            } else {
                heading
            };
            Value::String(title.into())
        }
        AtomKind::Field => {
            let parsed = index::fields(raw, 0..raw.len());
            let field = parsed.first().ok_or_else(|| {
                MergeError::content("merge.markdown_field", "Incoming field lost its label")
            })?;
            field_semantic(&atom.key, index::decode_field(field).as_ref())
        }
        AtomKind::Yaml => {
            let yaml = YamlDocument::parse(raw)?;
            yaml.root
                .get(&atom.key)?
                .ok_or_else(|| {
                    MergeError::content("merge.paper_field", "Incoming metadata lost its key")
                })?
                .to_json()?
        }
        AtomKind::Residual => unreachable!(),
    }))
}
fn incoming_equal(
    a: Option<(&Document, &Atom)>,
    b: Option<(&Document, &Atom)>,
    mapped: Option<&str>,
    map: &IdentityMap,
) -> Result<bool, MergeError> {
    match (a, b) {
        (None, None) => Ok(true),
        (Some((a_doc, a)), Some((_, b))) => {
            let raw = mapped.expect("present incoming atom");
            let value = semantic(b, raw)?;
            Ok(match (&a.semantic, value) {
                (Some(left), Some(right)) if a.structured && b.structured => {
                    super::rewrite::references_equal(left, &right, map)
                }
                (Some(left), Some(right)) => *left == right,
                _ => a_doc.raw(a) == raw,
            })
        }
        _ => Ok(false),
    }
}
fn mapped_equal(
    a: Option<(&Document, &Atom)>,
    a_raw: Option<&str>,
    b: Option<(&Document, &Atom)>,
    b_raw: Option<&str>,
    map: &IdentityMap,
) -> Result<bool, MergeError> {
    match (a, b) {
        (None, None) => Ok(true),
        (Some((_, a)), Some((_, b))) => {
            let a_raw = a_raw.expect("present ancestor atom");
            let b_raw = b_raw.expect("present incoming atom");
            Ok(match (semantic(a, a_raw)?, semantic(b, b_raw)?) {
                (Some(left), Some(right)) if a.structured && b.structured => {
                    super::rewrite::references_equal(&left, &right, map)
                }
                (Some(left), Some(right)) => left == right,
                _ => a_raw == b_raw,
            })
        }
        _ => Ok(false),
    }
}
fn atom_bytes<'a>(value: Option<(&'a Document, &Atom)>) -> Option<&'a [u8]> {
    value.map(|(doc, atom)| doc.raw(atom).as_bytes())
}
fn locator(entry: &str, field: &str) -> ConflictLocator {
    ConflictLocator::Markdown {
        entry: entry.into(),
        field: field.into(),
    }
}
#[allow(clippy::too_many_arguments)]
fn merge_entry(
    path: &str,
    base: Option<(&Document, &Entry)>,
    ours: (&Document, &Entry),
    theirs: (&Document, &Entry),
    map: &IdentityMap,
    report: &mut MergeReport,
    comparison: &mut MergeReport,
    patches: &mut Vec<Patch>,
) -> Result<(), MergeError> {
    let (ours_doc, ours_entry) = ours;
    let (theirs_doc, theirs_entry) = theirs;
    let base_atoms = base
        .map(|(_, entry)| {
            entry
                .atoms
                .iter()
                .map(|a| (a.key.as_str(), a))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let ours_atoms = ours_entry
        .atoms
        .iter()
        .map(|a| (a.key.as_str(), a))
        .collect::<BTreeMap<_, _>>();
    let theirs_atoms = theirs_entry
        .atoms
        .iter()
        .map(|a| (a.key.as_str(), a))
        .collect::<BTreeMap<_, _>>();
    let keys = base_atoms
        .keys()
        .chain(ours_atoms.keys())
        .chain(theirs_atoms.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut additions = Vec::new();
    for key in keys {
        let b = base.and_then(|(doc, _)| base_atoms.get(key).map(|atom| (doc, *atom)));
        let o = ours_atoms.get(key).map(|atom| (ours_doc, *atom));
        let t = theirs_atoms.get(key).map(|atom| (theirs_doc, *atom));
        comparison.rewritten.clear();
        comparison.needs_review.clear();
        let mapped = t
            .map(|(doc, atom)| rewrite_atom(doc, theirs_entry, atom, path, map, comparison))
            .transpose()?;
        let mapped_base = b
            .map(|(doc, atom)| {
                rewrite_atom(
                    doc,
                    base.expect("present ancestor").1,
                    atom,
                    path,
                    map,
                    comparison,
                )
            })
            .transpose()?;
        if incoming_equal(o, t, mapped.as_deref(), map)?
            || mapped_equal(b, mapped_base.as_deref(), t, mapped.as_deref(), map)?
        {
            continue;
        }
        let writable = t.or(b).is_none_or(|(_, atom)| atom.writable);
        if incoming_equal(o, b, mapped_base.as_deref(), map)? && writable {
            let replacement = if let Some((doc, atom)) = t {
                rewrite_atom(doc, theirs_entry, atom, path, map, report)?
            } else {
                String::new()
            };
            if let Some((_, atom)) = o {
                patches.push(Patch {
                    range: atom.range.clone(),
                    text: replacement,
                });
            } else if !replacement.is_empty() {
                let atom = t.expect("present added field").1;
                let at = theirs_entry
                    .atoms
                    .iter()
                    .filter(|next| next.range.start >= atom.range.end)
                    .find_map(|next| {
                        ours_atoms
                            .get(next.key.as_str())
                            .map(|ours| ours.range.start)
                    })
                    .unwrap_or(ours_entry.own.end);
                let prefix = if at > 0 && !ours_doc.text[..at].ends_with('\n') {
                    "\n"
                } else {
                    ""
                };
                let suffix = if at < ours_doc.text.len() && !replacement.ends_with('\n') {
                    "\n"
                } else {
                    ""
                };
                additions.push((
                    atom.range.start,
                    Patch {
                        range: at..at,
                        text: format!("{prefix}{replacement}{suffix}"),
                    },
                ));
            }
        } else {
            conflict(
                report,
                path,
                &ours_entry.address,
                key,
                if writable {
                    "mutable_field"
                } else {
                    "unsupported_paper_field"
                },
                atom_bytes(b),
                atom_bytes(o),
                atom_bytes(t),
                locator(&ours_entry.address, key),
            );
        }
    }
    additions.sort_by_key(|(source_start, _)| *source_start);
    patches.extend(additions.into_iter().map(|(_, patch)| patch));
    Ok(())
}
fn entry_conflict(
    path: &str,
    address: &str,
    kind: &str,
    base: Option<(&Document, &Entry)>,
    ours: Option<(&Document, &Entry)>,
    theirs: Option<(&Document, &Entry)>,
    report: &mut MergeReport,
) {
    conflict(
        report,
        path,
        address,
        "$entry",
        kind,
        base.map(|(doc, entry)| doc.full(entry).as_bytes()),
        ours.map(|(doc, entry)| doc.full(entry).as_bytes()),
        theirs.map(|(doc, entry)| doc.full(entry).as_bytes()),
        locator(address, "$entry"),
    );
}
fn stage(
    path: &str,
    original: &str,
    mut patches: Vec<Patch>,
    working: &mut WorkingArtifact,
) -> Result<(), MergeError> {
    if patches.is_empty() {
        return Ok(());
    }
    patches.sort_by_key(|patch| patch.range.start);
    let mut cursor = 0;
    let mut candidate = String::with_capacity(original.len());
    for patch in patches {
        if patch.range.start < cursor
            || patch.range.start > patch.range.end
            || patch.range.end > original.len()
            || !original.is_char_boundary(patch.range.start)
            || !original.is_char_boundary(patch.range.end)
        {
            return Err(MergeError::content(
                "merge.markdown_overlap",
                "Overlapping or invalid Markdown source spans",
            ));
        }
        candidate.push_str(&original[cursor..patch.range.start]);
        candidate.push_str(&patch.text);
        cursor = patch.range.end;
    }
    candidate.push_str(&original[cursor..]);
    Document::parse(path, candidate.clone())?;
    if candidate != original {
        working.stage_replace(
            path,
            candidate.as_bytes(),
            "merge exact Markdown span composition",
        )?;
    }
    Ok(())
}
pub(crate) fn apply(
    base: &Inventory,
    ours: &Inventory,
    theirs: &Inventory,
    map: &IdentityMap,
    working: &mut WorkingArtifact,
    report: &mut MergeReport,
) -> Result<(), MergeError> {
    let mut paths = base
        .paths
        .iter()
        .chain(&ours.paths)
        .chain(&theirs.paths)
        .cloned()
        .collect::<BTreeSet<_>>();
    let paper = paths.take("PAPER.md");
    let mut comparison = scratch(report);
    for path in paper.into_iter().chain(paths) {
        let b = base.docs.get(&path);
        let o = ours.docs.get(&path);
        let t = theirs.docs.get(&path);
        if !documents::allowed(&path)
            && path != "PAPER.md"
            && !working.is_allowed_document(&path)?
        {
            if t.map(|d| d.text.as_str()) != b.map(|d| d.text.as_str()) {
                conflict(
                    report,
                    &path,
                    &path,
                    "$document",
                    "unregistered_knowledge_path",
                    b.map(|d| d.text.as_bytes()),
                    o.map(|d| d.text.as_bytes()),
                    t.map(|d| d.text.as_bytes()),
                    ConflictLocator::Document,
                );
            }
            continue;
        }
        if let Some(base_doc) = b
            && (o.is_none() || t.is_none())
        {
            let unchanged = if let Some(doc) = o {
                comparison.rewritten.clear();
                comparison.needs_review.clear();
                doc.text
                    == base_doc.rewrite(
                        0..base_doc.text.len(),
                        &path,
                        &path,
                        map,
                        &mut comparison,
                    )?
            } else {
                t.is_none_or(|doc| doc.text == base_doc.text)
            };
            if unchanged && path != "PAPER.md" {
                if o.is_some() {
                    working.delete(
                        &path,
                        "merge agreed or unchanged-peer mutable document deletion",
                    )?;
                }
            } else {
                conflict(
                    report,
                    &path,
                    &path,
                    "$document",
                    if unchanged {
                        "unsupported_document_deletion"
                    } else {
                        "delete_edit"
                    },
                    b.map(|d| d.text.as_bytes()),
                    o.map(|d| d.text.as_bytes()),
                    t.map(|d| d.text.as_bytes()),
                    ConflictLocator::Document,
                );
            }
            continue;
        }
        let Some(theirs_doc) = t else {
            continue;
        };
        let Some(ours_doc) = o else {
            if path == "PAPER.md"
                && theirs_doc
                    .entries
                    .iter()
                    .flat_map(|e| &e.atoms)
                    .any(|atom| !atom.writable)
            {
                conflict(
                    report,
                    &path,
                    &path,
                    "$document",
                    "unsupported_paper_field",
                    None,
                    None,
                    Some(theirs_doc.text.as_bytes()),
                    ConflictLocator::Document,
                );
                continue;
            }
            let imported =
                theirs_doc.rewrite(0..theirs_doc.text.len(), &path, &path, map, report)?;
            Document::parse(&path, imported.clone())?;
            working.stage_create(&path, imported.as_bytes())?;
            continue;
        };
        let recipe = (path.starts_with("logic/solution/")
            && path != "logic/solution/heuristics.md")
            || (!documents::allowed(&path) && path != "PAPER.md");
        if b.is_none() && recipe && ours_doc.text != theirs_doc.text {
            conflict(
                report,
                &path,
                &path,
                "$document",
                "identity",
                None,
                Some(ours_doc.text.as_bytes()),
                Some(theirs_doc.text.as_bytes()),
                ConflictLocator::Document,
            );
            continue;
        }
        let mut patches = Vec::new();
        let mut skipped_base: Option<Range<usize>> = None;
        if let Some(base_doc) = b {
            for entry in &base_doc.entries {
                if skipped_base
                    .as_ref()
                    .is_some_and(|r| r.start <= entry.range.start && entry.range.end <= r.end)
                {
                    continue;
                }
                let target = map.source_target(&entry.address);
                let our_entry = ours_doc.entry(target);
                let their_entry = theirs_doc.entry(&entry.address);
                let mapped_base = if our_entry.is_some() && their_entry.is_none() {
                    comparison.rewritten.clear();
                    comparison.needs_review.clear();
                    Some(base_doc.rewrite(
                        entry.range.clone(),
                        &path,
                        &entry.address,
                        map,
                        &mut comparison,
                    )?)
                } else {
                    None
                };
                match (our_entry, their_entry) {
                    (Some(our_entry), Some(their_entry)) => merge_entry(
                        &path,
                        Some((base_doc, entry)),
                        (ours_doc, our_entry),
                        (theirs_doc, their_entry),
                        map,
                        report,
                        &mut comparison,
                        &mut patches,
                    )?,
                    (None, None) => {
                        skipped_base = Some(entry.range.clone());
                    }
                    (Some(our_entry), None)
                        if Some(ours_doc.full(our_entry)) == mapped_base.as_deref() =>
                    {
                        patches.push(Patch {
                            range: our_entry.range.clone(),
                            text: String::new(),
                        });
                        skipped_base = Some(entry.range.clone());
                    }
                    (None, Some(their_entry))
                        if theirs_doc.full(their_entry) == base_doc.full(entry) =>
                    {
                        skipped_base = Some(entry.range.clone());
                    }
                    (o, t) => {
                        entry_conflict(
                            &path,
                            target,
                            "delete_edit",
                            Some((base_doc, entry)),
                            o.map(|e| (ours_doc, e)),
                            t.map(|e| (theirs_doc, e)),
                            report,
                        );
                        skipped_base = Some(entry.range.clone());
                    }
                }
            }
        }
        let mut imported_range: Option<Range<usize>> = None;
        for entry in &theirs_doc.entries {
            if b.is_some_and(|doc| doc.entry(&entry.address).is_some())
                || imported_range
                    .as_ref()
                    .is_some_and(|r| r.start <= entry.range.start && entry.range.end <= r.end)
            {
                continue;
            }
            if entry.parent.as_ref().is_some_and(|parent| {
                b.is_some_and(|doc| doc.entry(parent).is_some())
                    && ours_doc.entry(map.source_target(parent)).is_none()
            }) {
                continue;
            }
            let target = map.source_target(&entry.address);
            if let Some(our_entry) = ours_doc.entry(target) {
                if entry.numeric.is_some() && !map.contains_key(&entry.address) {
                    return Err(MergeError::content(
                        "merge.markdown_mapping",
                        format!(
                            "Source-only numeric identity {} has no import mapping",
                            entry.address
                        ),
                    ));
                }
                if entry.address == path
                    || entry.address.ends_with("#frontmatter")
                    || entry.level == 1
                {
                    merge_entry(
                        &path,
                        None,
                        (ours_doc, our_entry),
                        (theirs_doc, entry),
                        map,
                        report,
                        &mut comparison,
                        &mut patches,
                    )?;
                } else if ours_doc.full(our_entry) != theirs_doc.full(entry) {
                    entry_conflict(
                        &path,
                        target,
                        "identity",
                        None,
                        Some((ours_doc, our_entry)),
                        Some((theirs_doc, entry)),
                        report,
                    );
                    imported_range = Some(entry.range.clone());
                }
                continue;
            }
            if entry.address == path || entry.address.ends_with("#frontmatter") {
                if entry.atoms.iter().any(|atom| !atom.writable) {
                    entry_conflict(
                        &path,
                        target,
                        "unsupported_paper_field",
                        None,
                        None,
                        Some((theirs_doc, entry)),
                        report,
                    );
                    continue;
                }
                // A new frontmatter block is a bounded insertion, including its delimiters.
                let range = if entry.address.ends_with("#frontmatter") {
                    let (_, end) = frontmatter(&theirs_doc.text)?.unwrap();
                    0..end
                } else {
                    entry.range.clone()
                };
                let value = theirs_doc.rewrite(range, &path, &entry.address, map, report)?;
                let at = if entry.address.ends_with("#frontmatter") {
                    0
                } else {
                    ours_doc.entries[0].range.start
                };
                patches.push(Patch {
                    range: at..at,
                    text: value,
                });
                continue;
            }
            if entry.numeric.is_some() && !map.contains_key(&entry.address) {
                return Err(MergeError::content(
                    "merge.markdown_mapping",
                    format!("Missing import mapping for {}", entry.address),
                ));
            }
            let imported =
                theirs_doc.rewrite(entry.range.clone(), &path, &entry.address, map, report)?;
            let at = entry
                .parent
                .as_ref()
                .and_then(|parent| ours_doc.entry(map.source_target(parent)))
                .map_or(ours_doc.text.len(), |parent| parent.range.end);
            let separator = if at == 0 || ours_doc.text[..at].ends_with('\n') {
                ""
            } else {
                "\n"
            };
            let suffix = if at < ours_doc.text.len() && !imported.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            patches.push(Patch {
                range: at..at,
                text: format!("{separator}{imported}{suffix}"),
            });
            imported_range = Some(entry.range.clone());
        }
        stage(&path, &ours_doc.text, patches, working)?;
    }
    Ok(())
}
pub(crate) fn resolve(
    working: &mut WorkingArtifact,
    conflict: &MergeConflict,
    value: &MergeValue,
) -> Result<(), MergeError> {
    if !working.is_allowed_document(&conflict.path)? || conflict.allowed.is_empty() {
        return Err(MergeError::content(
            "merge.protected_resolution",
            "Resolution cannot mutate protected files",
        ));
    }
    let ConflictLocator::Markdown { entry, field } = &conflict.locator else {
        return Err(MergeError::content(
            "merge.markdown_locator",
            "Expected a Markdown locator",
        ));
    };
    let original = working.text(&conflict.path)?.to_owned();
    let document = Document::parse(&conflict.path, original.clone())?;
    let current_entry = document.entry(entry);
    let current_atom =
        current_entry.and_then(|entry| entry.atoms.iter().find(|atom| &atom.key == field));
    let current = if field == "$entry" {
        current_entry.map(|entry| document.full(entry).as_bytes())
    } else {
        current_atom.map(|atom| document.raw(atom).as_bytes())
    };
    if MergeValue::new(current).fingerprint != conflict.ours.fingerprint {
        return Err(MergeError::content(
            "merge.stale_conflict",
            "Markdown conflict value changed after the conflict was recorded",
        ));
    }
    if conflict.kind == "unsupported_paper_field" || current_atom.is_some_and(|atom| !atom.writable)
    {
        let current = MergeValue::new(current);
        if current.present == value.present && current.bytes == value.bytes {
            return Ok(());
        }
        return Err(MergeError::content(
            "merge.unsupported_resolution",
            "PAPER extension metadata has no approved write adapter",
        ));
    }
    let text = if value.present {
        std::str::from_utf8(&value.bytes).map_err(|_| {
            MergeError::content("merge.markdown_encoding", "Resolution is not UTF-8")
        })?
    } else {
        ""
    };
    let range = if field == "$entry" {
        current_entry
            .map(|entry| entry.range.clone())
            .unwrap_or(original.len()..original.len())
    } else if let Some(atom) = current_atom {
        atom.range.clone()
    } else {
        let entry = current_entry.ok_or_else(|| {
            MergeError::content(
                "merge.markdown_locator",
                "The field's owning entry is absent",
            )
        })?;
        entry.own.end..entry.own.end
    };
    let prefix = if range.is_empty() && range.start > 0 && !original[..range.start].ends_with('\n')
    {
        "\n"
    } else {
        ""
    };
    let suffix = if range.end < original.len() && !text.is_empty() && !text.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    stage(
        &conflict.path,
        &original,
        vec![Patch {
            range,
            text: format!("{prefix}{text}{suffix}"),
        }],
        working,
    )
}

/// Scalar reference consumers use the same exact vector inventory as typed
/// selectors. Ambiguous displays have no entry; this index is never persisted.
pub(crate) fn reference_targets(
    view: &Inventory,
    redirects: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut targets = redirects.clone();
    for (display, exact) in super::identity::heading_suffixes(view) {
        match exact {
            Some(exact) if display != exact => {
                targets.entry(display).or_insert(exact);
            }
            None => {
                targets.remove(&display);
            }
            _ => {}
        }
    }
    targets
}

/// Validate only approved structured fields, not possible IDs in residual prose.
/// Redirects permit historical spellings while deletions cannot leave new links
/// without a surviving native destination.
pub(crate) fn validate_references(
    view: &Inventory,
    identities: &BTreeSet<String>,
    redirects: &BTreeMap<String, String>,
) -> Result<(), MergeError> {
    fn resolves(
        address: &str,
        identities: &BTreeSet<String>,
        redirects: &BTreeMap<String, String>,
    ) -> bool {
        if identities.contains(address) {
            return true;
        }
        let mut current = address;
        let mut seen = BTreeSet::new();
        while let Some(target) = redirects.get(current) {
            if !seen.insert(current) {
                return false;
            }
            if identities.contains(target) {
                return true;
            }
            current = target;
        }
        false
    }
    let references = reference_targets(view, redirects);
    let redirects = &references;
    for (path, document) in &view.docs {
        for entry in &document.entries {
            for atom in &entry.atoms {
                if !atom.structured {
                    continue;
                }
                let raw = document.raw(atom);
                for token in crate::query::scan_tokens(raw) {
                    // O1 in a problem framing is not staging observation O01.
                    if path == "logic/problem.md" && token.literal.starts_with('O') {
                        continue;
                    }
                    if !resolves(token.literal, identities, redirects) {
                        return Err(MergeError::content(
                            "merge.dangling_reference",
                            format!(
                                "{path}:{}:{} references absent {}",
                                entry.address, atom.key, token.literal
                            ),
                        ));
                    }
                }
                for token in raw.split(|c: char| {
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
                }) {
                    let token = token.trim_end_matches(['.', ';']);
                    if let Some(id) = token.strip_prefix("trace:") {
                        if !resolves(id, identities, redirects) {
                            return Err(MergeError::content(
                                "merge.dangling_reference",
                                format!(
                                    "{path}:{}:{} references absent {token}",
                                    entry.address, atom.key
                                ),
                            ));
                        }
                        continue;
                    }
                    let parts = token.split_once('#').or_else(|| token.split_once(':'));
                    let prefix = parts.map_or(token, |(doc, _)| doc);
                    let local = !token.contains("://")
                        && ([
                            "logic/",
                            "rubric/",
                            "trace/",
                            "staging/",
                            "src/",
                            "evidence/",
                        ]
                        .iter()
                        .any(|root| prefix.starts_with(*root))
                            || prefix == "PAPER.md"
                            || prefix.ends_with(".md")
                            || (prefix.contains('/') && identities.contains(prefix)));
                    if local {
                        let direct = resolves(token, identities, redirects);
                        let qualified = parts.is_some_and(|(doc, entry)| {
                            let external_location =
                                crate::file_access_path(doc) && !entry.is_empty();
                            resolves(doc, identities, redirects)
                                && (external_location
                                    || resolves(entry, identities, redirects)
                                    || resolves(&format!("{doc}#{entry}"), identities, redirects))
                        });
                        if !direct && !qualified {
                            return Err(MergeError::content(
                                "merge.dangling_reference",
                                format!(
                                    "{path}:{}:{} references absent {token}",
                                    entry.address, atom.key
                                ),
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod selector_tests {
    use super::*;
    use crate::write::source::{FileSnapshot, digest};
    fn view(path: &str, text: &str) -> Inventory {
        inventory(&ArtifactSnapshot {
            root: "/pure-selector-fixture".into(),
            files: BTreeMap::from([(
                path.into(),
                FileSnapshot {
                    bytes: text.as_bytes().to_vec(),
                    existed: true,
                    permissions: None,
                    digest: digest(text.as_bytes()),
                },
            )]),
            identity_paths: BTreeSet::new(),
        })
        .unwrap()
    }
    #[test]
    fn inverse_native_address_preserves_literal_slash_not_hierarchy() {
        let path = "logic/solution/method.md";
        let view = view(path, "# Method\n\n## Target/with slash\nComplete body.\n");
        let actual =
            selector_for_address(&view, "logic/solution/method.md#Method/Target/with slash")
                .unwrap();
        assert_eq!(
            actual,
            Some(EntrySelector::Document {
                document: path.into(),
                heading: vec!["Method".into(), "Target/with slash".into()],
                entry: None
            })
        );
        let split = EntrySelector::Document {
            document: path.into(),
            heading: vec!["Method".into(), "Target".into(), "with slash".into()],
            entry: None,
        };
        assert_eq!(selector_address(&view, &split).unwrap(), None);
    }
    #[test]
    fn inverse_experiment_address_remains_document_qualified() {
        let path = "logic/experiments.md";
        let view = view(
            path,
            "# Experiments\n\n## E05: Evaluate mechanism\n- **Question**: measurable question\n",
        );
        assert_eq!(
            selector_for_address(&view, "E05").unwrap(),
            Some(EntrySelector::Document {
                document: path.into(),
                heading: vec!["Experiments".into(), "E05: Evaluate mechanism".into()],
                entry: None
            })
        );
    }
    #[test]
    fn ambiguous_heading_suffix_requires_exact_inverse_parent_path() {
        let path = "logic/solution/method.md";
        let view = view(
            path,
            "# Method\n\n## First\n\n### Details\nLeft body.\n\n## Second\n\n### Details\nRight body.\n",
        );
        let short = EntrySelector::Document {
            document: path.into(),
            heading: vec!["Details".into()],
            entry: None,
        };
        assert_eq!(
            selector_address(&view, &short).unwrap_err().code,
            "merge.selector_ambiguous"
        );
        assert_eq!(
            selector_for_address(&view, "logic/solution/method.md#Method/Second/Details").unwrap(),
            Some(EntrySelector::Document {
                document: path.into(),
                heading: vec!["Method".into(), "Second".into(), "Details".into()],
                entry: None
            })
        );
    }
}

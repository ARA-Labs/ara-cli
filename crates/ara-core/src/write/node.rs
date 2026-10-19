//! Source-indexed, additive exploration-tree authoring.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::positions::{PathPart, YamlDocument, YamlKind, YamlNode};
use super::{Fields, OperationResult, WorkingArtifact, WriteError, WriteOperation};

const TREE: &str = "trace/exploration_tree.yaml";
const ANNOTATION_INTENT: &str = "append immutable node conflict annotation:";

#[derive(serde::Serialize)]
struct AnnotationPayload<'a> {
    kind: &'a str,
    references: &'a [String],
    comment: &'a str,
}

#[derive(serde::Deserialize)]
struct AnnotationReferences {
    references: Vec<String>,
}

#[derive(Debug)]
pub(super) struct CachedNodeIndex {
    document: std::sync::Arc<YamlDocument>,
    index: std::sync::Arc<NodeIndex>,
}

#[derive(Debug)]
struct NodeRecord {
    id: String,
    selector: usize,
    kind: Option<String>,
    parent: Option<String>,
    dependencies: Vec<String>,
    same_as: Vec<String>,
    timestamp: Option<String>,
    concepts: Vec<String>,
}

#[derive(Debug)]
enum NodeLocation {
    Root {
        field: &'static str,
        index: Option<usize>,
    },
    Child {
        parent: usize,
        index: usize,
    },
}
#[derive(Debug, Default)]
pub(super) struct NodeIndex {
    nodes: BTreeMap<String, NodeRecord>,
    children: BTreeMap<String, Vec<String>>,
    locations: Vec<NodeLocation>,
}

fn invalid(message: impl Into<String>) -> WriteError {
    WriteError::semantic("write.node", message)
}

fn null(node: &YamlNode) -> bool {
    matches!(&node.kind, YamlKind::Scalar { value, plain: true } if matches!(value.as_str(), "" | "null" | "Null" | "NULL" | "~"))
}

fn scalar_field(node: &YamlNode, key: &str) -> Result<Option<String>, WriteError> {
    node.get(key)?
        .map(|value| {
            value.editable()?;
            value
                .scalar()
                .map(str::to_owned)
                .ok_or_else(|| invalid(format!("node field `{key}` must be a scalar")).at(key))
        })
        .transpose()
}

fn references(node: &YamlNode, key: &str) -> Result<Vec<String>, WriteError> {
    let Some(value) = node.get(key)? else {
        return Ok(Vec::new());
    };
    value.editable()?;
    if null(value) {
        return Ok(Vec::new());
    }
    value
        .sequence()?
        .iter()
        .map(|item| {
            item.editable()?;
            item.scalar()
                .map(str::to_owned)
                .ok_or_else(|| invalid(format!("`{key}` entries must be node ID strings")).at(key))
        })
        .collect()
}

fn walk(
    node: &YamlNode,
    location: NodeLocation,
    nodes: &mut BTreeMap<String, NodeRecord>,
    locations: &mut Vec<NodeLocation>,
) -> Result<(), WriteError> {
    let slot = locations.len();
    locations.push(location);
    let mut pending = vec![(node, slot, None::<String>)];
    while let Some((node, selector, parent)) = pending.pop() {
        node.mapping()?;
        let id =
            scalar_field(node, "id")?.ok_or_else(|| invalid("a source node has no id").at("id"))?;
        check_id(&id, "id")?;
        if nodes.contains_key(&id) {
            return Err(invalid(format!("duplicate source node ID `{id}`")).at("id"));
        }
        let explicit_parent = scalar_field(node, "parent")?;
        if parent
            .as_deref()
            .zip(explicit_parent.as_deref())
            .is_some_and(|(physical, explicit)| physical != explicit)
        {
            return Err(invalid(format!(
                "node `{id}` explicit parent disagrees with its physical nesting"
            ))
            .at("parent"));
        }
        let record = NodeRecord {
            id: id.clone(),
            selector,
            kind: scalar_field(node, "type")?,
            parent: parent.or(explicit_parent),
            dependencies: references(node, "also_depends_on")?,
            same_as: references(node, "same_as")?,
            timestamp: scalar_field(node, "timestamp")?,
            concepts: references(node, "concepts")?,
        };
        nodes.insert(id.clone(), record);
        if let Some(children) = node.get("children")?
            && !null(children)
        {
            for (index, child) in children.sequence()?.iter().enumerate().rev() {
                let slot = locations.len();
                locations.push(NodeLocation::Child {
                    parent: selector,
                    index,
                });
                pending.push((child, slot, Some(id.clone())));
            }
        }
    }
    Ok(())
}

fn index(document: &YamlDocument) -> Result<NodeIndex, WriteError> {
    let root = &document.root;
    root.mapping()?;
    let tree = root.get("tree")?;
    let single = root.get("root")?;
    if tree.is_some() && single.is_some() {
        return Err(invalid(
            "ambiguous exploration tree: both `tree` and `root` are present",
        ));
    }
    let mut nodes = BTreeMap::new();
    let mut locations = Vec::new();
    if let Some(tree) = tree {
        if !null(tree) {
            for (index, node) in tree.sequence()?.iter().enumerate() {
                walk(
                    node,
                    NodeLocation::Root {
                        field: "tree",
                        index: Some(index),
                    },
                    &mut nodes,
                    &mut locations,
                )?;
            }
        }
    } else if let Some(single) = single
        && !null(single)
    {
        walk(
            single,
            NodeLocation::Root {
                field: "root",
                index: None,
            },
            &mut nodes,
            &mut locations,
        )?;
    }
    // Numeric identity must be unambiguous even for an explicit replay ID.
    let mut numeric = BTreeMap::new();
    for id in nodes.keys() {
        let number = id[1..]
            .parse::<u64>()
            .map_err(|_| invalid(format!("node ID suffix overflows: `{id}`")).at("id"))?;
        if let Some(previous) = numeric.insert(number, id) {
            return Err(invalid(format!(
                "ambiguous numeric-equivalent node IDs `{previous}` and `{id}`"
            ))
            .at("id"));
        }
    }
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for node in nodes.values() {
        if let Some(parent) = &node.parent {
            check_id(parent, "parent")?;
            if !nodes.contains_key(parent) {
                return Err(invalid(format!(
                    "node `{}` references unknown parent `{parent}`",
                    node.id
                ))
                .at("parent"));
            }
            children
                .entry(parent.clone())
                .or_default()
                .push(node.id.clone());
        }
    }
    // A root-level parent pointer contributes a real Child edge. Validate its
    // ancestry before any ancestor walk, including source-authored cycles.
    let mut colors: BTreeMap<&str, u8> = BTreeMap::new();
    let mut chain = Vec::new();
    for node in nodes.values() {
        let mut current = node.id.as_str();
        loop {
            match colors.get(current) {
                Some(1) => {
                    return Err(
                        invalid(format!("explicit parent cycle reaches `{current}`")).at("parent"),
                    );
                }
                Some(2) => break,
                _ => {}
            }
            colors.insert(current, 1);
            chain.push(current);
            let Some(parent) = nodes.get(current).and_then(|node| node.parent.as_deref()) else {
                break;
            };
            current = parent;
        }
        for id in chain.drain(..) {
            colors.insert(id, 2);
        }
    }
    Ok(NodeIndex {
        nodes,
        children,
        locations,
    })
}

pub(super) fn cached_node_index(
    working: &WorkingArtifact,
) -> Result<std::sync::Arc<NodeIndex>, WriteError> {
    if !working.exists(TREE) {
        return Ok(std::sync::Arc::new(NodeIndex::default()));
    }
    let document = working.yaml(TREE)?;
    if let Some(cached) = working.node_index_cache.borrow().as_ref()
        && std::sync::Arc::ptr_eq(&cached.document, &document)
    {
        return Ok(std::sync::Arc::clone(&cached.index));
    }
    let index = std::sync::Arc::new(index(&document)?);
    working.node_index_cache.replace(Some(CachedNodeIndex {
        document,
        index: std::sync::Arc::clone(&index),
    }));
    Ok(index)
}
pub fn node_ids(working: &WorkingArtifact) -> Result<Vec<String>, WriteError> {
    Ok(cached_node_index(working)?.nodes.keys().cloned().collect())
}
pub fn node_kind(working: &WorkingArtifact, id: &str) -> Result<Option<String>, WriteError> {
    Ok(cached_node_index(working)?.kind(id).map(str::to_owned))
}

fn check_id(id: &str, field: &str) -> Result<(), WriteError> {
    if id.len() < 2 || !id.starts_with('N') || !id[1..].bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid(format!("expected concrete node ID N<number>, got `{id}`")).at(field));
    }
    id[1..]
        .parse::<u64>()
        .map_err(|_| invalid(format!("node ID suffix overflows: `{id}`")).at(field))?;
    Ok(())
}

fn strings(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|items| items.iter().all(Value::is_string))
}

fn validate_fields(kind: &str, fields: &Fields) -> Result<(), WriteError> {
    if !matches!(
        kind,
        "question" | "decision" | "experiment" | "dead_end" | "pivot"
    ) {
        return Err(invalid(format!("unsupported node type `{kind}`")).at("type"));
    }
    let required: &[&str] = match kind {
        "question" => &["description"],
        "decision" => &["choice", "alternatives"],
        "experiment" => &["result"],
        "dead_end" => &["hypothesis", "failure_mode", "lesson"],
        "pivot" => &["from", "to", "trigger"],
        _ => unreachable!("validated kind"),
    };
    for name in required {
        if !fields.get(*name).is_some_and(|value| {
            value.as_str().is_some_and(|text| !text.trim().is_empty())
                || *name == "alternatives" && strings(value)
        }) {
            return Err(
                invalid(format!("native `{kind}` requires caller-supplied `{name}`"))
                    .at(format!("fields.{name}")),
            );
        }
    }
    for (key, value) in fields {
        let accepted = match key.as_str() {
            "id" | "type" | "title" | "timestamp" | "provenance" | "support_level"
            | "description" | "thinking" | "parent" => Some(value.is_string()),
            "concepts" => Some(strings(value)),
            "source_refs" => Some(value.is_string() || strings(value)),
            "artifacts" => Some(value.as_array().is_some_and(|items| {
                items.iter().all(|item| {
                    item.as_object().is_some_and(|object| {
                        object
                            .keys()
                            .all(|key| matches!(key.as_str(), "name" | "pointer" | "what"))
                            && ["name", "pointer", "what"]
                                .iter()
                                .all(|key| object.get(*key).is_some_and(Value::is_string))
                    })
                })
            })),
            "choice" | "rationale" if kind == "decision" => Some(value.is_string()),
            "alternatives" if kind == "decision" => Some(strings(value)),
            "evidence" if matches!(kind, "decision" | "experiment") => {
                Some(value.is_string() || strings(value))
            }
            "status" => Some(value.is_string()),
            "result" | "exploration" | "outcome" if kind == "experiment" => Some(value.is_string()),
            "hypothesis" | "failure_mode" | "lesson" | "why_failed" if kind == "dead_end" => {
                Some(value.is_string())
            }
            "from" | "to" | "trigger" | "prior_direction" | "new_direction" | "reason"
                if kind == "pivot" =>
            {
                Some(value.is_string())
            }
            _ => None,
        };
        match accepted {
            None => {
                return Err(
                    invalid(format!("field `{key}` is not permitted on `{kind}` nodes"))
                        .at(format!("fields.{key}")),
                );
            }
            Some(false) => {
                return Err(
                    invalid(format!("field `{key}` has an incompatible value type"))
                        .at(format!("fields.{key}")),
                );
            }
            Some(true) => {}
        }
        if key == "parent" {
            check_id(value.as_str().expect("validated string"), "fields.parent")?;
        }
    }
    Ok(())
}

impl NodeIndex {
    pub(super) fn kind(&self, id: &str) -> Option<&str> {
        self.nodes.get(id).and_then(|node| node.kind.as_deref())
    }

    fn selector(&self, record: &NodeRecord) -> Vec<PathPart> {
        let mut children = Vec::new();
        let mut slot = record.selector;
        let (field, index) = loop {
            match self.locations[slot] {
                NodeLocation::Root { field, index } => break (field, index),
                NodeLocation::Child { parent, index } => {
                    children.push(index);
                    slot = parent;
                }
            }
        };
        let mut selector = Vec::with_capacity(children.len() * 2 + 2);
        selector.push(PathPart::from(field));
        if let Some(index) = index {
            selector.push(PathPart::Index(index));
        }
        for index in children.into_iter().rev() {
            selector.push(PathPart::from("children"));
            selector.push(PathPart::Index(index));
        }
        selector
    }
    fn get(&self, id: &str, field: &str) -> Result<&NodeRecord, WriteError> {
        self.nodes
            .get(id)
            .ok_or_else(|| invalid(format!("unknown node `{id}`")).at(field))
    }

    fn ancestor(&self, ancestor: &str, node: &str) -> bool {
        let mut current = node;
        while let Some(parent) = self
            .nodes
            .get(current)
            .and_then(|node| node.parent.as_deref())
        {
            if parent == ancestor {
                return true;
            }
            current = parent;
        }
        false
    }

    fn reachable(&self, from: &str, to: &str) -> bool {
        let mut pending = vec![from];
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if id == to {
                return true;
            }
            if !seen.insert(id) {
                continue;
            }
            if let Some(node) = self.nodes.get(id) {
                for dependency in &node.dependencies {
                    // Existing redundant ancestor edges are not part of the parsed graph.
                    if !self.ancestor(dependency, id) {
                        pending.push(dependency.as_str());
                    }
                }
            }
            // Match parse.rs: nesting points parent -> child, cross-edges node -> dependency.
            if let Some(children) = self.children.get(id) {
                pending.extend(children.iter().map(String::as_str));
            }
        }
        false
    }

    fn dependency(
        &self,
        node: &str,
        dependency: &str,
        require_target: bool,
    ) -> Result<(), WriteError> {
        check_id(dependency, "depends_on")?;
        if node == dependency {
            return Err(invalid("a node cannot depend on itself").at("depends_on"));
        }
        if require_target {
            self.get(dependency, "depends_on")?;
        }
        if self.ancestor(dependency, node) {
            return Err(invalid(format!(
                "dependency on ancestor `{dependency}` is redundant"
            ))
            .at("depends_on"));
        }
        if self.reachable(dependency, node) {
            return Err(invalid(format!(
                "dependency `{node}` -> `{dependency}` creates a cycle"
            ))
            .at("depends_on"));
        }
        Ok(())
    }
}

pub fn plan(
    working: &mut WorkingArtifact,
    operation: &WriteOperation,
) -> Result<OperationResult, WriteError> {
    match operation {
        WriteOperation::NodeAdd {
            id,
            kind,
            parent,
            title,
            fields,
            depends_on,
        } => {
            validate_fields(kind, fields)?;
            for (key, expected) in [("type", kind.as_str()), ("title", title.as_str())] {
                if fields
                    .get(key)
                    .is_some_and(|value| value.as_str() != Some(expected))
                {
                    return Err(invalid(format!("fields.{key} disagrees with `{key}`"))
                        .at(format!("fields.{key}")));
                }
            }
            if let Some(explicit) = fields.get("parent").and_then(Value::as_str)
                && (parent == "root" || explicit != parent)
            {
                return Err(invalid(
                    "fields.parent must agree with the operation's physical parent",
                )
                .at("fields.parent"));
            }
            working.ensure_yaml(TREE, "tree: []\n")?;
            let document = working.yaml(TREE)?;
            let mut source = index(&document)?;
            let requested = id
                .as_deref()
                .or_else(|| fields.get("id").and_then(Value::as_str));
            if id
                .as_deref()
                .zip(fields.get("id").and_then(Value::as_str))
                .is_some_and(|(id, field)| id != field)
            {
                return Err(invalid("fields.id disagrees with requested id").at("fields.id"));
            }
            let ids = source.nodes.keys().cloned().collect::<Vec<_>>();
            let assigned = working.allocate_id('N', &ids, requested)?;
            let mut authored: Map<String, Value> = fields
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            authored.insert("id".into(), Value::String(assigned.clone()));
            if !authored.contains_key("timestamp") {
                // An omitted creation time is the writer's captured batch time.
                authored.insert(
                    "timestamp".into(),
                    Value::String(working.clock_time()?.to_owned()),
                );
            }
            authored.insert("type".into(), Value::String(kind.clone()));
            authored.insert("title".into(), Value::String(title.clone()));
            let (selector, new_parent, replace_root) = if parent == "root" {
                match document.root.get("root")? {
                    Some(root) if !null(root) => return Err(invalid(
                        "the single-root dialect already has a root; select its node ID as parent",
                    )
                    .at("parent")),
                    Some(_) => (vec![PathPart::from("root")], None, true),
                    None => (vec![PathPart::from("tree")], None, false),
                }
            } else {
                check_id(parent, "parent")?;
                let parent_node = source.get(parent, "parent")?;
                if parent_node.kind.as_deref() == Some("dead_end") {
                    return Err(
                        invalid("dead_end nodes are leaves and cannot receive children")
                            .at("parent"),
                    );
                }
                let mut selector = source.selector(parent_node);
                selector.push(PathPart::from("children"));
                (selector, Some(parent.clone()), false)
            };
            let location = if let Some(parent) = &new_parent {
                let physical = document.root.at(&selector[..selector.len() - 1])?;
                let child_index = physical
                    .get("children")?
                    .filter(|node| !null(node))
                    .map(|node| node.sequence().map(<[_]>::len))
                    .transpose()?
                    .unwrap_or(0);
                NodeLocation::Child {
                    parent: source.nodes[parent].selector,
                    index: child_index,
                }
            } else if replace_root {
                NodeLocation::Root {
                    field: "root",
                    index: None,
                }
            } else {
                let root_index = document
                    .root
                    .get("tree")?
                    .filter(|node| !null(node))
                    .map(|node| node.sequence().map(<[_]>::len))
                    .transpose()?
                    .unwrap_or(0);
                NodeLocation::Root {
                    field: "tree",
                    index: Some(root_index),
                }
            };
            let location_slot = source.locations.len();
            source.locations.push(location);
            if let Some(parent) = &new_parent {
                source
                    .children
                    .entry(parent.clone())
                    .or_default()
                    .push(assigned.clone());
            }
            source.nodes.insert(
                assigned.clone(),
                NodeRecord {
                    id: assigned.clone(),
                    selector: location_slot,
                    kind: Some(kind.clone()),
                    parent: new_parent,
                    dependencies: Vec::new(),
                    same_as: Vec::new(),
                    timestamp: authored
                        .get("timestamp")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    concepts: fields
                        .get("concepts")
                        .and_then(Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default(),
                },
            );
            let mut distinct = BTreeSet::new();
            for dependency in depends_on {
                if !distinct.insert(dependency) {
                    return Err(
                        invalid(format!("duplicate requested dependency `{dependency}`"))
                            .at("depends_on"),
                    );
                }
                source.dependency(&assigned, dependency, false)?;
            }
            if !depends_on.is_empty() {
                authored.insert(
                    "also_depends_on".into(),
                    Value::Array(depends_on.iter().cloned().map(Value::String).collect()),
                );
            }
            let authored = Value::Object(authored);
            if replace_root {
                working.replace_yaml_field(TREE, &[], "root", &authored)?;
            } else {
                append_node(working, &document, &selector, &authored)?;
            }
            working
                .intents
                .last_mut()
                .expect("node append records source edit")
                .reason = format!("node.add:{assigned}");
            Ok(OperationResult::new("node.add", Some(assigned)))
        }
        WriteOperation::EdgeAdd { node, depends_on } => link(working, node, depends_on, false),
        WriteOperation::NodeLinkSameAs { node, same_as } => link(working, node, same_as, true),
        _ => Err(invalid("operation is not a node operation")),
    }
}

fn append_node(
    working: &mut WorkingArtifact,
    document: &YamlDocument,
    selector: &[PathPart],
    authored: &Value,
) -> Result<(), WriteError> {
    let (last, parent) = selector.split_last().expect("collection selector");
    let PathPart::Key(key) = last else {
        return Err(invalid("invalid node insertion selector"));
    };
    let target = document.root.at(parent)?;
    match target.get(key)? {
        None => {
            working.replace_yaml_field(TREE, parent, key, &Value::Array(vec![authored.clone()]))
        }
        Some(value) if null(value) => {
            working.replace_yaml_field(TREE, parent, key, &Value::Array(vec![authored.clone()]))
        }
        Some(value) => {
            value.sequence()?;
            working.append_yaml(TREE, selector, authored)
        }
    }
}

fn link(
    working: &mut WorkingArtifact,
    node: &str,
    target: &str,
    same_as: bool,
) -> Result<OperationResult, WriteError> {
    check_id(node, "node")?;
    check_id(target, if same_as { "same_as" } else { "depends_on" })?;
    let document = working.yaml(TREE)?;
    let source = index(&document)?;
    let record = source.get(node, "node")?;
    let operation = if same_as {
        "node.link_same_as"
    } else {
        "edge.add"
    };
    let field = if same_as {
        "same_as"
    } else {
        "also_depends_on"
    };
    if node == target {
        return Err(invalid("a node cannot link to itself").at(field));
    }
    let existing = if same_as {
        &record.same_as
    } else {
        &record.dependencies
    };
    let mut result = OperationResult::new(operation, Some(node.to_owned()));
    result.target = Some(target.to_owned());
    if existing.iter().any(|id| id == target) {
        if same_as {
            return Err(invalid("duplicate same_as target").at(field));
        }
        result.no_op = true;
        return Ok(result);
    }
    if !same_as {
        source.dependency(node, target, false)?;
    }
    let mut selector = source.selector(record);
    let mapping = document.root.at(&selector)?;
    match mapping.get(field)? {
        None => working.replace_yaml_field(
            TREE,
            &selector,
            field,
            &Value::Array(vec![Value::String(target.into())]),
        )?,
        Some(value) if null(value) => working.replace_yaml_field(
            TREE,
            &selector,
            field,
            &Value::Array(vec![Value::String(target.into())]),
        )?,
        Some(_) => {
            selector.push(PathPart::from(field));
            working.append_yaml(TREE, &selector, &Value::String(target.into()))?;
        }
    }
    working
        .intents
        .last_mut()
        .expect("link append records source edit")
        .reason = format!("node.{}:{node}", if same_as { "same_as" } else { "edge" });
    Ok(result)
}

/// Resolve concrete forward references only after every operation has been planned.
/// Existing unrelated source references and diagnostics are not rewritten or tightened.
pub fn validate_references(working: &WorkingArtifact) -> Result<(), WriteError> {
    if !working.exists(TREE) || !working.files.contains_key(TREE) {
        return Ok(());
    }
    let candidate = cached_node_index(working)?;
    let original = if working
        .base
        .files
        .get(TREE)
        .is_some_and(|file| file.existed)
    {
        let file = &working.base.files[TREE];
        let text = std::str::from_utf8(&file.bytes)
            .map_err(|_| WriteError::io("exploration tree is not UTF-8"))?;
        Some(index(working.indexed_yaml(TREE, text, true)?.as_ref())?)
    } else {
        None
    };
    let created: BTreeMap<&str, usize> = working
        .intents
        .iter()
        .enumerate()
        .filter_map(|(ordinal, intent)| {
            intent
                .reason
                .strip_prefix("node.add:")
                .map(|id| (id, ordinal))
        })
        .collect();
    let authored_edges: BTreeSet<&str> = working
        .intents
        .iter()
        .filter_map(|intent| intent.reason.strip_prefix("node.edge:"))
        .collect();
    let authored_same_as: BTreeSet<&str> = working
        .intents
        .iter()
        .filter_map(|intent| intent.reason.strip_prefix("node.same_as:"))
        .collect();
    let imported = super::intent::imports_history(working, TREE)?;
    for node in candidate.nodes.values() {
        let previous = original.as_ref().and_then(|base| base.nodes.get(&node.id));
        let imported_links = imported
            && !created.contains_key(node.id.as_str())
            && !authored_same_as.contains(node.id.as_str());
        for dependency in &node.dependencies {
            if previous.is_some_and(|node| node.dependencies.contains(dependency)) {
                continue;
            }
            if created.contains_key(node.id.as_str()) || authored_edges.contains(node.id.as_str()) {
                candidate.dependency(&node.id, dependency, true)?;
            } else {
                candidate.get(dependency, "also_depends_on")?;
            }
        }
        let mut same_as_counts = BTreeMap::new();
        for same_as in &node.same_as {
            let count = same_as_counts.entry(same_as).or_insert(0usize);
            *count += 1;
            let old_count = previous.map_or(0, |node| {
                node.same_as
                    .iter()
                    .filter(|target| *target == same_as)
                    .count()
            });
            if !imported_links && *count > 1 && *count > old_count {
                return Err(invalid("duplicate same_as target").at("same_as"));
            }
            if previous.is_some_and(|node| node.same_as.contains(same_as)) {
                continue;
            }
            check_id(same_as, "same_as")?;
            candidate.get(same_as, "same_as")?;
            if same_as == &node.id {
                return Err(invalid("a node cannot link to itself").at("same_as"));
            }
            if !imported_links {
                let target = candidate.get(same_as, "same_as")?;
                let append_order = created.get(node.id.as_str()).is_some_and(|current| {
                    created.get(same_as.as_str()).map_or_else(
                        || {
                            original
                                .as_ref()
                                .is_some_and(|base| base.nodes.contains_key(same_as))
                        },
                        |previous| previous < current,
                    )
                });
                let earlier = match (node.timestamp.as_deref(), target.timestamp.as_deref()) {
                    (Some(current), Some(previous)) => {
                        match super::sessions::timestamp_key(previous)?
                            .cmp(&super::sessions::timestamp_key(current)?)
                        {
                            std::cmp::Ordering::Less => true,
                            std::cmp::Ordering::Equal => append_order,
                            std::cmp::Ordering::Greater => false,
                        }
                    }
                    _ => append_order,
                };
                if !earlier {
                    return Err(invalid(
                        "same_as must point from a later node to a proven earlier node",
                    )
                    .at("same_as"));
                }
                let mut pending = vec![same_as.as_str()];
                let mut visited = BTreeSet::new();
                while let Some(id) = pending.pop() {
                    if id == node.id {
                        return Err(invalid("same_as cannot introduce a cycle").at("same_as"));
                    }
                    if visited.insert(id) {
                        // Historical dangling links remain readable; only the
                        // newly authored target must exist. Still follow every
                        // available historical edge to detect a new cycle.
                        if let Some(record) = candidate.nodes.get(id) {
                            pending.extend(record.same_as.iter().map(String::as_str));
                        }
                    }
                }
            }
        }
        for concept in &node.concepts {
            if previous.is_some_and(|node| node.concepts.contains(concept)) {
                continue;
            }
            if !crate::merge::concept_reference_exists(working, concept)
                .map_err(|error| WriteError::semantic(&error.code, error.message))?
            {
                return Err(
                    invalid(format!("concepts references unknown concept `{concept}`"))
                        .at("concepts"),
                );
            }
        }
    }
    let mut known = None;
    for intent in &working.intents {
        if intent.path != TREE {
            continue;
        }
        let Some(encoded) = intent.reason.strip_prefix(ANNOTATION_INTENT) else {
            continue;
        };
        let annotation: AnnotationReferences = serde_json::from_str(encoded)
            .map_err(|error| invalid(format!("invalid conflict annotation intent: {error}")))?;
        if known.is_none() {
            known = Some(super::logic::known_reference_ids(working)?);
        }
        for reference in annotation.references {
            if !known
                .as_ref()
                .expect("initialized reference index")
                .contains(&reference)
            {
                return Err(invalid(format!(
                    "conflict annotation references unknown entry `{reference}`"
                ))
                .at("references"));
            }
        }
    }
    Ok(())
}

/// Conflict annotations add a comment only; they never change node values.
pub fn annotate(
    working: &mut WorkingArtifact,
    id: &str,
    kind: &str,
    references: &[String],
    comment: &str,
) -> Result<OperationResult, WriteError> {
    if kind != "conflict" || references.is_empty() {
        return Err(
            invalid("node annotations require kind=conflict and at least one reference").at("kind"),
        );
    }
    if references
        .iter()
        .any(|reference| reference.is_empty() || reference.contains(['\r', '\n']))
    {
        return Err(
            invalid("conflict annotation references must be nonempty and single-line")
                .at("references"),
        );
    }
    let document = working.yaml(TREE)?;
    let source = index(&document)?;
    let record = source.get(id, "target.id")?;
    let node = document.root.at(&source.selector(record))?;
    if node.flow {
        return Err(invalid("cannot annotate a flow-style node mapping"));
    }
    let text = working.text(TREE)?;
    let line_start = text[..node.start].rfind('\n').map_or(0, |index| index + 1);
    let indent = text[line_start..node.start]
        .chars()
        .take_while(|character| *character == ' ')
        .count();
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let payload = serde_json::to_string(&AnnotationPayload {
        kind,
        references,
        comment,
    })
    .map_err(|error| invalid(format!("cannot encode annotation payload: {error}")))?;
    let padding = " ".repeat(indent);
    let mut annotation = String::new();
    for reference in references {
        annotation.push_str(&padding);
        annotation.push_str("# CONFLICT: see ");
        annotation.push_str(reference);
        annotation.push_str(eol);
    }
    annotation.push_str(&padding);
    annotation.push_str("# ARA annotation: ");
    annotation.push_str(&payload);
    annotation.push_str(eol);
    let reason = format!("{ANNOTATION_INTENT}{payload}");
    working.edit(TREE, line_start..line_start, &annotation, &reason)?;
    let mut result = OperationResult::new("entry.annotate", Some(id.into()));
    result.target = Some(id.into());
    Ok(result)
}

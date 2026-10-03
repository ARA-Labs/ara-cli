//! Agent reads over one complete native load and a borrowing graph index.
use crate::output::{AgentError, excerpt};
use ara_core::query::{QueryIndex, scan_tokens, token_may_refer};
use ara_core::{Manifest, NodeFields, NodeKind, parse_dir_detailed};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
mod references;

#[derive(Debug, Default, Clone, clap::Args)]
pub struct ReadOptions {
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub full: bool,
    #[arg(long)]
    pub fields: Option<String>,
}
#[derive(Debug, Default, Clone, clap::Args)]
pub struct ListArgs {
    #[command(flatten)]
    pub output: ReadOptions,
    #[arg(long = "type")]
    pub kind: Option<String>,
    #[arg(long)]
    pub under: Option<String>,
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub provenance: Option<String>,
}
#[derive(Debug, Default, Clone, clap::Args)]
pub struct ShowArgs {
    pub ids: Vec<String>,
    #[command(flatten)]
    pub output: ReadOptions,
    #[arg(long = "with", value_delimiter = ',')]
    pub relations: Vec<String>,
    #[arg(long)]
    pub document: Option<String>,
    #[arg(long)]
    pub heading: Vec<String>,
    #[arg(long)]
    pub source: bool,
}
#[derive(Debug, Clone, clap::Args)]
pub struct IdArgs {
    pub id: String,
    #[command(flatten)]
    pub output: ReadOptions,
}

pub struct Artifact {
    pub manifest: Manifest,
    pub sources: BTreeMap<String, String>,
    pub root: std::path::PathBuf,
    pub report: ara_core::ParseReport,
    pub claim_redirects: BTreeMap<String, String>,
    snapshot: std::cell::OnceCell<ara_core::write::ArtifactSnapshot>,
}
impl Artifact {
    pub fn load(root: &Path) -> Result<Self, AgentError> {
        let loaded = parse_dir_detailed(root);
        if !loaded.io_issues.is_empty() {
            return Err(AgentError::io(format!(
                "Cannot read artifact sources: {:?}",
                loaded.io_issues
            )));
        }
        if !loaded.report.is_ok() {
            return Err(AgentError::semantic(
                "invalid_artifact",
                loaded.report.to_string(),
            ));
        }
        if !representable(&loaded.report) {
            return Err(AgentError::semantic(
                "incomplete_artifact",
                loaded.report.to_string(),
            ));
        }
        let manifest = loaded.manifest.ok_or_else(|| {
            AgentError::semantic(
                "incomplete_artifact",
                "The source cannot be represented completely; structural queries refused",
            )
        })?;
        Ok(Self {
            manifest,
            sources: loaded.sources,
            root: root.to_path_buf(),
            report: loaded.report,
            claim_redirects: loaded.claim_redirects,
            snapshot: std::cell::OnceCell::new(),
        })
    }
    fn entries(&self) -> Vec<Entry<'_>> {
        let mut result = entries(&self.manifest);
        result.extend(
            self.sources
                .iter()
                .filter(|(path, _)| self.is_knowledge(path))
                .map(|(path, content)| Entry::Document { path, content }),
        );
        result
    }
    fn is_knowledge(&self, path: &str) -> bool {
        knowledge_document(path) || !matches!(path.split('/').next(), Some("evidence" | "src"))
    }
    pub fn searchable_documents(&self) -> Vec<(&str, &str)> {
        self.sources
            .iter()
            .filter(|(path, _)| path.ends_with(".md") && self.is_knowledge(path))
            .filter_map(|(path, text)| {
                if path != "logic/solution/heuristics.md"
                    && path
                        .strip_prefix("logic/solution/")
                        .and_then(|path| path.strip_suffix(".md"))
                        .is_some_and(|name| {
                            self.manifest
                                .recipes
                                .iter()
                                .any(|recipe| recipe.name == name)
                        })
                {
                    return None;
                }
                let typed = matches!(
                    path.as_str(),
                    "logic/claims.md"
                        | "logic/concepts.md"
                        | "logic/related_work.md"
                        | "logic/experiments.md"
                        | "logic/solution/heuristics.md"
                );
                let content = if typed {
                    let end = ara_core::markdown::sections(text)
                        .first()
                        .map_or(text.len(), |section| section.range.start);
                    &text[..end]
                } else {
                    text.as_str()
                };
                (!content.trim().is_empty()).then_some((path.as_str(), content))
            })
            .collect()
    }
    fn snapshot(&self) -> Result<&ara_core::write::ArtifactSnapshot, AgentError> {
        if self.snapshot.get().is_none() {
            let snapshot = ara_core::write::ArtifactSnapshot::load_with_identities(&self.root)
                .map_err(crate::write::convert_error)?;
            for (path, source) in &self.sources {
                if self.is_knowledge(path)
                    && !snapshot
                        .files
                        .get(path)
                        .is_some_and(|file| file.existed && file.bytes == source.as_bytes())
                {
                    return Err(AgentError::semantic(
                        "stale_read_snapshot",
                        format!("Source `{path}` changed while resolving identities"),
                    ));
                }
            }
            let _ = self.snapshot.set(snapshot);
        }
        Ok(self.snapshot.get().expect("initialized"))
    }
    fn entry(&self, id: &str) -> Result<Entry<'_>, AgentError> {
        let resolved = if id.contains(':')
            || id.contains('#')
            || id.contains('@')
            || self.sources.contains_key("trace/logic_mutations.yaml")
            || self.sources.contains_key("trace/merge_log.yaml")
        {
            Some(
                ara_core::merge::resolve_local(self.snapshot()?, id)
                    .map_err(crate::merge::convert_error)?,
            )
        } else {
            None
        };
        let resolved = resolved.as_deref().unwrap_or(id);
        let native = id
            .split_once(':')
            .or_else(|| id.split_once('#'))
            .filter(|(path, _)| self.is_knowledge(path) && path.contains('/'));
        let (scope, key) = if let Some((path, key)) = resolved.split_once('#') {
            (Some(path), key)
        } else {
            (native.map(|(path, _)| path), resolved)
        };
        let recipe_key = key
            .strip_prefix("logic/solution/")
            .and_then(|path| path.strip_suffix(".md"));
        let has_recipe = recipe_key.is_some_and(|name| {
            self.manifest
                .recipes
                .iter()
                .any(|recipe| recipe.name == name)
        });
        let mut matching = self.entries().into_iter().filter(|entry|scope.is_none_or(|path|entry.source_matches(path))&&(entry.key()==key||matches!(entry,Entry::Recipe(recipe) if recipe_key==Some(recipe.name.as_str())))&&!(has_recipe&&matches!(entry,Entry::Document{..})));
        let entry = matching.next().ok_or_else(|| AgentError::unknown(id))?;
        if matching.next().is_some() {
            return Err(AgentError::unknown(id));
        }
        Ok(entry)
    }
}
fn representable(report: &ara_core::ParseReport) -> bool {
    !report.warnings().iter().any(|diagnostic| {
        matches!(
            diagnostic.code,
            ara_core::RuleCode::MalformedAgentLayer
                | ara_core::RuleCode::DuplicateAgentId
                | ara_core::RuleCode::MalformedSameAs
                | ara_core::RuleCode::MalformedNodeAnnotation
        )
    })
}
fn diagnostics(report: &ara_core::ParseReport) -> Value {
    json!({"errors":report.errors().iter().map(|diagnostic|json!({"code":diagnostic.code,"severity":diagnostic.severity,"path":diagnostic.path,"message":diagnostic.message})).collect::<Vec<_>>(),"warnings":report.warnings().iter().map(|diagnostic|json!({"code":diagnostic.code,"severity":diagnostic.severity,"path":diagnostic.path,"message":diagnostic.message})).collect::<Vec<_>>()})
}

#[derive(Clone, Copy)]
enum Entry<'a> {
    Node(&'a ara_core::Node),
    Claim(&'a ara_core::Claim),
    Observation(&'a ara_core::Observation),
    Session(&'a ara_core::Session),
    Heuristic(&'a ara_core::Heuristic),
    Experiment(&'a ara_core::ExperimentPlan),
    Taste(&'a ara_core::TasteComment),
    Concept(&'a ara_core::Concept),
    RelatedWork(&'a ara_core::RelatedWork),
    Recipe(&'a ara_core::Recipe),
    Exhibit(&'a ara_core::Exhibit),
    Document { path: &'a str, content: &'a str },
}
fn entries(manifest: &Manifest) -> Vec<Entry<'_>> {
    manifest
        .nodes
        .iter()
        .map(Entry::Node)
        .chain(manifest.claims.iter().map(Entry::Claim))
        .chain(manifest.observations.iter().map(Entry::Observation))
        .chain(manifest.sessions.iter().map(Entry::Session))
        .chain(manifest.heuristics.iter().map(Entry::Heuristic))
        .chain(manifest.experiment_plans.iter().map(Entry::Experiment))
        .chain(manifest.taste_comments.iter().map(Entry::Taste))
        .chain(manifest.concepts.iter().map(Entry::Concept))
        .chain(manifest.related_work.iter().map(Entry::RelatedWork))
        .chain(
            manifest
                .recipes
                .iter()
                .filter(|recipe| recipe.name != "heuristics")
                .map(Entry::Recipe),
        )
        .chain(manifest.exhibits.iter().map(Entry::Exhibit))
        .collect()
}
impl<'a> Entry<'a> {
    fn key(self) -> &'a str {
        match self {
            Self::Node(v) => v.id.as_str(),
            Self::Claim(v) => v.id.as_str(),
            Self::Observation(v) => v.id.as_str(),
            Self::Session(v) => v.id.as_str(),
            Self::Heuristic(v) => v.id.as_str(),
            Self::Experiment(v) => v.id.as_str(),
            Self::Taste(v) => v.id.as_str(),
            Self::Concept(v) => &v.term,
            Self::RelatedWork(v) => &v.id,
            Self::Recipe(v) => &v.name,
            Self::Exhibit(v) => &v.id,
            Self::Document { path, .. } => path,
        }
    }
    fn kind(self) -> &'a str {
        match self {
            Self::Node(v) => node_kind(&v.kind),
            Self::Claim(_) => "claim",
            Self::Observation(_) => "observation",
            Self::Session(_) => "session",
            Self::Heuristic(_) => "heuristic",
            Self::Experiment(_) => "experiment_plan",
            Self::Taste(_) => "taste",
            Self::Concept(_) => "concept",
            Self::RelatedWork(_) => "related_work",
            Self::Recipe(_) => "solution",
            Self::Exhibit(_) => "exhibit",
            Self::Document { .. } => "source_document",
        }
    }
    fn date(self) -> Option<&'a str> {
        match self {
            Self::Node(v) => v.timestamp.as_deref(),
            Self::Observation(v) => v.timestamp.as_deref(),
            Self::Session(v) => v.date.as_deref(),
            Self::Taste(v) => v.timestamp.as_deref(),
            _ => None,
        }
    }
    fn status(self) -> Option<&'a str> {
        match self {
            Self::Node(v) => v.status.as_deref().or({
                if let NodeFields::Experiment { status, .. } = &v.fields {
                    status.as_deref()
                } else {
                    None
                }
            }),
            Self::Claim(v) => v.status.as_deref(),
            Self::Heuristic(v) => v.status.as_deref(),
            Self::Experiment(v) => v.status.as_deref(),
            _ => None,
        }
    }
    fn provenance(self) -> Option<&'a str> {
        match self {
            Self::Node(v) => v.provenance.as_deref(),
            Self::Claim(v) => v.provenance.as_deref(),
            Self::Observation(v) => v.provenance.as_deref(),
            Self::Heuristic(v) => v.provenance.as_deref(),
            Self::Experiment(v) => v.provenance.as_deref(),
            _ => None,
        }
    }
    fn source_matches(self, path: &str) -> bool {
        match self {
            Self::Node(_) => path == "trace/exploration_tree.yaml",
            Self::Claim(_) => path == "logic/claims.md",
            Self::Observation(entry) => path == entry.source_file,
            Self::Session(entry) => path == entry.source_file,
            Self::Heuristic(entry) => path == entry.source_file,
            Self::Experiment(entry) => path == entry.source_file,
            Self::Taste(entry) => path == entry.source_file,
            Self::Concept(_) => path == "logic/concepts.md",
            Self::RelatedWork(_) => path == "logic/related_work.md",
            Self::Recipe(entry) => {
                path.strip_prefix("logic/solution/")
                    .and_then(|path| path.strip_suffix(".md"))
                    == Some(entry.name.as_str())
            }
            Self::Exhibit(entry) => path == entry.file,
            Self::Document { path: source, .. } => path == source,
        }
    }
    fn value(self, full: bool) -> Value {
        let mut value = match self {
            Self::Node(v) => serde_json::to_value(v),
            Self::Claim(v) => serde_json::to_value(v),
            Self::Observation(v) => serde_json::to_value(v),
            Self::Session(v) => serde_json::to_value(v),
            Self::Heuristic(v) => serde_json::to_value(v),
            Self::Experiment(v) => serde_json::to_value(v),
            Self::Taste(v) => serde_json::to_value(v),
            Self::Concept(v) => serde_json::to_value(v),
            Self::RelatedWork(v) => serde_json::to_value(v),
            Self::Recipe(v) => serde_json::to_value(v),
            Self::Exhibit(v) => serde_json::to_value(v),
            Self::Document { content, .. } => Ok(json!({"content":content})),
        }
        .expect("entry serialization is infallible");
        let object = value.as_object_mut().expect("entry object");
        object.insert("kind".into(), json!(self.kind()));
        if !object.contains_key("id") {
            object.insert("key".into(), json!(self.key()));
        }
        if let Some(status) = self.status() {
            object.insert("status".into(), json!(status));
        }
        let source = match self {
            Self::Node(_) => "trace/exploration_tree.yaml".to_owned(),
            Self::Claim(_) => "logic/claims.md".into(),
            Self::Observation(v) => v.source_file.clone(),
            Self::Session(v) => v.source_file.clone(),
            Self::Heuristic(v) => v.source_file.clone(),
            Self::Experiment(v) => v.source_file.clone(),
            Self::Taste(v) => v.source_file.clone(),
            Self::Concept(_) => "logic/concepts.md".into(),
            Self::RelatedWork(_) => "logic/related_work.md".into(),
            Self::Recipe(v) => format!("logic/solution/{}.md", v.name),
            Self::Exhibit(v) => v.file.clone(),
            Self::Document { path, .. } => path.into(),
        };
        object.insert("source".into(), json!(source));
        if matches!(self, Self::Recipe(_)) {
            object.insert("key".into(), json!(source));
        }
        if let Self::Node(node) = self {
            object.insert("title".into(), json!(node.label));
        }
        if !full {
            shorten(&mut value);
        }
        value
    }
}
pub fn node_kind(kind: &NodeKind) -> &str {
    match kind {
        NodeKind::Question => "question",
        NodeKind::Experiment => "experiment",
        NodeKind::Decision => "decision",
        NodeKind::DeadEnd => "dead_end",
        NodeKind::Insight => "insight",
        NodeKind::Pivot => "pivot",
        NodeKind::Other(name) => name,
    }
}
fn shorten(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if !matches!(
                    key.as_str(),
                    "id" | "key"
                        | "kind"
                        | "source"
                        | "source_file"
                        | "timestamp"
                        | "date"
                        | "promoted_to"
                        | "target"
                        | "provenance"
                        | "status"
                ) {
                    shorten(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                shorten(value);
            }
        }
        Value::String(text) => *text = excerpt(text),
        _ => {}
    }
}
fn validate_kind(artifact: &Artifact, kind: &str) -> Result<(), AgentError> {
    if matches!(
        kind,
        "question"
            | "experiment"
            | "decision"
            | "dead_end"
            | "insight"
            | "pivot"
            | "claim"
            | "heuristic"
            | "observation"
            | "session"
            | "exhibit"
            | "concept"
            | "related_work"
            | "solution"
            | "experiment_plan"
            | "taste"
            | "source_document"
    ) || artifact
        .manifest
        .nodes
        .iter()
        .any(|n| node_kind(&n.kind) == kind)
    {
        Ok(())
    } else {
        Err(AgentError::semantic(
            "unknown_type",
            format!("Unknown entry type `{kind}`"),
        ))
    }
}
pub fn list(root: &Path, args: &ListArgs) -> Result<Value, AgentError> {
    let artifact = Artifact::load(root)?;
    if let Some(kind) = &args.kind {
        validate_kind(&artifact, kind)?;
    }
    if let Some(date) = &args.since {
        validate_date(date)?;
    }
    let index = QueryIndex::new(&artifact.manifest);
    let descendants = args
        .under
        .as_deref()
        .map(|id| {
            artifact.entry(id).and_then(|entry| {
                index
                    .descendants(entry.key())
                    .ok_or_else(|| AgentError::unknown(id))
            })
        })
        .transpose()?;
    let rows = artifact
        .entries()
        .into_iter()
        .filter(|entry| {
            args.kind.as_deref().is_none_or(|kind| entry.kind() == kind)
                && descendants
                    .as_ref()
                    .is_none_or(|ids| matches!(entry, Entry::Node(_)) && ids.contains(entry.key()))
                && args.since.as_deref().is_none_or(|since| {
                    entry
                        .date()
                        .and_then(|date| date.get(..10))
                        .is_some_and(|date| date >= since)
                })
                && args
                    .status
                    .as_deref()
                    .is_none_or(|status| entry.status() == Some(status))
                && args
                    .provenance
                    .as_deref()
                    .is_none_or(|provenance| entry.provenance() == Some(provenance))
        })
        .map(|entry| entry.value(args.output.full))
        .collect::<Vec<_>>();
    Ok(json!({"format":"ara.ls/v1","entries":rows,"diagnostics":diagnostics(&artifact.report)}))
}
pub fn show(root: &Path, args: &ShowArgs) -> Result<Value, AgentError> {
    if args.source {
        let document = args.document.as_deref().ok_or_else(|| {
            AgentError::semantic("invalid_selector", "--source requires --document")
        })?;
        if !args.ids.is_empty() || !args.relations.is_empty() {
            return Err(AgentError::semantic(
                "invalid_selector",
                "Source selection cannot be mixed with entry IDs or relations",
            ));
        }
        return show_source(root, document, &args.heading);
    }
    let artifact = Artifact::load(root)?;
    show_loaded(&artifact, args)
}
pub fn show_loaded(artifact: &Artifact, args: &ShowArgs) -> Result<Value, AgentError> {
    if let Some(document) = &args.document {
        return show_document(
            artifact,
            document,
            &args.heading,
            args.source,
            args.output.full,
        );
    }
    if args.ids.is_empty() || !args.heading.is_empty() || args.source {
        return Err(AgentError::semantic(
            "invalid_selector",
            "Supply entry IDs or --document; heading/source require a document",
        ));
    }
    for relation in &args.relations {
        if !matches!(
            relation.as_str(),
            "parents" | "children" | "claims" | "sessions" | "same_as" | "depends_on"
        ) {
            return Err(AgentError::semantic(
                "unknown_relation",
                format!("Unknown relation `{relation}`"),
            ));
        }
    }
    if args.ids.len() == 1
        && artifact.sources.contains_key(&args.ids[0])
        && !artifact
            .manifest
            .recipes
            .iter()
            .any(|recipe| format!("logic/solution/{}.md", recipe.name) == args.ids[0])
    {
        return show_document(artifact, &args.ids[0], &[], false, args.output.full);
    }
    let selected = args
        .ids
        .iter()
        .map(|id| artifact.entry(id))
        .collect::<Result<Vec<_>, _>>()?;
    let index = QueryIndex::new(&artifact.manifest);
    let mut rows = Vec::with_capacity(selected.len());
    let requested: Vec<_> = selected
        .iter()
        .filter_map(|entry| {
            if let Entry::Node(node) = entry {
                Some(node.id.as_str())
            } else {
                None
            }
        })
        .collect();
    let raw_nodes = if args.output.full && !requested.is_empty() {
        ara_core::source_node_fields(
            artifact
                .sources
                .get("trace/exploration_tree.yaml")
                .expect("loaded tree"),
            &requested,
        )
        .map_err(|message| AgentError::semantic("invalid_artifact", message))?
    } else {
        BTreeMap::new()
    };
    for entry in selected {
        let mut value = entry.value(args.output.full);
        if let Entry::Node(node) = entry
            && let Some(raw) = raw_nodes.get(node.id.as_str())
        {
            value.as_object_mut().unwrap().insert(
                "source_fields".into(),
                serde_json::to_value(raw).map_err(|error| {
                    AgentError::semantic("invalid_source_value", error.to_string())
                })?,
            );
        }
        let mut relations = serde_json::Map::new();
        for relation in &args.relations {
            let data = match relation.as_str() {
                "parents" => json!(index.parent(entry.key()).into_iter().collect::<Vec<_>>()),
                "children" => json!(index.children(entry.key())),
                "depends_on" => json!(index.dependencies(entry.key())),
                "claims" => json!(
                    artifact
                        .manifest
                        .bindings
                        .iter()
                        .filter(|b| b.node.as_str() == entry.key())
                        .map(|b| b.claim.as_str())
                        .collect::<Vec<_>>()
                ),
                "sessions" => json!(
                    artifact
                        .manifest
                        .sessions
                        .iter()
                        .filter(|s| scan_tokens(&s.body)
                            .iter()
                            .any(|t| t.literal == entry.key()))
                        .map(|s| s.id.as_str())
                        .collect::<Vec<_>>()
                ),
                "same_as" => same_as_relation(artifact, entry.key()),
                _ => unreachable!(),
            };
            relations.insert(relation.clone(), data);
        }
        if !relations.is_empty() {
            value
                .as_object_mut()
                .unwrap()
                .insert("relations".into(), Value::Object(relations));
        }
        rows.push(value);
    }
    Ok(json!({"format":"ara.show/v1","entries":rows,"diagnostics":diagnostics(&artifact.report)}))
}
fn same_as_relation(artifact: &Artifact, id: &str) -> Value {
    let outgoing = artifact
        .manifest
        .nodes
        .iter()
        .find(|node| node.id.as_str() == id)
        .map(|node| &node.same_as);
    let incoming = artifact
        .manifest
        .nodes
        .iter()
        .filter(|node| node.same_as.iter().any(|target| target.as_str() == id))
        .map(|node| node.id.as_str())
        .collect::<Vec<_>>();
    json!({"outgoing":outgoing.into_iter().flatten().collect::<Vec<_>>(),"incoming":incoming})
}
pub fn path(root: &Path, args: &IdArgs) -> Result<Value, AgentError> {
    let artifact = Artifact::load(root)?;
    let index = QueryIndex::new(&artifact.manifest);
    let selected = artifact.entry(&args.id)?;
    let path = index
        .path(selected.key())
        .ok_or_else(|| AgentError::unknown(&args.id))?;
    Ok(
        json!({"format":"ara.path/v1","steps":path.into_iter().map(|n| Entry::Node(n).value(args.output.full)).collect::<Vec<_>>()}),
    )
}
pub fn status(root: &Path) -> Result<Value, AgentError> {
    let loaded = parse_dir_detailed(root);
    if !loaded.io_issues.is_empty() {
        return Err(AgentError::io(format!(
            "Cannot read artifact: {:?}",
            loaded.io_issues
        )));
    }
    let complete =
        loaded.report.is_ok() && loaded.manifest.is_some() && representable(&loaded.report);
    let mut counts = BTreeMap::<String, usize>::new();
    let mut native_ids = BTreeMap::<char, Vec<String>>::from([
        ('N', Vec::new()),
        ('C', Vec::new()),
        ('H', Vec::new()),
        ('E', Vec::new()),
        ('O', Vec::new()),
        ('T', Vec::new()),
    ]);
    let mut latest: Option<&str> = None;
    if let Some(manifest) = &loaded.manifest {
        for entry in entries(manifest) {
            *counts.entry(entry.kind().into()).or_default() += 1;
            let prefix = match entry {
                Entry::Node(_) => Some('N'),
                Entry::Claim(_) => Some('C'),
                Entry::Heuristic(_) => Some('H'),
                Entry::Experiment(_) => Some('E'),
                Entry::Observation(_) => Some('O'),
                Entry::Taste(_) => Some('T'),
                _ => None,
            };
            if let Some(prefix) = prefix {
                native_ids
                    .get_mut(&prefix)
                    .expect("native namespace")
                    .push(entry.key().into());
            }
        }
        latest = manifest.sessions.iter().map(|s| s.id.as_str()).max();
    }
    let historical = complete
        && (loaded.sources.contains_key("trace/logic_mutations.yaml")
            || loaded.sources.contains_key("trace/merge_log.yaml"));
    let working = if historical {
        Some(ara_core::write::WorkingArtifact::new(
            ara_core::write::ArtifactSnapshot::load(root).map_err(crate::write::convert_error)?,
        ))
    } else {
        None
    };
    let mut next_ids = BTreeMap::new();
    let mut next_id_errors = BTreeMap::new();
    if complete {
        for (prefix, ids) in native_ids {
            let next = if let Some(working) = &working {
                working.allocate_id(prefix, &ids, None)
            } else {
                ara_core::write::source::allocate_id(prefix, &ids, None)
            };
            match next {
                Ok(id) => {
                    next_ids.insert(prefix.to_string(), Some(id));
                }
                Err(error) => {
                    next_ids.insert(prefix.to_string(), None);
                    next_id_errors.insert(prefix.to_string(), error);
                }
            }
        }
    }
    let files = artifact_files(root)?;
    let total_bytes = files
        .iter()
        .try_fold(0u64, |total, file| total.checked_add(file.bytes))
        .ok_or_else(|| AgentError::io("Artifact byte size exceeds u64"))?;
    Ok(
        json!({"format":"ara.status/v1","artifact_location":root.canonicalize().map_err(|error|AgentError::io(error.to_string()))?,"file_count":files.len(),"total_bytes":total_bytes,"files":files,"complete":complete,"counts":if complete {json!(counts)}else{Value::Null},"next_ids":if complete{json!(next_ids)}else{Value::Null},"next_id_errors":next_id_errors,"latest_session":latest,"diagnostics":{"errors":loaded.report.errors().len(),"warnings":loaded.report.warnings().len(),"report":loaded.report}}),
    )
}
#[derive(serde::Serialize)]
struct ArtifactFile {
    path: String,
    bytes: u64,
    kind: &'static str,
}
fn artifact_files(root: &Path) -> Result<Vec<ArtifactFile>, AgentError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .map_err(|error| AgentError::io(format!("{}: {error}", directory.display())))?
        {
            let entry = entry.map_err(|error| AgentError::io(error.to_string()))?;
            if matches!(entry.file_name().to_str(), Some(".git" | ".ara"))
                || entry
                    .file_name()
                    .to_str()
                    .is_some_and(ara_core::write::source::is_temporary_path)
            {
                continue;
            }
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|error| AgentError::io(format!("{}: {error}", path.display())))?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() || kind.is_symlink() {
                let relative = path
                    .strip_prefix(root)
                    .expect("child of artifact root")
                    .to_str()
                    .ok_or_else(|| AgentError::io("Artifact file paths must be UTF-8"))?
                    .to_owned();
                let bytes = entry
                    .metadata()
                    .map_err(|error| AgentError::io(format!("{}: {error}", path.display())))?
                    .len();
                files.push(ArtifactFile {
                    path: relative,
                    bytes,
                    kind: if kind.is_symlink() { "symlink" } else { "file" },
                });
            } else {
                return Err(AgentError::io(format!(
                    "Unsupported special file: {}",
                    path.display()
                )));
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}
pub fn refs(root: &Path, args: &IdArgs) -> Result<Value, AgentError> {
    let artifact = Artifact::load(root)?;
    let target = artifact.entry(&args.id)?;
    let structured = references::structured(&artifact, target)?;
    let mut prose = Vec::new();
    for (source, text) in &artifact.sources {
        if !artifact.is_knowledge(source) {
            continue;
        }
        for token in scan_tokens(text).into_iter().filter(|token| {
            (token_may_refer(token.literal, target.key())
                || matches!(target, Entry::Claim(_))
                    && artifact
                        .claim_redirects
                        .get(token.literal)
                        .is_some_and(|current| current == target.key()))
                && !structured
                    .ranges
                    .get(source)
                    .and_then(|ranges| ranges.range(..=(token.range.start, usize::MAX)).next_back())
                    .is_some_and(|(start, end)| {
                        *start <= token.range.start && *end >= token.range.end
                    })
        }) {
            let start = text[..token.range.start]
                .char_indices()
                .rev()
                .nth(40)
                .map_or(0, |(i, _)| i);
            let end = text[token.range.end..]
                .char_indices()
                .nth(80)
                .map_or(text.len(), |(i, _)| token.range.end + i);
            prose.push(json!({"source":source,"field":"source_text","literal":token.literal,"range":token.range,"certainty":"possible","context":excerpt(&text[start..end])}));
        }
    }
    Ok(
        json!({"format":"ara.refs/v1","target":target.key(),"structured":structured.rows,"prose":prose,"diagnostics":diagnostics(&artifact.report)}),
    )
}
pub fn open(root: &Path, options: &ReadOptions) -> Result<Value, AgentError> {
    let artifact = Artifact::load(root)?;
    let index = QueryIndex::new(&artifact.manifest);
    let mut rows = Vec::new();
    let history = SessionHistory::new(&artifact.manifest);
    for entry in artifact.entries() {
        let mut reasons = Vec::new();
        match entry {
            Entry::Node(n)
                if n.kind == NodeKind::Question && index.children(n.id.as_str()).is_empty() =>
            {
                reasons.push("childless_question")
            }
            Entry::Claim(c) if c.status.as_deref() == Some("hypothesis") => {
                reasons.push("hypothesis_claim")
            }
            Entry::Observation(o) => {
                if o.promoted != Some(true) {
                    reasons.push("unpromoted_observation");
                }
                if o.stale == Some(true) || history.stale(o.id.as_str(), o.timestamp.as_deref()) {
                    reasons.push("stale_observation");
                }
            }
            _ => {}
        }
        let value = entry.value(true);
        if contains_pending(&value) {
            reasons.push("pending_binding");
        }
        if !reasons.is_empty() {
            let mut value = entry.value(options.full);
            value
                .as_object_mut()
                .unwrap()
                .insert("reasons".into(), json!(reasons));
            rows.push(value);
        }
    }
    Ok(json!({"format":"ara.open/v1","items":rows,"diagnostics":diagnostics(&artifact.report)}))
}
fn contains_pending(value: &Value) -> bool {
    match value {
        Value::String(s) => s.contains("[pending]") || s == "pending",
        Value::Array(v) => v.iter().any(contains_pending),
        Value::Object(o) => o.values().any(contains_pending),
        _ => false,
    }
}
struct SessionHistory<'a> {
    days: Vec<&'a str>,
    latest: BTreeMap<&'a str, &'a str>,
}
impl<'a> SessionHistory<'a> {
    fn new(manifest: &'a Manifest) -> Self {
        let mut days = BTreeSet::new();
        let mut latest = BTreeMap::<&str, &str>::new();
        for session in &manifest.sessions {
            if let Some(day) = session.date.as_deref() {
                days.insert(day);
                for token in scan_tokens(&session.body) {
                    latest
                        .entry(token.literal)
                        .and_modify(|previous| *previous = (*previous).max(day))
                        .or_insert(day);
                }
            }
        }
        Self {
            days: days.into_iter().collect(),
            latest,
        }
    }
    fn stale(&self, id: &str, created: Option<&str>) -> bool {
        let Some(created) = created.and_then(|date| date.get(..10)) else {
            return false;
        };
        let last = self.latest.get(id).copied().unwrap_or(created).max(created);
        self.days.len() - self.days.partition_point(|day| *day <= last) >= 3
    }
}
pub fn validate_date(date: &str) -> Result<(), AgentError> {
    let valid = (|| {
        let bytes = date.as_bytes();
        if !bytes.is_ascii() || bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return false;
        }
        let year = date[..4].parse::<u32>().ok();
        let month = date[5..7].parse::<u32>().ok();
        let day = date[8..].parse::<u32>().ok();
        let (Some(year), Some(month), Some(day)) = (year, month, day) else {
            return false;
        };
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                    29
                } else {
                    28
                }
            }
            _ => 0,
        };
        year > 0 && day > 0 && day <= days
    })();
    if valid {
        Ok(())
    } else {
        Err(AgentError::semantic(
            "invalid_date",
            format!("Invalid calendar date `{date}`; expected YYYY-MM-DD"),
        ))
    }
}
pub fn knowledge_document(path: &str) -> bool {
    path == "PAPER.md"
        || path == "rubric/requirements.md"
        || path.starts_with("logic/") && path.ends_with(".md")
        || path.starts_with("trace/") && path.ends_with(".yaml")
        || path == "staging/observations.yaml"
}
fn valid_document_path(document: &str) -> bool {
    !document.contains('\\')&&Path::new(document).components().all(|component|matches!(component,std::path::Component::Normal(name) if name!=".git"&&name!=".ara"))
}
fn show_source(root: &Path, document: &str, headings: &[String]) -> Result<Value, AgentError> {
    if !valid_document_path(document) {
        return Err(AgentError::semantic(
            "invalid_document",
            "Document outside the knowledge boundary",
        ));
    }
    if !knowledge_document(document) {
        let paper_path = ara_core::write::transaction::checked_destination(root, "PAPER.md")
            .map_err(crate::write::convert_error)?;
        let paper = std::fs::read_to_string(paper_path)
            .map_err(|error| AgentError::io(error.to_string()))?;
        if !ara_core::knowledge_paths(&paper)
            .map_err(|message| AgentError::semantic("invalid_knowledge_registry", message))?
            .iter()
            .any(|path| path == document)
        {
            return Err(AgentError::semantic(
                "invalid_document",
                "Document outside the knowledge boundary",
            ));
        }
    }
    let path = ara_core::write::transaction::checked_destination(root, document)
        .map_err(crate::write::convert_error)?;
    let text = std::fs::read_to_string(path).map_err(|error| AgentError::io(error.to_string()))?;
    let mut value = source_output(document, headings, true, &text)?;
    value
        .as_object_mut()
        .unwrap()
        .insert("artifact_validation".into(), json!("not_run"));
    Ok(value)
}
fn show_document(
    artifact: &Artifact,
    document: &str,
    headings: &[String],
    source: bool,
    full: bool,
) -> Result<Value, AgentError> {
    if !valid_document_path(document) || !artifact.is_knowledge(document) {
        return Err(AgentError::semantic(
            "invalid_document",
            "Document outside the knowledge boundary",
        ));
    }
    let direct = artifact
        .sources
        .get(document)
        .map(|text| source_output(document, headings, source || full, text))
        .unwrap_or_else(|| Err(AgentError::unknown(document)));
    let mut value = match direct {
        Ok(value) => value,
        Err(error) if !source && !headings.is_empty() => {
            let selector = ara_core::write::EntrySelector::Document {
                document: document.into(),
                heading: headings.to_vec(),
                entry: None,
            };
            let resolved = ara_core::merge::resolve_selector(artifact.snapshot()?, &selector)
                .map_err(crate::merge::convert_error)?;
            if resolved == selector {
                return Err(error);
            }
            let (current_document, current_headings) = match resolved {
                ara_core::write::EntrySelector::Document {
                    document,
                    heading,
                    entry,
                } => {
                    let heading = if heading.is_empty() {
                        entry.into_iter().collect()
                    } else {
                        heading
                    };
                    (document, heading)
                }
                ara_core::write::EntrySelector::Id { id } if id.starts_with('C') => {
                    ("logic/claims.md".into(), vec![id])
                }
                ara_core::write::EntrySelector::Id { id } if id.starts_with('H') => {
                    ("logic/solution/heuristics.md".into(), vec![id])
                }
                _ => return Err(error),
            };
            let text = artifact
                .sources
                .get(&current_document)
                .ok_or_else(|| AgentError::unknown(&current_document))?;
            source_output(&current_document, &current_headings, full, text)?
        }
        Err(error) => return Err(error),
    };
    value
        .as_object_mut()
        .unwrap()
        .insert("diagnostics".into(), diagnostics(&artifact.report));
    Ok(value)
}
fn heading_matches(actual: &str, wanted: &str) -> bool {
    actual == wanted
        || (wanted
            .as_bytes()
            .first()
            .is_some_and(|prefix| matches!(prefix, b'N' | b'C' | b'H' | b'E' | b'O' | b'T'))
            && wanted.len() > 1
            && wanted[1..].bytes().all(|byte| byte.is_ascii_digit())
            && actual
                .split_once(':')
                .is_some_and(|(id, _)| id.trim() == wanted))
}
fn source_output(
    document: &str,
    headings: &[String],
    full: bool,
    text: &str,
) -> Result<Value, AgentError> {
    let content = if headings.is_empty() {
        text
    } else {
        let selected = ara_core::markdown::headings(text)
            .into_iter()
            .filter(|section| {
                section.path.len() >= headings.len()
                    && section.path[section.path.len() - headings.len()..]
                        .iter()
                        .zip(headings)
                        .all(|(actual, wanted)| heading_matches(actual, wanted))
            })
            .collect::<Vec<_>>();
        if selected.len() != 1 {
            return Err(AgentError::unknown(&headings.join(" / ")));
        }
        &text[selected[0].body_range.clone()]
    };
    use sha2::{Digest, Sha256};
    let digest = format!("sha256:{:x}", Sha256::digest(content.as_bytes()));
    Ok(
        json!({"format":"ara.show/v1","entries":[{"key":document,"kind":"source_document","document":document,"heading":headings,"source":document,"content":if full{content.to_owned()}else{excerpt(content)},"digest":digest}]}),
    )
}

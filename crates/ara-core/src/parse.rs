//! Parse + normalize `trace/exploration_tree.yaml` (+ optional
//! `logic/claims.md`) into a [`Manifest`].
//!
//! [`parse_sources`] is pure and wasm-safe (no threads, filesystem, or
//! `SystemTime`). [`parse_dir`] is a thin native wrapper that reads the two
//! files and delegates. Determinism comes from **preserving input order**:
//! nodes are pre-order DFS, links/bindings follow source order. Nothing is
//! sorted by id.

use std::collections::{BTreeMap, BTreeSet};

use crate::claims::parse_claims;
use crate::manifest::{
    Binding, BindingRole, ClaimId, Link, LinkKind, Manifest, Node, NodeArtifact, NodeFields,
    NodeId, NodeKind, SourceValue, is_canonical_id,
};
use crate::report::ParseReport;
use crate::rules::RuleCode;
use crate::schema::{RawNode, parse_doc};

#[allow(
    clippy::large_enum_variant,
    reason = "boxing Manifest would add a heap allocation to every successful parse"
)]
pub(crate) enum ParseOutcome {
    Normalized(Manifest, ParseReport),
    Fatal(ParseReport),
}

/// Parses in-memory sources into a [`Manifest`]. Pure and wasm-safe.
///
/// `claims_md = None` means claim references cannot be resolved: each `C##`
/// evidence reference becomes an **unresolved-binding warning** (not an error),
/// and no bindings are produced.
///
/// Returns `Ok((manifest, report))` when there are no errors (the report may
/// still carry warnings that callers must surface), or `Err(report)` otherwise.
pub fn parse_sources(
    tree_yaml: &str,
    claims_md: Option<&str>,
) -> Result<(Manifest, ParseReport), ParseReport> {
    match parse_sources_detailed(tree_yaml, claims_md) {
        ParseOutcome::Normalized(manifest, report) if report.is_ok() => Ok((manifest, report)),
        ParseOutcome::Normalized(_, report) | ParseOutcome::Fatal(report) => Err(report),
    }
}

/// Normalize known claim pointers through an explicitly supplied native redirect
/// ledger. Source prose and stored claim bodies remain unchanged.
pub fn parse_sources_with_claim_redirects(
    tree_yaml: &str,
    claims_md: Option<&str>,
    redirects: &BTreeMap<String, String>,
) -> Result<(Manifest, ParseReport), ParseReport> {
    match parse_sources_detailed_with_claim_redirects(tree_yaml, claims_md, redirects) {
        ParseOutcome::Normalized(manifest, report) if report.is_ok() => Ok((manifest, report)),
        ParseOutcome::Normalized(_, report) | ParseOutcome::Fatal(report) => Err(report),
    }
}

pub(crate) fn parse_sources_detailed(tree_yaml: &str, claims_md: Option<&str>) -> ParseOutcome {
    parse_sources_detailed_with_claim_redirects(tree_yaml, claims_md, &BTreeMap::new())
}
pub(crate) fn parse_sources_detailed_with_claim_redirects(
    tree_yaml: &str,
    claims_md: Option<&str>,
    redirects: &BTreeMap<String, String>,
) -> ParseOutcome {
    let mut report = ParseReport::default();

    let doc = match parse_doc(tree_yaml) {
        Ok(doc) => doc,
        Err(msg) => {
            report.error(RuleCode::MalformedTree, "document", msg);
            return ParseOutcome::Fatal(report);
        }
    };

    for key in doc.extra.keys() {
        report.warn(
            RuleCode::UnknownDocumentField,
            "document",
            format!("unknown field `{key}`"),
        );
    }

    let roots: Vec<RawNode> = match (doc.tree, doc.root) {
        (Some(_), Some(_)) => {
            report.error(
                RuleCode::TreeAndRoot,
                "document",
                "both `tree:` and `root:` are present; exactly one is allowed",
            );
            return ParseOutcome::Fatal(report);
        }
        (None, None) => {
            report.error(
                RuleCode::MissingTree,
                "document",
                "neither `tree:` nor `root:` is present",
            );
            return ParseOutcome::Fatal(report);
        }
        (Some(tree), None) => {
            if tree.is_empty() {
                report.warn(
                    RuleCode::EmptyTree,
                    "document",
                    "empty manifest (`tree: []`)",
                );
            }
            tree
        }
        (None, Some(root)) => vec![*root],
    };

    // Claims resolve node→claim and claim→claim references.
    let claims_present = claims_md.is_some();
    let (mut claims, duplicate_claim_ids) = match claims_md {
        Some(md) => {
            let parsed = parse_claims(md);
            (parsed.claims, parsed.duplicate_ids)
        }
        None => (Vec::new(), Vec::new()),
    };
    let claim_ids: BTreeSet<ClaimId> = claims.iter().map(|c| c.id.clone()).collect();
    for id in duplicate_claim_ids {
        report.error(
            RuleCode::DuplicateClaimId,
            format!("claims[{id}]"),
            "duplicate claim id",
        );
    }

    let mut resolved = BTreeMap::new();
    for (from, to) in redirects {
        if !is_canonical_id(from, 'C')
            || !is_canonical_id(to, 'C')
            || claim_ids.contains(from.as_str())
        {
            report.error(
                RuleCode::InvalidClaimRedirect,
                "document.claim_redirects",
                format!("Invalid or resurrected claim redirect `{from}` → `{to}`"),
            );
            return ParseOutcome::Fatal(report);
        }
    }
    for (from, to) in redirects {
        if resolved.contains_key(from.as_str()) {
            continue;
        }
        let mut current = to.as_str();
        let mut visited = BTreeSet::from([from.as_str()]);
        while let Some(next) = redirects.get(current) {
            if let Some(cached) = resolved.get(current) {
                current = *cached;
                break;
            }
            if !visited.insert(current) {
                report.error(
                    RuleCode::InvalidClaimRedirect,
                    "document.claim_redirects",
                    "Claim redirect cycle",
                );
                return ParseOutcome::Fatal(report);
            }
            current = next;
        }
        if !claim_ids.contains(current) {
            report.error(
                RuleCode::InvalidClaimRedirect,
                "document.claim_redirects",
                format!("Claim redirect `{from}` has no live destination `{current}`"),
            );
            return ParseOutcome::Fatal(report);
        }
        for origin in visited {
            resolved.insert(origin, current);
        }
    }
    for claim in &mut claims {
        for dep in &mut claim.deps {
            if let Some(target) = resolved.get(dep.as_str()) {
                dep.redirect(target);
            }
        }
    }
    let mut norm = Normalizer {
        report,
        claims_present,
        claim_ids,
        claim_redirects: &resolved,
        nodes: Vec::new(),
        node_ids: BTreeSet::new(),
        bindings: Vec::new(),
        child_links: Vec::new(),
        also: Vec::new(),
    };
    let mut pending = roots
        .iter()
        .rev()
        .map(|raw| (raw, None))
        .collect::<Vec<_>>();
    while let Some((raw, parent)) = pending.pop() {
        if norm.emit(raw, parent) {
            let parent = raw.id.as_deref().map(str::trim);
            pending.extend(raw.children.iter().rev().map(|child| (child, parent)));
        }
    }
    validate_same_as(&norm.nodes, &norm.node_ids, &mut norm.report);
    norm.child_links.retain(|link| {
        if !norm.node_ids.contains(&link.from) {
            norm.report.error(
                RuleCode::UnknownParentNode,
                format!("nodes[{}].parent", link.to),
                format!("`parent` references unknown node `{}`", link.from),
            );
            false
        } else {
            true
        }
    });

    // Resolve `also_depends_on` (needs the full node-id set), then combine and
    // dedupe links.
    //
    // A dependency whose target is an *ancestor* of the source (reachable by
    // walking `children:` nesting upward) merely restates the nesting: it is
    // redundant, and combined with the parent→child edge it would close a cycle
    // and fail the whole artifact. Real traces do this (a child re-declaring a
    // dependency on its parent), so drop the redundant edge with a warning
    // instead. Genuine cross-cycles — a dependency on a sibling or descendant
    // that closes a loop — are not ancestors and stay fatal in `detect_cycles`.
    let parent_of: BTreeMap<NodeId, NodeId> = norm
        .child_links
        .iter()
        .map(|l| (l.to.clone(), l.from.clone()))
        .collect();
    let mut depends_links: Vec<Link> = Vec::new();
    for (from, targets) in &norm.also {
        for (i, target) in targets.iter().enumerate() {
            let t = target.trim();
            let to = NodeId::new(t);
            if !norm.node_ids.contains(&to) {
                norm.report.error(
                    RuleCode::UnknownDependencyNode,
                    format!("nodes[{from}].also_depends_on[{i}]"),
                    format!("`also_depends_on` references unknown node `{t}`"),
                );
                continue;
            }
            if is_ancestor(&to, from, &parent_of) {
                norm.report.warn(
                    RuleCode::RedundantAncestorDependency,
                    format!("nodes[{from}].also_depends_on[{i}]"),
                    format!(
                        "redundant `also_depends_on` on ancestor `{t}` (already nested under it)"
                    ),
                );
                continue;
            }
            depends_links.push(Link {
                from: from.clone(),
                to,
                kind: LinkKind::DependsOn,
            });
        }
    }

    let mut links = norm.child_links;
    links.extend(depends_links);
    let links = dedupe_links(links, &mut norm.report);

    detect_cycles(&norm.nodes, &links, &mut norm.report);

    // Resolve claim→claim dependencies.
    for claim in &claims {
        for (i, dep) in claim.deps.iter().enumerate() {
            if !norm.claim_ids.contains(dep) {
                norm.report.error(
                    RuleCode::UnknownClaimDependency,
                    format!("claims[{}].dependencies[{i}]", claim.id),
                    format!("dependency references unknown claim `{dep}`"),
                );
            }
        }
    }

    let manifest = Manifest {
        nodes: norm.nodes,
        links,
        bindings: norm.bindings,
        claims,
        bounds: None,
        paper: None,
        related_work: Vec::new(),
        concepts: Vec::new(),
        problem: None,
        recipes: Vec::new(),
        exhibits: Vec::new(),
        built_on: Vec::new(),
        node_exhibits: Vec::new(),
        observations: Vec::new(),
        sessions: Vec::new(),
        heuristics: Vec::new(),
        experiment_plans: Vec::new(),
        taste_comments: Vec::new(),
    };

    ParseOutcome::Normalized(manifest, norm.report)
}

/// Reads `trace/exploration_tree.yaml` (required) and `logic/claims.md`
/// (optional) from `dir` and normalizes them, then augments the manifest with
/// the optional logic-section files (`PAPER.md`, `logic/problem.md`,
/// `logic/concepts.md`, `logic/related_work.md`, `logic/solution/*.md`). An
/// absent section file is silently skipped; a present-but-malformed one adds a
/// warning without failing the parse. Native only.
#[cfg(feature = "native")]
pub fn parse_dir(dir: &std::path::Path) -> Result<(Manifest, ParseReport), ParseReport> {
    let load = parse_dir_detailed(dir);
    match load.manifest {
        Some(manifest) if load.report.is_ok() => Ok((manifest, load.report)),
        _ => Err(load.report),
    }
}

/// Native load diagnostics distinguish absence from failed reads without
/// changing the existing validate report's serialized shape.
#[cfg(feature = "native")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadIssueKind {
    Missing,
    Unreadable,
}

#[cfg(feature = "native")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LoadIssue {
    pub path: String,
    pub kind: LoadIssueKind,
    pub message: String,
}

#[cfg(feature = "native")]
#[derive(Debug, Default)]
pub struct NativeLoad {
    /// None when the base parse could not retain every node/claim.
    pub manifest: Option<Manifest>,
    pub report: ParseReport,
    pub io_issues: Vec<LoadIssue>,
    /// Exact UTF-8 source files, keyed by artifact-relative path.
    pub sources: BTreeMap<String, String>,
    /// Exact audited native claim origins and their destinations.
    pub claim_redirects: BTreeMap<String, String>,
}

#[cfg(feature = "native")]
impl NativeLoad {
    pub(crate) fn read_source(
        &mut self,
        dir: &std::path::Path,
        file: &str,
        required: bool,
    ) -> Option<String> {
        if let Some(source) = self.sources.get(file) {
            return Some(source.clone());
        }
        match std::fs::read_to_string(dir.join(file)) {
            Ok(source) => {
                self.sources.insert(file.to_string(), source.clone());
                Some(source)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => None,
            Err(error) => {
                self.io_error(file, error, required);
                None
            }
        }
    }
    pub(crate) fn io_error(&mut self, file: &str, error: std::io::Error, required: bool) {
        let kind = if error.kind() == std::io::ErrorKind::NotFound {
            LoadIssueKind::Missing
        } else {
            LoadIssueKind::Unreadable
        };
        let message = format!("cannot read {file}: {error}");
        if required {
            self.report
                .error(RuleCode::UnreadableTree, "document", &message);
        } else {
            self.report
                .warn(RuleCode::UnreadableOptionalLayer, file, &message);
        }
        self.io_issues.push(LoadIssue {
            path: file.to_string(),
            kind,
            message,
        });
    }
    pub(crate) fn list_files(&mut self, dir: &std::path::Path, subdir: &str) -> Vec<String> {
        let entries = match std::fs::read_dir(dir.join(subdir)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(e) => {
                self.io_error(subdir, e, false);
                return Vec::new();
            }
        };
        let mut paths = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => {
                    paths.push(format!("{subdir}/{}", entry.file_name().to_string_lossy()))
                }
                Err(e) => self.io_error(subdir, e, false),
            }
        }
        paths.sort();
        paths
    }
    fn read_remaining_logic(&mut self, dir: &std::path::Path) {
        let mut directories = vec![String::from("logic")];
        while let Some(directory) = directories.pop() {
            for file in self.list_files(dir, &directory) {
                match std::fs::symlink_metadata(dir.join(&file)) {
                    Ok(metadata) if metadata.is_dir() => directories.push(file),
                    Ok(metadata)
                        if metadata.is_file()
                            && file.ends_with(".md")
                            && !self.sources.contains_key(&file) =>
                    {
                        self.read_source(dir, &file, false);
                    }
                    Ok(_) => {}
                    Err(e) => self.io_error(&file, e, false),
                }
            }
        }
    }
    fn read_knowledge_registry(&mut self, dir: &std::path::Path) {
        self.read_registered_source(dir, "rubric/requirements.md", false);
        let paths = match self
            .sources
            .get("PAPER.md")
            .map(|paper| crate::agent_layers::knowledge_paths(paper))
            .transpose()
        {
            Ok(paths) => paths.unwrap_or_default(),
            Err(e) => {
                self.report
                    .warn(RuleCode::MalformedAgentLayer, "PAPER.md.knowledge_paths", e);
                return;
            }
        };
        for file in paths {
            self.read_registered_source(dir, &file, true);
        }
    }
    fn read_registered_source(&mut self, dir: &std::path::Path, file: &str, declared: bool) {
        let mut path = dir.to_path_buf();
        for part in file.split('/') {
            path.push(part);
            match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    self.io_error(
                        file,
                        std::io::Error::other("symlink in knowledge path"),
                        false,
                    );
                    return;
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && !declared => return,
                Err(e) => {
                    self.io_error(file, e, false);
                    return;
                }
            }
        }
        if !self.sources.contains_key(file) {
            self.read_source(dir, file, false);
        }
    }
}

/// Retain a complete logical graph even when reference checks fail, but never
/// expose a duplicate/missing-id-truncated graph as a complete read result.
#[cfg(feature = "native")]
pub fn parse_dir_detailed(dir: &std::path::Path) -> NativeLoad {
    let mut load = NativeLoad::default();
    let Some(tree) = load.read_source(dir, "trace/exploration_tree.yaml", true) else {
        return load;
    };
    let claims = load.read_source(dir, "logic/claims.md", false);
    if load
        .read_source(dir, "trace/logic_mutations.yaml", false)
        .is_some()
    {
        load.read_source(dir, "trace/merge_log.yaml", false);
        for path in load
            .list_files(dir, "trace/sessions")
            .into_iter()
            .filter(|path| path.ends_with(".yaml") || path.ends_with(".yml"))
        {
            load.read_source(dir, &path, false);
        }
        match crate::write::logic::claim_redirects_from_sources(&load.sources) {
            Ok(redirects) => load.claim_redirects = redirects,
            Err(error) => {
                load.report.error(
                    RuleCode::InvalidClaimRedirect,
                    "trace/logic_mutations.yaml",
                    error.to_string(),
                );
                return load;
            }
        }
    }
    let (mut manifest, report) = match parse_sources_detailed_with_claim_redirects(
        &tree,
        claims.as_deref(),
        &load.claim_redirects,
    ) {
        ParseOutcome::Normalized(manifest, report) => (manifest, report),
        ParseOutcome::Fatal(report) => {
            load.report.append(report);
            return load;
        }
    };
    load.report.append(report);
    let complete = !load.report.errors().iter().any(|d| {
        matches!(
            d.code,
            RuleCode::MissingNodeId | RuleCode::DuplicateNodeId | RuleCode::DuplicateClaimId
        )
    });
    read_logic_layer(dir, &mut manifest, &mut load);
    read_evidence_layer(dir, &mut manifest, &mut load);
    crate::agent_layers::read_layers(dir, &mut load, &mut manifest);
    load.read_remaining_logic(dir);
    load.read_knowledge_registry(dir);
    let terms: BTreeSet<&str> = manifest.concepts.iter().map(|c| c.term.as_str()).collect();
    for node in &manifest.nodes {
        for concept in &node.concepts {
            let term = concept
                .strip_prefix("logic/concepts.md#")
                .unwrap_or(concept);
            if !terms.contains(term) {
                load.report.warn(
                    RuleCode::UnknownNodeConcept,
                    format!("nodes[{}].concepts", node.id),
                    format!("unknown concept term `{concept}`"),
                );
            }
        }
    }
    if complete {
        load.manifest = Some(manifest);
    }
    load
}

/// Reads the optional `evidence/` layer into `manifest.exhibits`, then runs the
/// two deterministic resolution passes (`node_exhibits`, `built_on`) over the
/// assembled manifest. Absent evidence yields empty fields; every defect warns
/// but never errors. Native only.
#[cfg(feature = "native")]
fn read_evidence_layer(dir: &std::path::Path, manifest: &mut Manifest, load: &mut NativeLoad) {
    use crate::evidence::{read_evidence_detailed, resolve_built_on, resolve_node_exhibits};

    manifest.exhibits = read_evidence_detailed(dir, load);
    manifest.node_exhibits =
        resolve_node_exhibits(&manifest.nodes, &manifest.bindings, &manifest.exhibits);
    manifest.built_on =
        resolve_built_on(&manifest.nodes, &manifest.bindings, &manifest.related_work);
}

/// Reads the optional logic-section files into `manifest`, appending reader
/// warnings to `report`. Absent files are skipped without a warning; a present
/// file that parses degenerately warns but never errors.
#[cfg(feature = "native")]
fn read_logic_layer(dir: &std::path::Path, manifest: &mut Manifest, load: &mut NativeLoad) {
    use crate::paper::parse_paper;
    use crate::sections::{parse_concepts, parse_problem, parse_related_work};

    if let Some(md) = load.read_source(dir, "PAPER.md", false) {
        let (paper, warnings) = parse_paper(&md);
        manifest.paper = paper;
        for w in warnings {
            load.report
                .warn(RuleCode::MalformedPaperFrontmatter, "PAPER.md", w);
        }
    }

    if let Some(md) = load.read_source(dir, "logic/problem.md", false) {
        manifest.problem = Some(parse_problem(&md));
    }

    if let Some(md) = load.read_source(dir, "logic/concepts.md", false) {
        let concepts = parse_concepts(&md);
        for c in &concepts {
            if c.definition.is_none() {
                load.report.warn(
                    RuleCode::ConceptMissingDefinition,
                    format!("concepts[{}]", c.term),
                    "concept has no definition",
                );
            }
        }
        manifest.concepts = concepts;
    }

    if let Some(md) = load.read_source(dir, "logic/related_work.md", false) {
        let related_work = parse_related_work(&md);
        for r in &related_work {
            if r.doi.is_none() {
                load.report.warn(
                    RuleCode::RelatedWorkMissingDoi,
                    format!("related_work[{}]", r.id),
                    "related work has no DOI",
                );
            }
        }
        manifest.related_work = related_work;
    }

    manifest.recipes = read_recipes(dir, load);
}

/// Enumerates `logic/solution/*.md` sorted by path (determinism) and builds one
/// [`crate::manifest::Recipe`] per file: filename stem, first `# Title`, and the
/// verbatim body. A missing directory yields no recipes.
#[cfg(feature = "native")]
fn read_recipes(dir: &std::path::Path, load: &mut NativeLoad) -> Vec<crate::manifest::Recipe> {
    let mut recipes = Vec::new();
    for file in load.list_files(dir, "logic/solution") {
        if !file.ends_with(".md") {
            continue;
        }
        let Some(body) = load.read_source(dir, &file, false) else {
            continue;
        };
        let name = std::path::Path::new(&file)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let title = crate::paper::first_h1(&body);
        recipes.push(crate::manifest::Recipe { name, title, body });
    }
    recipes
}

fn annotation_strings(
    value: Option<&SourceValue>,
    id: &NodeId,
    field: &str,
    code: RuleCode,
    strict_shape: bool,
    report: &mut ParseReport,
) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let malformed = |report: &mut ParseReport, message: &str| {
        if strict_shape {
            report.error(
                RuleCode::MalformedTree,
                format!("nodes[{id}].{field}"),
                message,
            );
        } else {
            report.warn(code, format!("nodes[{id}].{field}"), message);
        }
    };
    let SourceValue::Sequence(items) = value else {
        malformed(report, "expected a sequence");
        return Vec::new();
    };
    let mut seen = BTreeSet::new();
    items
        .iter()
        .filter_map(|value| match value {
            SourceValue::String(text) => {
                if !seen.insert(text) {
                    report.warn(
                        code,
                        format!("nodes[{id}].{field}"),
                        "duplicate annotation (retained)",
                    );
                }
                Some(text.clone())
            }
            _ => {
                malformed(report, "annotation must be text");
                None
            }
        })
        .collect()
}

fn annotation_artifacts(
    value: Option<&SourceValue>,
    id: &NodeId,
    report: &mut ParseReport,
) -> Vec<NodeArtifact> {
    let Some(value) = value else {
        return Vec::new();
    };
    let SourceValue::Sequence(items) = value else {
        report.error(
            RuleCode::MalformedTree,
            format!("nodes[{id}].artifacts"),
            "expected artifact sequence",
        );
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|value| {
            if let SourceValue::Mapping(map) = value {
                let text = |key| match map.get(key) {
                    None => Some(""),
                    Some(SourceValue::String(value)) => Some(value.as_str()),
                    _ => None,
                };
                if let (Some(name), Some(pointer), Some(what)) =
                    (text("name"), text("pointer"), text("what"))
                {
                    let extra = map
                        .iter()
                        .filter(|(key, _)| !matches!(key.as_str(), "name" | "pointer" | "what"))
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect();
                    return Some(NodeArtifact {
                        name: name.into(),
                        pointer: pointer.into(),
                        what: what.into(),
                        extra,
                    });
                }
            }
            report.error(
                RuleCode::MalformedTree,
                format!("nodes[{id}].artifacts"),
                "artifact entry must be a mapping with text fields",
            );
            None
        })
        .collect()
}

fn validate_same_as(nodes: &[Node], ids: &BTreeSet<NodeId>, report: &mut ParseReport) {
    if nodes.iter().all(|n| n.same_as.is_empty()) {
        return;
    }
    let mut incoming: BTreeMap<&NodeId, usize> = nodes.iter().map(|n| (&n.id, 0)).collect();
    let by_id: BTreeMap<&NodeId, &Node> = nodes.iter().map(|n| (&n.id, n)).collect();
    for node in nodes {
        for target in &node.same_as {
            if target == &node.id {
                report.warn(
                    RuleCode::SelfSameAs,
                    format!("nodes[{}].same_as", node.id),
                    "self same-finding pointer",
                );
            } else if !ids.contains(target) {
                report.warn(
                    RuleCode::DanglingSameAs,
                    format!("nodes[{}].same_as", node.id),
                    format!("unknown same-finding target `{target}`"),
                );
            } else if let Some(count) = incoming.get_mut(target) {
                *count += 1;
            }
        }
    }
    // Kahn's algorithm over the annotation relation only, never dependency edges.
    let mut ready: Vec<&NodeId> = incoming
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect();
    let mut visited = 0;
    while let Some(id) = ready.pop() {
        visited += 1;
        for target in &by_id[id].same_as {
            if target == id {
                continue;
            }
            if let Some(count) = incoming.get_mut(target) {
                *count -= 1;
                if *count == 0 {
                    ready.push(target);
                }
            }
        }
    }
    if visited != nodes.len() {
        report.warn(
            RuleCode::SameAsCycle,
            "document.same_as",
            "directional same-finding cycle",
        );
    }
}

/// Mutable accumulators for the normalization DFS.
struct Normalizer<'a> {
    report: ParseReport,
    claims_present: bool,
    claim_ids: BTreeSet<ClaimId>,
    claim_redirects: &'a BTreeMap<&'a str, &'a str>,
    nodes: Vec<Node>,
    node_ids: BTreeSet<NodeId>,
    bindings: Vec<Binding>,
    child_links: Vec<Link>,
    /// Per emitted node, its raw `also_depends_on` targets (resolved later).
    also: Vec<(NodeId, Vec<String>)>,
}

impl Normalizer<'_> {
    /// Pre-order visit of `raw`, emitting one [`Node`] plus its child link,
    /// bindings, and evidence notes. A missing or duplicate id drops the node
    /// (and its subtree) with an error, rather than corrupting the graph.
    fn emit(&mut self, raw: &RawNode, parent: Option<&str>) -> bool {
        let id_str = raw.id.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let Some(id_str) = id_str else {
            let label = raw.title.as_deref().unwrap_or("<no id>");
            self.report.error(
                RuleCode::MissingNodeId,
                format!("nodes[{label}]"),
                "node is missing an `id`",
            );
            return false;
        };
        let id = NodeId::new(id_str);
        if self.node_ids.contains(&id) {
            self.report.error(
                RuleCode::DuplicateNodeId,
                format!("nodes[{id}]"),
                "duplicate node id",
            );
            return false;
        }
        self.node_ids.insert(id.clone());

        let explicit_parent = raw.parent.as_deref().map(str::trim);
        if let (Some(nested), Some(explicit)) = (parent, explicit_parent)
            && nested != explicit
        {
            self.report.error(
                RuleCode::ConflictingParent,
                format!("nodes[{id}].parent"),
                format!("nested parent `{nested}` disagrees with explicit parent `{explicit}`"),
            );
        }
        if let Some(parent) = parent.or(explicit_parent) {
            self.child_links.push(Link {
                from: NodeId::new(parent),
                to: id.clone(),
                kind: LinkKind::Child,
            });
        }

        let (kind, fields) = self.project_kind(raw, &id);
        let evidence_notes = self.split_evidence(raw, &id);

        for key in raw.extra.keys() {
            self.report.warn(
                RuleCode::UnknownNodeField,
                format!("nodes[{id}]"),
                format!("unknown field `{key}`"),
            );
        }

        self.nodes.push(Node {
            id: id.clone(),
            kind,
            label: raw.title.clone(),
            support_level: raw.support_level.clone(),
            source_refs: raw.source_refs.clone(),
            description: raw.description.clone(),
            thinking: raw.thinking.clone(),
            status: if matches!(&fields, NodeFields::Experiment { .. }) {
                None
            } else {
                raw.status.clone()
            },
            provenance: raw.provenance.clone(),
            timestamp: raw.timestamp.clone(),
            fields,
            evidence_notes,
            same_as: annotation_strings(
                raw.same_as.as_ref(),
                &id,
                "same_as",
                RuleCode::MalformedSameAs,
                false,
                &mut self.report,
            )
            .into_iter()
            .filter_map(|target| {
                if is_canonical_id(&target, 'N') {
                    Some(NodeId::new(target))
                } else {
                    self.report.warn(
                        RuleCode::MalformedSameAs,
                        format!("nodes[{id}].same_as"),
                        format!("noncanonical node id `{target}`"),
                    );
                    None
                }
            })
            .collect(),
            artifacts: annotation_artifacts(raw.artifacts.as_ref(), &id, &mut self.report),
            concepts: annotation_strings(
                raw.concepts.as_ref(),
                &id,
                "concepts",
                RuleCode::MalformedNodeAnnotation,
                true,
                &mut self.report,
            ),
            isolated: raw.isolated,
            pos: None,
        });
        self.also.push((id.clone(), raw.also_depends_on.clone()));

        true
    }

    /// Projects `type:` + body fields into a typed [`NodeKind`]/[`NodeFields`].
    /// Unknown/missing types become [`NodeKind::Other`]; any canonical body
    /// fields carried by a type that does not project them (unknown or known)
    /// are warned so nothing is lost silently.
    fn project_kind(&mut self, raw: &RawNode, id: &NodeId) -> (NodeKind, NodeFields) {
        // `projected` lists the canonical body fields the kind keeps; any other
        // body field present on the node is dropped with a warning below.
        let (kind, fields, ty, projected): (NodeKind, NodeFields, &str, &[&str]) = match raw
            .ty
            .as_deref()
            .map(str::trim)
        {
            Some("question") => (NodeKind::Question, NodeFields::Question, "question", &[]),
            Some("experiment") => (
                NodeKind::Experiment,
                NodeFields::Experiment {
                    result: raw.result.clone(),
                    exploration: raw.exploration.clone(),
                    outcome: raw.outcome.clone(),
                    status: raw.status.clone(),
                },
                "experiment",
                &["result", "exploration", "outcome", "status"],
            ),
            Some("decision") => (
                NodeKind::Decision,
                NodeFields::Decision {
                    choice: raw.choice.clone(),
                    alternatives: raw.alternatives.clone(),
                    rationale: raw.rationale.clone(),
                },
                "decision",
                &["choice", "alternatives", "rationale"],
            ),
            Some("dead_end") => (
                NodeKind::DeadEnd,
                NodeFields::DeadEnd {
                    hypothesis: raw.hypothesis.clone(),
                    failure_mode: raw.failure_mode.clone(),
                    lesson: raw.lesson.clone(),
                    why_failed: raw.why_failed.clone(),
                },
                "dead_end",
                &["hypothesis", "failure_mode", "lesson", "why_failed"],
            ),
            Some("insight") => (NodeKind::Insight, NodeFields::Insight, "insight", &[]),
            Some("pivot") => (
                NodeKind::Pivot,
                NodeFields::Pivot {
                    prior_direction: raw.from.as_ref().or(raw.prior_direction.as_ref()).cloned(),
                    new_direction: raw.to.as_ref().or(raw.new_direction.as_ref()).cloned(),
                    reason: raw.trigger.as_ref().or(raw.reason.as_ref()).cloned(),
                    lesson: raw.lesson.clone(),
                },
                "pivot",
                &[
                    "from",
                    "to",
                    "trigger",
                    "prior_direction",
                    "new_direction",
                    "reason",
                    "lesson",
                ],
            ),
            Some("") | None => {
                self.report.warn(
                    RuleCode::MissingNodeType,
                    format!("nodes[{id}]"),
                    "node is missing a `type`",
                );
                for field in body_field_names(raw) {
                    self.report.warn(
                        RuleCode::FieldDroppedMissingType,
                        format!("nodes[{id}]"),
                        format!("field `{field}` dropped for missing type"),
                    );
                }
                return (NodeKind::Other(String::new()), NodeFields::Other);
            }
            Some(other) => {
                for field in body_field_names(raw) {
                    self.report.warn(
                        RuleCode::FieldDroppedUnknownType,
                        format!("nodes[{id}]"),
                        format!("field `{field}` dropped for unknown type `{other}`"),
                    );
                }
                return (NodeKind::Other(other.to_string()), NodeFields::Other);
            }
        };
        if matches!(kind, NodeKind::Pivot) {
            for (native, legacy, left, right) in [
                ("from", "prior_direction", &raw.from, &raw.prior_direction),
                ("to", "new_direction", &raw.to, &raw.new_direction),
                ("trigger", "reason", &raw.trigger, &raw.reason),
            ] {
                if matches!((left,right),(Some(left),Some(right))if left!=right) {
                    self.report.warn(RuleCode::MalformedAgentLayer,format!("nodes[{id}].{native}"),format!("Conflicting pivot `{native}` and `{legacy}` values; native value projected, both source fields retained"));
                }
            }
        }
        for field in body_field_names(raw) {
            if !projected.contains(&field) {
                self.report.warn(
                    RuleCode::FieldDroppedForType,
                    format!("nodes[{id}]"),
                    format!("field `{field}` dropped for type `{ty}`"),
                );
            }
        }
        (kind, fields)
    }

    /// Splits `evidence:` into `C##` bindings (node→claim) and prose notes.
    fn split_evidence(&mut self, raw: &RawNode, id: &NodeId) -> Vec<String> {
        let mut notes = Vec::new();
        let Some(evidence) = &raw.evidence else {
            return notes;
        };
        for (i, entry) in evidence.entries().iter().enumerate() {
            let trimmed = entry.trim();
            if is_canonical_id(trimmed, 'C') {
                let target = self
                    .claim_redirects
                    .get(trimmed)
                    .copied()
                    .unwrap_or(trimmed);
                let path = format!("nodes[{id}].evidence[{i}]");
                if !self.claims_present {
                    self.report.warn(
                        RuleCode::UnresolvedClaimReference,
                        path,
                        format!("claim reference `{trimmed}` unresolved (no claims.md provided)"),
                    );
                } else if self.claim_ids.contains(target) {
                    self.bindings.push(Binding {
                        node: id.clone(),
                        claim: ClaimId::new(target),
                        role: BindingRole::Evidence,
                    });
                } else {
                    self.report.error(
                        RuleCode::UnknownEvidenceClaim,
                        path,
                        format!("evidence references unknown claim `{trimmed}`"),
                    );
                }
            } else {
                notes.push(entry.clone());
            }
        }
        notes
    }
}

/// Names of canonical body fields present on `raw` (used to warn when a node
/// carries a field its type does not project, so nothing is dropped silently).
fn body_field_names(raw: &RawNode) -> Vec<&'static str> {
    let mut names = Vec::new();
    if raw.result.is_some() {
        names.push("result");
    }
    if raw.exploration.is_some() {
        names.push("exploration");
    }
    if raw.outcome.is_some() {
        names.push("outcome");
    }
    if raw.why_failed.is_some() {
        names.push("why_failed");
    }
    if raw.hypothesis.is_some() {
        names.push("hypothesis");
    }
    if raw.failure_mode.is_some() {
        names.push("failure_mode");
    }
    if raw.lesson.is_some() {
        names.push("lesson");
    }
    if raw.from.is_some() {
        names.push("from");
    }
    if raw.to.is_some() {
        names.push("to");
    }
    if raw.trigger.is_some() {
        names.push("trigger");
    }
    if raw.prior_direction.is_some() {
        names.push("prior_direction");
    }
    if raw.new_direction.is_some() {
        names.push("new_direction");
    }
    if raw.reason.is_some() {
        names.push("reason");
    }
    if raw.choice.is_some() {
        names.push("choice");
    }
    if !raw.alternatives.is_empty() {
        names.push("alternatives");
    }
    if raw.rationale.is_some() {
        names.push("rationale");
    }
    names
}

/// True when `ancestor` lies on the `children:`-nesting chain above `node` —
/// i.e. reachable by walking parent pointers up from `node`. Used to drop
/// redundant `also_depends_on` edges that only restate the nesting.
fn is_ancestor(ancestor: &NodeId, node: &NodeId, parent_of: &BTreeMap<NodeId, NodeId>) -> bool {
    // Explicit resumed branches may contain cycles. Bound the walk; the cycle
    // pass below reports them without an ancestry check hanging first.
    let mut cur = node;
    for _ in 0..=parent_of.len() {
        let Some(parent) = parent_of.get(cur) else {
            break;
        };
        if parent == ancestor {
            return true;
        }
        cur = parent;
    }
    false
}

/// Removes identical `(from, to, kind)` links, keeping the first and warning on
/// each duplicate.
fn dedupe_links(links: Vec<Link>, report: &mut ParseReport) -> Vec<Link> {
    let mut seen: BTreeSet<(NodeId, NodeId, LinkKind)> = BTreeSet::new();
    let mut out = Vec::with_capacity(links.len());
    for link in links {
        let key = (link.from.clone(), link.to.clone(), link.kind);
        if seen.contains(&key) {
            report.warn(
                RuleCode::DuplicateLink,
                format!("nodes[{}]", link.from),
                format!("duplicate {:?} link to `{}`", link.kind, link.to),
            );
        } else {
            seen.insert(key);
            out.push(link);
        }
    }
    out
}

/// Reports a cycle error for every back-edge in the combined
/// `Child` + `DependsOn` graph (DFS three-color).
fn detect_cycles(nodes: &[Node], links: &[Link], report: &mut ParseReport) {
    // BTreeMap (not HashMap) keeps traversal — and thus error ordering — free of
    // any hash-seed influence, matching the crate's determinism guarantee.
    let mut adj: BTreeMap<&NodeId, Vec<&NodeId>> = BTreeMap::new();
    for link in links {
        adj.entry(&link.from).or_default().push(&link.to);
    }
    let mut color: BTreeMap<&NodeId, u8> = BTreeMap::new(); // 0=white, 1=gray, 2=black
    let mut stack = Vec::new();
    for node in nodes {
        if color.get(&node.id).copied().unwrap_or(0) == 0 {
            visit(&node.id, &adj, &mut color, report, &mut stack);
        }
    }
}

fn visit<'a>(
    u: &'a NodeId,
    adj: &BTreeMap<&'a NodeId, Vec<&'a NodeId>>,
    color: &mut BTreeMap<&'a NodeId, u8>,
    report: &mut ParseReport,
    stack: &mut Vec<(&'a NodeId, usize)>,
) {
    color.insert(u, 1);
    stack.push((u, 0));
    while let Some((current, index)) = stack.last_mut() {
        let neighbors = adj.get(current).map(Vec::as_slice).unwrap_or_default();
        if let Some(&next) = neighbors.get(*index) {
            *index += 1;
            match color.get(next).copied().unwrap_or(0) {
                0 => {
                    color.insert(next, 1);
                    stack.push((next, 0));
                }
                1 => report.error(
                    RuleCode::DependencyCycle,
                    format!("nodes[{current}]"),
                    format!("cycle detected: edge to `{next}` closes a cycle"),
                ),
                _ => {}
            }
        } else {
            let (current, _) = stack.pop().expect("visited frame");
            color.insert(current, 2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = "\
tree:
  - id: N01
    type: question
    title: Q?
    children:
      - id: N02
        type: experiment
        result: 28.4 BLEU
        evidence: [C01, \"Table 2\"]
";
    const CLAIMS: &str = "## C01: A claim\n- **Statement**: yes\n";

    #[test]
    fn json_sources_preserve_yaml_normalization_and_annotations() {
        for source in [
            r#"{"tree":[{"id":"N01","type":"question","title":"escaped \u03b1 / \n text","artifacts":[{"pointer":"src/code.py","extra":{"values":[null,true,-2,18446744073709551615]}}],"concepts":["term"],"children":[{"id":"N02","type":"insight","same_as":["N01"]}]}],"future":{"nested":true}}"#,
            r#"{"root":{"id":"N01","type":"future-kind","thinking":"retained","children":null,"same_as":42}}"#,
            r#"{"tree":[{"id":"N01","type":"question","artifacts":[{"extra":{"float":0.10000000000000002,"exponent":1e2,"overflow_integer":18446744073709551616}}]}]}"#,
        ] {
            let json = parse_sources(source, None).expect("JSON-valid YAML");
            let yaml = parse_sources(&format!("---\n{source}"), None).expect("explicit YAML");
            assert_eq!(json, yaml);
        }
    }

    #[test]
    fn json_duplicate_keys_remain_hard_tree_errors() {
        for source in [
            r#"{"tree":[],"tree":[]}"#,
            r#"{"tree":[{"id":"N01","\u0069d":"N02","type":"question"}]}"#,
            r#"{"tree":[{"id":"N01","type":"question","future":1,"future":2}]}"#,
            r#"{"tree":[{"id":"N01","type":"question","artifacts":[{"name":"first","name":"second"}]}]}"#,
            r#"{"tree":[{"id":"N01","type":"question","artifacts":[{"extra":{"key":1,"key":2}}]}]}"#,
        ] {
            let report = parse_sources(source, None).expect_err("duplicate source key");
            assert!(
                report
                    .errors()
                    .iter()
                    .any(|diagnostic| diagnostic.code == RuleCode::MalformedTree)
            );
        }
    }

    #[test]
    fn json_annotation_shapes_keep_errors_and_same_as_warning_separate() {
        for field in [
            r#""concepts":42"#,
            r#""concepts":[42]"#,
            r#""artifacts":42"#,
            r#""artifacts":[42]"#,
            r#""artifacts":[{"name":42}]"#,
        ] {
            let source = format!(r#"{{"tree":[{{"id":"N01","type":"question",{field}}}]}}"#);
            let report = parse_sources(&source, None).expect_err("malformed typed annotation");
            assert!(
                report
                    .errors()
                    .iter()
                    .any(|diagnostic| diagnostic.code == RuleCode::MalformedTree)
            );
        }
        let (manifest, report) = parse_sources(
            r#"{"tree":[{"id":"N01","type":"question","same_as":42,"artifacts":[{"custom":[true,7]}]}]}"#,
            None,
        )
        .expect("same_as remains advisory");
        assert!(
            report
                .warnings()
                .iter()
                .any(|diagnostic| diagnostic.code == RuleCode::MalformedSameAs)
        );
        let artifact = &manifest.nodes[0].artifacts[0];
        assert_eq!(
            (
                artifact.name.as_str(),
                artifact.pointer.as_str(),
                artifact.what.as_str()
            ),
            ("", "", "")
        );
        assert_eq!(
            artifact.extra["custom"],
            SourceValue::Sequence(vec![SourceValue::Bool(true), SourceValue::Integer(7)])
        );
    }

    #[test]
    fn json_resource_checks_include_ignored_values() {
        let source = format!(r#"{{"tree":[],"future":[{}]}}"#, "0,".repeat(250_000) + "0");
        let report = parse_sources(&source, None).expect_err("all source nodes count");
        assert!(
            report
                .errors()
                .iter()
                .any(|diagnostic| diagnostic.code == RuleCode::MalformedTree)
        );
    }

    #[test]
    fn detailed_outcome_classifies_parser_trust_boundary() {
        #[derive(Debug, PartialEq)]
        enum ExpectedOutcome {
            Normalized,
            Fatal,
        }

        let cases = [
            (
                "clean",
                "tree:\n  - id: N01\n    type: question\n",
                ExpectedOutcome::Normalized,
                true,
                Some(1),
            ),
            (
                "semantic duplicate id",
                "tree:\n  - id: N01\n    type: question\n  - id: N01\n    type: insight\n",
                ExpectedOutcome::Normalized,
                false,
                Some(1),
            ),
            (
                "malformed yaml",
                "tree: not-a-list\n",
                ExpectedOutcome::Fatal,
                false,
                None,
            ),
            (
                "both tree and root",
                "tree: []\nroot:\n  id: N01\n",
                ExpectedOutcome::Fatal,
                false,
                None,
            ),
            (
                "neither root",
                "meta: hi\n",
                ExpectedOutcome::Fatal,
                false,
                None,
            ),
        ];

        for (name, yaml, expected_outcome, expected_public_ok, expected_node_count) in cases {
            let (actual_outcome, detailed_report, node_count) =
                match parse_sources_detailed(yaml, None) {
                    ParseOutcome::Normalized(manifest, report) => (
                        ExpectedOutcome::Normalized,
                        report,
                        Some(manifest.nodes.len()),
                    ),
                    ParseOutcome::Fatal(report) => (ExpectedOutcome::Fatal, report, None),
                };
            assert_eq!(actual_outcome, expected_outcome, "{name}");
            assert_eq!(node_count, expected_node_count, "{name}");

            match (parse_sources(yaml, None), expected_public_ok) {
                (Ok((_, public_report)), true) => {
                    assert_eq!(public_report, detailed_report, "{name}");
                }
                (Err(public_report), false) => {
                    assert_eq!(public_report, detailed_report, "{name}");
                }
                (actual, expected_ok) => {
                    panic!("{name}: expected public ok={expected_ok}, got {actual:?}");
                }
            }
        }
    }

    #[test]
    fn resolves_bindings_and_splits_evidence() {
        let (m, report) = parse_sources(MINIMAL, Some(CLAIMS)).expect("ok");
        assert!(report.is_ok());
        assert!(report.warnings().is_empty());
        assert_eq!(m.nodes.len(), 2);
        assert_eq!(m.nodes[0].id, NodeId::new("N01")); // DFS/source order
        assert_eq!(m.nodes[1].id, NodeId::new("N02"));
        assert_eq!(m.links.len(), 1); // N01 -> N02 child
        assert_eq!(m.links[0].kind, LinkKind::Child);
        assert_eq!(m.bindings.len(), 1); // N02 -> C01
        assert_eq!(m.bindings[0].claim, ClaimId::new("C01"));
        assert_eq!(m.nodes[1].evidence_notes, vec!["Table 2"]);
    }

    #[test]
    fn missing_claims_downgrades_binding_to_warning() {
        let (m, report) = parse_sources(MINIMAL, None).expect("ok");
        assert!(report.is_ok());
        assert!(m.bindings.is_empty());
        assert_eq!(report.warnings().len(), 1);
        assert!(report.warnings()[0].message.contains("unresolved"));
    }

    #[test]
    fn broken_claim_ref_is_error() {
        let err = parse_sources(MINIMAL, Some("## C99: other\n")).unwrap_err();
        assert!(!err.is_ok());
        assert!(err.errors()[0].message.contains("unknown claim"));
    }

    #[test]
    fn malformed_yaml_is_error_not_panic() {
        let err = parse_sources("tree: not-a-list\n", None).unwrap_err();
        assert_eq!(err.errors()[0].path, "document");
    }

    #[test]
    fn both_roots_is_error() {
        let err = parse_sources("tree: []\nroot:\n  id: N01\n", None).unwrap_err();
        assert!(err.errors()[0].message.contains("both"));
    }

    #[test]
    fn neither_root_is_error() {
        let err = parse_sources("meta: hi\n", None).unwrap_err();
        assert!(err.errors()[0].message.contains("neither"));
    }

    #[test]
    fn empty_tree_warns_and_is_ok() {
        let (m, report) = parse_sources("tree: []\n", None).expect("ok");
        assert!(m.nodes.is_empty());
        assert_eq!(report.warnings().len(), 1);
    }

    #[test]
    fn cycle_is_detected() {
        // A genuine cross-cycle across two branches: N02 -> N04 -> N02, where
        // neither is an ancestor of the other (so it is not the tolerated
        // redundant-back-edge case). detect_cycles must flag it.
        let yaml = "\
tree:
  - id: N01
    type: question
    children:
      - id: N02
        type: experiment
        also_depends_on: [N04]
      - id: N03
        type: decision
        children:
          - id: N04
            type: insight
            also_depends_on: [N02]
";
        let err = parse_sources(yaml, None).unwrap_err();
        assert!(err.errors().iter().any(|d| d.message.contains("cycle")));
    }

    #[test]
    fn duplicate_node_id_is_error() {
        let yaml = "\
tree:
  - id: N01
    type: question
  - id: N01
    type: insight
";
        let err = parse_sources(yaml, None).unwrap_err();
        assert!(
            err.errors()
                .iter()
                .any(|d| d.message.contains("duplicate node id"))
        );
    }

    #[test]
    fn unknown_type_becomes_other_and_warns() {
        let yaml = "tree:\n  - id: N01\n    type: hypothesis\n    title: h\n";
        let (m, _r) = parse_sources(yaml, None).expect("ok");
        assert_eq!(m.nodes[0].kind, NodeKind::Other("hypothesis".into()));
    }

    #[test]
    fn root_single_matches_tree_shape() {
        let tree = "tree:\n  - id: N01\n    type: question\n    title: q\n";
        let root = "root:\n  id: N01\n  type: question\n  title: q\n";
        let (mt, _) = parse_sources(tree, None).expect("ok");
        let (mr, _) = parse_sources(root, None).expect("ok");
        assert_eq!(mt.nodes, mr.nodes);
    }

    #[test]
    fn determinism_parse_twice_identical() {
        let (a, _) = parse_sources(MINIMAL, Some(CLAIMS)).expect("ok");
        let (b, _) = parse_sources(MINIMAL, Some(CLAIMS)).expect("ok");
        assert_eq!(a, b);
    }

    #[test]
    fn broken_node_to_node_ref_is_error() {
        let yaml = "\
tree:
  - id: N01
    type: question
    children:
      - id: N02
        type: experiment
        also_depends_on: [N99]
";
        let err = parse_sources(yaml, None).unwrap_err();
        assert!(
            err.errors()
                .iter()
                .any(|d| d.message.contains("unknown node") && d.path.contains("also_depends_on")),
            "expected broken node->node error, got: {err}"
        );
    }

    #[test]
    fn broken_claim_to_claim_dep_is_error() {
        // C01 depends on C99, which does not exist.
        let claims = "## C01: A\n- **Dependencies**: [C99]\n";
        let err = parse_sources(MINIMAL, Some(claims)).unwrap_err();
        assert!(
            err.errors()
                .iter()
                .any(|d| d.message.contains("unknown claim") && d.path.contains("dependencies")),
            "expected broken claim->claim error, got: {err}"
        );
    }

    #[test]
    fn proof_evidence_refs_emit_no_error() {
        // `E##` proof refs are stored raw and must never produce a diagnostic.
        let claims = "## C01: A\n- **Statement**: s\n- **Proof**: [E01, E02]\n";
        let (m, report) = parse_sources(MINIMAL, Some(claims)).expect("ok");
        assert_eq!(m.claims[0].proof, vec!["E01", "E02"]);
        // Success with no errors at all: E## refs are opaque, never validated.
        assert!(report.is_ok());
        assert!(report.errors().is_empty());
    }

    #[test]
    fn sibling_only_depends_on_cycle_is_detected() {
        // Cycle formed purely by DependsOn edges between siblings (no Child edge).
        let yaml = "\
tree:
  - id: N01
    type: question
    children:
      - id: N02
        type: experiment
        also_depends_on: [N03]
      - id: N03
        type: insight
        also_depends_on: [N02]
";
        let err = parse_sources(yaml, None).unwrap_err();
        assert!(err.errors().iter().any(|d| d.message.contains("cycle")));
    }

    #[test]
    fn redundant_ancestor_depends_on_is_dropped_with_warning() {
        // A child that re-declares `also_depends_on` on its own parent restates
        // the nesting. The edge is dropped with a WARNING (not a fatal cycle),
        // so the artifact still parses.
        let yaml = "\
tree:
  - id: N01
    type: question
    children:
      - id: N02
        type: experiment
        also_depends_on: [N01]
";
        let (m, report) = parse_sources(yaml, None).expect("parses ok despite ancestor dep");
        assert!(report.is_ok(), "must not error: {report}");
        // No DependsOn link survives; only the N01->N02 child link remains.
        assert_eq!(m.links.len(), 1);
        assert_eq!(m.links[0].kind, LinkKind::Child);
        assert!(
            report
                .warnings()
                .iter()
                .any(|d| d.message.contains("redundant") && d.message.contains("ancestor")),
            "expected redundant-ancestor warning, got: {report}"
        );
    }

    #[test]
    fn redundant_grandparent_depends_on_is_dropped() {
        // Ancestry is transitive: a dependency on a grandparent is redundant too.
        let yaml = "\
tree:
  - id: N01
    type: question
    children:
      - id: N02
        type: experiment
        children:
          - id: N03
            type: insight
            also_depends_on: [N01]
";
        let (m, report) = parse_sources(yaml, None).expect("ok");
        assert!(report.is_ok());
        // Two child links, zero DependsOn links.
        assert!(m.links.iter().all(|l| l.kind == LinkKind::Child));
        assert_eq!(m.links.len(), 2);
    }

    #[test]
    fn sibling_depends_on_is_kept_not_dropped() {
        // A dependency on a sibling is a genuine DAG cross-edge (the sibling is
        // not an ancestor) and must survive.
        let yaml = "\
tree:
  - id: N01
    type: question
    children:
      - id: N02
        type: experiment
      - id: N03
        type: insight
        also_depends_on: [N02]
";
        let (m, report) = parse_sources(yaml, None).expect("ok");
        assert!(report.is_ok());
        assert!(
            m.links.iter().any(|l| l.kind == LinkKind::DependsOn
                && l.from == NodeId::new("N03")
                && l.to == NodeId::new("N02")),
            "sibling cross-edge must be kept"
        );
    }

    #[test]
    fn missing_node_id_is_error() {
        // A node with no `id` is dropped with an ERROR (data-dropping path).
        let err = parse_sources("tree:\n  - type: question\n    title: q\n", None).unwrap_err();
        assert!(
            err.errors()
                .iter()
                .any(|d| d.message.contains("missing an `id`")),
            "expected missing-id error, got: {err}"
        );
    }

    #[test]
    fn duplicate_claim_id_is_error() {
        // `claims.rs` surfaces the dup as data; `parse_sources` turns it into the
        // `claims[{id}]` ERROR diagnostic.
        let err = parse_sources(MINIMAL, Some("## C01: A\n## C01: B\n")).unwrap_err();
        assert!(
            err.errors()
                .iter()
                .any(|d| d.path.contains("claims[C01]") && d.message.contains("duplicate claim id")),
            "expected duplicate-claim-id error, got: {err}"
        );
    }

    #[test]
    fn isolated_field_defaults_false_and_sources_from_raw() {
        // Absent `isolated:` → false; an explicit `isolated: true` on a node is
        // carried through to the normalized node.
        let yaml = "\
tree:
  - id: N01
    type: question
    children:
      - id: N02
        type: experiment
        isolated: true
";
        let (m, _r) = parse_sources(yaml, None).expect("ok");
        assert!(!m.nodes[0].isolated, "N01 has no isolated key → false");
        assert!(m.nodes[1].isolated, "N02 carries isolated: true");
    }

    #[test]
    fn missing_type_warns() {
        // Distinct from the unknown-type branch: an absent `type:` warns (WARNING),
        // and the node still parses as `Other`.
        let (m, report) = parse_sources("tree:\n  - id: N01\n    title: q\n", None).expect("ok");
        assert_eq!(m.nodes[0].kind, NodeKind::Other(String::new()));
        assert!(
            report
                .warnings()
                .iter()
                .any(|d| d.message.contains("missing a `type`")),
            "expected missing-type warning, got: {report}"
        );
    }

    #[test]
    fn missing_type_dropped_body_fields_warn() {
        // A missing-`type:` node carrying canonical body fields must warn per
        // field in addition to the missing-type warning, matching the
        // unknown-type arm — nothing is lost silently.
        let yaml = "tree:\n  - id: N01\n    title: q\n    result: observed result\n";
        let (m, report) = parse_sources(yaml, None).expect("ok");
        assert_eq!(m.nodes[0].kind, NodeKind::Other(String::new()));
        assert!(
            report
                .warnings()
                .iter()
                .any(|d| d.message.contains("missing a `type`")),
            "expected missing-type warning, got: {report}"
        );
        assert!(
            report
                .warnings()
                .iter()
                .any(|d| d.message.contains("`result` dropped for missing type")),
            "expected dropped-field warning for `result`, got: {report}"
        );
    }

    #[test]
    fn unknown_type_dropped_body_field_warns() {
        // An unknown-typed node carrying a canonical body field warns that the
        // field is dropped, so nothing is lost silently.
        let yaml = "tree:\n  - id: N01\n    type: hypothesis\n    result: 28.4 BLEU\n";
        let (m, report) = parse_sources(yaml, None).expect("ok");
        assert_eq!(m.nodes[0].kind, NodeKind::Other("hypothesis".into()));
        assert!(
            report.warnings().iter().any(|d| d
                .message
                .contains("`result` dropped for unknown type `hypothesis`")),
            "expected dropped-field warning, got: {report}"
        );
    }

    #[test]
    fn unknown_type_dropped_new_body_fields_warn() {
        // The published pivot/experiment body fields are canonical too: an
        // unknown-typed node carrying them must warn per field, so nothing is
        // lost silently.
        let yaml = "\
tree:
  - id: N01
    type: hypothesis
    prior_direction: dense
    new_direction: sparse
    reason: latency
    status: running
    exploration: grid
    outcome: wins
";
        let (m, report) = parse_sources(yaml, None).expect("ok");
        assert_eq!(m.nodes[0].kind, NodeKind::Other("hypothesis".into()));
        for field in [
            "prior_direction",
            "new_direction",
            "reason",
            "exploration",
            "outcome",
        ] {
            assert!(
                report.warnings().iter().any(|d| d
                    .message
                    .contains(&format!("`{field}` dropped for unknown type `hypothesis`"))),
                "expected dropped-field warning for `{field}`, got: {report}"
            );
        }
    }

    #[test]
    fn pivot_projects_kind_and_fields_no_warning() {
        // A `pivot` node projects to NodeKind::Pivot + NodeFields::Pivot with
        // prior_direction/new_direction/reason/lesson populated, and carries no
        // unknown-field warning. `lesson` is shared with `dead_end` at the raw
        // layer and must project for pivot too (regression pin).
        let yaml = "\
tree:
  - id: N01
    type: pivot
    prior_direction: manual
    new_direction: automated
    reason: infeasible at scale
    lesson: profile before committing
";
        let (m, report) = parse_sources(yaml, None).expect("ok");
        assert_eq!(m.nodes[0].kind, NodeKind::Pivot);
        assert_eq!(
            m.nodes[0].fields,
            NodeFields::Pivot {
                prior_direction: Some("manual".to_string()),
                new_direction: Some("automated".to_string()),
                reason: Some("infeasible at scale".to_string()),
                lesson: Some("profile before committing".to_string()),
            }
        );
        assert!(
            report.warnings().is_empty(),
            "pivot fields must not warn, got: {report}"
        );
    }

    #[test]
    fn dead_end_widened_fields_no_warning() {
        // A `dead_end` node carrying hypothesis/failure_mode/lesson populates all
        // fields (plus why_failed) and carries no unknown-field warning.
        let yaml = "\
tree:
  - id: N01
    type: dead_end
    hypothesis: h
    failure_mode: fm
    lesson: l
    why_failed: wf
";
        let (m, report) = parse_sources(yaml, None).expect("ok");
        assert_eq!(m.nodes[0].kind, NodeKind::DeadEnd);
        assert_eq!(
            m.nodes[0].fields,
            NodeFields::DeadEnd {
                hypothesis: Some("h".to_string()),
                failure_mode: Some("fm".to_string()),
                lesson: Some("l".to_string()),
                why_failed: Some("wf".to_string()),
            }
        );
        assert!(
            report.warnings().is_empty(),
            "dead_end fields must not warn, got: {report}"
        );
    }

    #[test]
    fn wrong_kind_body_field_warns_per_field() {
        // Every modeled body field present on a known kind that does not
        // project it is dropped with exactly one warning naming the field and
        // the kind, so nothing is lost silently. (The unknown-type path is
        // pinned separately by unknown_type_dropped_*.)
        let cases: &[(&str, &str, &str)] = &[
            // (field, yaml entry, a kind that does not project it)
            ("result", "result: r", "question"),
            ("exploration", "exploration: grid", "decision"),
            ("outcome", "outcome: wins", "pivot"),
            ("why_failed", "why_failed: wf", "experiment"),
            ("hypothesis", "hypothesis: h", "experiment"),
            ("failure_mode", "failure_mode: fm", "question"),
            ("lesson", "lesson: l", "question"),
            ("lesson", "lesson: l", "insight"),
            ("prior_direction", "prior_direction: dense", "dead_end"),
            ("new_direction", "new_direction: sparse", "experiment"),
            ("reason", "reason: latency", "decision"),
            ("choice", "choice: c", "experiment"),
            ("alternatives", "alternatives: [a, b]", "pivot"),
            ("rationale", "rationale: rat", "dead_end"),
        ];
        for (field, entry, kind) in cases {
            let yaml = format!("tree:\n  - id: N01\n    type: {kind}\n    {entry}\n");
            let (_m, report) = parse_sources(&yaml, None).expect("ok");
            let drops: Vec<_> = report
                .warnings()
                .iter()
                .filter(|d| {
                    d.message
                        .contains(&format!("`{field}` dropped for type `{kind}`"))
                })
                .collect();
            assert_eq!(
                drops.len(),
                1,
                "expected exactly one drop warning for `{field}` on `{kind}`, got: {report}"
            );
            assert_eq!(
                report.warnings().len(),
                1,
                "no other warnings expected for `{field}` on `{kind}`, got: {report}"
            );
        }
    }

    #[test]
    fn right_kind_body_fields_no_drop_warnings() {
        // Each scoped body field on a kind that projects it is kept silently:
        // experiment keeps result/exploration/outcome/status, decision keeps
        // choice/alternatives/rationale. (dead_end/pivot — including `lesson`,
        // shared by both — are pinned by dead_end_widened_fields_no_warning
        // and pivot_projects_kind_and_fields_no_warning.)
        let yaml = "\
tree:
  - id: N01
    type: experiment
    result: r
    exploration: e
    outcome: o
    status: s
  - id: N02
    type: decision
    choice: c
    alternatives: [a, b]
    rationale: rat
";
        let (_m, report) = parse_sources(yaml, None).expect("ok");
        assert!(
            report.warnings().is_empty(),
            "right-kind fields must not warn, got: {report}"
        );
    }

    #[test]
    fn quoted_scalars_remain_text_and_explicit_tags_keep_their_type() {
        let yaml = r#"tree:
  - id: 'N01'
    type: "question"
    title: 'null'
    description: "true\nnext"
    artifacts:
      - name: "null"
        pointer: 'false'
        what: "42"
        quoted_bool: "true"
        quoted_null: 'null'
        quoted_integer: "42"
        tagged_integer: !!int "42"
        tagged_string: !!str true
"#;
        let (manifest, _) = parse_sources(yaml, None).unwrap();
        let node = &manifest.nodes[0];
        assert_eq!(node.kind, NodeKind::Question);
        assert_eq!(node.label.as_deref(), Some("null"));
        assert_eq!(node.description.as_deref(), Some("true\nnext"));
        let artifact = &node.artifacts[0];
        assert_eq!(artifact.name, "null");
        assert_eq!(artifact.pointer, "false");
        assert_eq!(artifact.what, "42");
        for (field, value) in [
            ("quoted_bool", "true"),
            ("quoted_null", "null"),
            ("quoted_integer", "42"),
            ("tagged_string", "true"),
        ] {
            assert_eq!(
                artifact.extra[field],
                crate::manifest::SourceValue::String(value.into())
            );
        }
        assert_eq!(
            artifact.extra["tagged_integer"],
            crate::manifest::SourceValue::Integer(42)
        );
    }

    #[test]
    fn duplicate_link_warns() {
        // A repeated `also_depends_on` target yields two identical DependsOn links;
        // `dedupe_links` keeps the first and warns on the duplicate. Two siblings
        // keep the graph acyclic.
        let yaml = "\
tree:
  - id: N01
    type: question
    children:
      - id: N02
        type: experiment
        also_depends_on: [N03, N03]
      - id: N03
        type: insight
";
        let (_m, report) = parse_sources(yaml, None).expect("ok");
        assert!(
            report
                .warnings()
                .iter()
                .any(|d| d.message.contains("duplicate") && d.message.contains("link to `N03`")),
            "expected duplicate-link warning, got: {report}"
        );
    }
}

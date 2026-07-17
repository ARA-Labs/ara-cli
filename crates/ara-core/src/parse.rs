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
    Binding, BindingRole, ClaimId, Link, LinkKind, Manifest, Node, NodeFields, NodeId, NodeKind,
    is_canonical_id,
};
use crate::report::ParseReport;
use crate::schema::{RawNode, parse_doc};

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
    let mut report = ParseReport::default();

    let doc = match parse_doc(tree_yaml) {
        Ok(doc) => doc,
        Err(msg) => {
            report.error("document", msg);
            return Err(report);
        }
    };

    for key in doc.extra.keys() {
        report.warn("document", format!("unknown field `{key}`"));
    }

    let roots: Vec<RawNode> = match (doc.tree, doc.root) {
        (Some(_), Some(_)) => {
            report.error(
                "document",
                "both `tree:` and `root:` are present; exactly one is allowed",
            );
            return Err(report);
        }
        (None, None) => {
            report.error("document", "neither `tree:` nor `root:` is present");
            return Err(report);
        }
        (Some(tree), None) => {
            if tree.is_empty() {
                report.warn("document", "empty manifest (`tree: []`)");
            }
            tree
        }
        (None, Some(root)) => vec![*root],
    };

    // Claims resolve node→claim and claim→claim references.
    let claims_present = claims_md.is_some();
    let (claims, duplicate_claim_ids) = match claims_md {
        Some(md) => {
            let parsed = parse_claims(md);
            (parsed.claims, parsed.duplicate_ids)
        }
        None => (Vec::new(), Vec::new()),
    };
    let claim_ids: BTreeSet<ClaimId> = claims.iter().map(|c| c.id.clone()).collect();
    for id in duplicate_claim_ids {
        report.error(format!("claims[{id}]"), "duplicate claim id");
    }

    let mut norm = Normalizer {
        report,
        claims_present,
        claim_ids,
        nodes: Vec::new(),
        node_ids: BTreeSet::new(),
        bindings: Vec::new(),
        child_links: Vec::new(),
        also: Vec::new(),
    };
    for raw in &roots {
        norm.dfs(raw, None);
    }

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
                    format!("nodes[{from}].also_depends_on[{i}]"),
                    format!("`also_depends_on` references unknown node `{t}`"),
                );
                continue;
            }
            if is_ancestor(&to, from, &parent_of) {
                norm.report.warn(
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
    };

    if norm.report.is_ok() {
        Ok((manifest, norm.report))
    } else {
        Err(norm.report)
    }
}

/// Reads `trace/exploration_tree.yaml` (required) and `logic/claims.md`
/// (optional) from `dir` and normalizes them, then augments the manifest with
/// the optional logic-section files (`PAPER.md`, `logic/problem.md`,
/// `logic/concepts.md`, `logic/related_work.md`, `logic/solution/*.md`). An
/// absent section file is silently skipped; a present-but-malformed one adds a
/// warning without failing the parse. Native only.
#[cfg(feature = "native")]
pub fn parse_dir(dir: &std::path::Path) -> Result<(Manifest, ParseReport), ParseReport> {
    let tree_path = dir.join("trace/exploration_tree.yaml");
    let tree_yaml = match std::fs::read_to_string(&tree_path) {
        Ok(s) => s,
        Err(e) => {
            let mut report = ParseReport::default();
            report.error(
                "document",
                format!("cannot read {}: {e}", tree_path.display()),
            );
            return Err(report);
        }
    };
    // A missing claims file is not an error — it downgrades bindings to warnings.
    let claims_path = dir.join("logic/claims.md");
    let claims_md = std::fs::read_to_string(&claims_path).ok();

    // The base parse owns the Ok/Err contract; an error short-circuits here.
    let (mut manifest, mut report) = parse_sources(&tree_yaml, claims_md.as_deref())?;
    read_logic_layer(dir, &mut manifest, &mut report);
    Ok((manifest, report))
}

/// Reads the optional logic-section files into `manifest`, appending reader
/// warnings to `report`. Absent files are skipped without a warning; a present
/// file that parses degenerately warns but never errors.
#[cfg(feature = "native")]
fn read_logic_layer(dir: &std::path::Path, manifest: &mut Manifest, report: &mut ParseReport) {
    use crate::paper::parse_paper;
    use crate::sections::{parse_concepts, parse_problem, parse_related_work};

    if let Some(md) = read_opt(&dir.join("PAPER.md")) {
        let (paper, warnings) = parse_paper(&md);
        manifest.paper = paper;
        for w in warnings {
            report.warn("PAPER.md", w);
        }
    }

    if let Some(md) = read_opt(&dir.join("logic/problem.md")) {
        manifest.problem = Some(parse_problem(&md));
    }

    if let Some(md) = read_opt(&dir.join("logic/concepts.md")) {
        let concepts = parse_concepts(&md);
        for c in &concepts {
            if c.definition.is_none() {
                report.warn(
                    format!("concepts[{}]", c.term),
                    "concept has no definition",
                );
            }
        }
        manifest.concepts = concepts;
    }

    if let Some(md) = read_opt(&dir.join("logic/related_work.md")) {
        let related_work = parse_related_work(&md);
        for r in &related_work {
            if r.doi.is_none() {
                report.warn(
                    format!("related_work[{}]", r.id),
                    "related work has no DOI",
                );
            }
        }
        manifest.related_work = related_work;
    }

    manifest.recipes = read_recipes(&dir.join("logic/solution"));
}

/// Reads a file to a string, mapping any I/O error (including absence) to
/// `None`. Absent section files are not an error.
#[cfg(feature = "native")]
fn read_opt(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Enumerates `logic/solution/*.md` sorted by path (determinism) and builds one
/// [`crate::manifest::Recipe`] per file: filename stem, first `# Title`, and the
/// verbatim body. A missing directory yields no recipes.
#[cfg(feature = "native")]
fn read_recipes(solution_dir: &std::path::Path) -> Vec<crate::manifest::Recipe> {
    let Ok(entries) = std::fs::read_dir(solution_dir) else {
        return Vec::new();
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();

    let mut recipes = Vec::new();
    for path in paths {
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let title = crate::paper::first_h1(&body);
        recipes.push(crate::manifest::Recipe { name, title, body });
    }
    recipes
}

/// Mutable accumulators for the normalization DFS.
struct Normalizer {
    report: ParseReport,
    claims_present: bool,
    claim_ids: BTreeSet<ClaimId>,
    nodes: Vec<Node>,
    node_ids: BTreeSet<NodeId>,
    bindings: Vec<Binding>,
    child_links: Vec<Link>,
    /// Per emitted node, its raw `also_depends_on` targets (resolved later).
    also: Vec<(NodeId, Vec<String>)>,
}

impl Normalizer {
    /// Pre-order visit of `raw`, emitting one [`Node`] plus its child link,
    /// bindings, and evidence notes. A missing or duplicate id drops the node
    /// (and its subtree) with an error, rather than corrupting the graph.
    fn dfs(&mut self, raw: &RawNode, parent: Option<&NodeId>) {
        let id_str = raw.id.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let Some(id_str) = id_str else {
            let label = raw.title.as_deref().unwrap_or("<no id>");
            self.report
                .error(format!("nodes[{label}]"), "node is missing an `id`");
            return;
        };
        let id = NodeId::new(id_str);
        if self.node_ids.contains(&id) {
            self.report
                .error(format!("nodes[{id}]"), "duplicate node id");
            return;
        }
        self.node_ids.insert(id.clone());

        if let Some(parent) = parent {
            self.child_links.push(Link {
                from: parent.clone(),
                to: id.clone(),
                kind: LinkKind::Child,
            });
        }

        let (kind, fields) = self.project_kind(raw, &id);
        let evidence_notes = self.split_evidence(raw, &id);

        for key in raw.extra.keys() {
            self.report
                .warn(format!("nodes[{id}]"), format!("unknown field `{key}`"));
        }

        self.nodes.push(Node {
            id: id.clone(),
            kind,
            label: raw.title.clone(),
            support_level: raw.support_level.clone(),
            source_refs: raw.source_refs.clone(),
            description: raw.description.clone(),
            fields,
            evidence_notes,
            isolated: raw.isolated,
            pos: None,
        });
        self.also.push((id.clone(), raw.also_depends_on.clone()));

        for child in &raw.children {
            self.dfs(child, Some(&id));
        }
    }

    /// Projects `type:` + body fields into a typed [`NodeKind`]/[`NodeFields`].
    /// Unknown/missing types become [`NodeKind::Other`]; any canonical body
    /// fields carried by an unknown type are warned so nothing is lost silently.
    fn project_kind(&mut self, raw: &RawNode, id: &NodeId) -> (NodeKind, NodeFields) {
        match raw.ty.as_deref().map(str::trim) {
            Some("question") => (NodeKind::Question, NodeFields::Question),
            Some("experiment") => (
                NodeKind::Experiment,
                NodeFields::Experiment {
                    result: raw.result.clone(),
                },
            ),
            Some("decision") => (
                NodeKind::Decision,
                NodeFields::Decision {
                    choice: raw.choice.clone(),
                    alternatives: raw.alternatives.clone(),
                    rationale: raw.rationale.clone(),
                },
            ),
            Some("dead_end") => (
                NodeKind::DeadEnd,
                NodeFields::DeadEnd {
                    hypothesis: raw.hypothesis.clone(),
                    failure_mode: raw.failure_mode.clone(),
                    lesson: raw.lesson.clone(),
                    why_failed: raw.why_failed.clone(),
                },
            ),
            Some("insight") => (NodeKind::Insight, NodeFields::Insight),
            Some("pivot") => (
                NodeKind::Pivot,
                NodeFields::Pivot {
                    from: raw.from.clone(),
                    to: raw.to.clone(),
                    trigger: raw.trigger.clone(),
                },
            ),
            Some("") | None => {
                self.report
                    .warn(format!("nodes[{id}]"), "node is missing a `type`");
                (NodeKind::Other(String::new()), NodeFields::Other)
            }
            Some(other) => {
                for field in body_field_names(raw) {
                    self.report.warn(
                        format!("nodes[{id}]"),
                        format!("field `{field}` dropped for unknown type `{other}`"),
                    );
                }
                (NodeKind::Other(other.to_string()), NodeFields::Other)
            }
        }
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
                let claim = ClaimId::new(trimmed);
                let path = format!("nodes[{id}].evidence[{i}]");
                if !self.claims_present {
                    self.report.warn(
                        path,
                        format!("claim reference `{trimmed}` unresolved (no claims.md provided)"),
                    );
                } else if self.claim_ids.contains(&claim) {
                    self.bindings.push(Binding {
                        node: id.clone(),
                        claim,
                        role: BindingRole::Evidence,
                    });
                } else {
                    self.report.error(
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

/// Names of canonical body fields present on `raw` (used only to warn when an
/// unknown-typed node carries them).
fn body_field_names(raw: &RawNode) -> Vec<&'static str> {
    let mut names = Vec::new();
    if raw.result.is_some() {
        names.push("result");
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
    // `parent_of` is built from `children:` nesting, which is a tree (each node
    // has at most one parent and no parent cycles), so walking up terminates.
    let mut cur = node;
    while let Some(parent) = parent_of.get(cur) {
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
    for node in nodes {
        if color.get(&node.id).copied().unwrap_or(0) == 0 {
            visit(&node.id, &adj, &mut color, report);
        }
    }
}

fn visit<'a>(
    u: &'a NodeId,
    adj: &BTreeMap<&'a NodeId, Vec<&'a NodeId>>,
    color: &mut BTreeMap<&'a NodeId, u8>,
    report: &mut ParseReport,
) {
    color.insert(u, 1);
    if let Some(neighbors) = adj.get(u) {
        for &v in neighbors {
            match color.get(v).copied().unwrap_or(0) {
                0 => visit(v, adj, color, report),
                1 => report.error(
                    format!("nodes[{u}]"),
                    format!("cycle detected: edge to `{v}` closes a cycle"),
                ),
                _ => {}
            }
        }
    }
    color.insert(u, 2);
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
            m.links
                .iter()
                .any(|l| l.kind == LinkKind::DependsOn
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
    fn pivot_projects_kind_and_fields_no_warning() {
        // A `pivot` node projects to NodeKind::Pivot + NodeFields::Pivot with
        // from/to/trigger populated, and carries no unknown-field warning.
        let yaml = "\
tree:
  - id: N01
    type: pivot
    from: manual
    to: automated
    trigger: infeasible at scale
";
        let (m, report) = parse_sources(yaml, None).expect("ok");
        assert_eq!(m.nodes[0].kind, NodeKind::Pivot);
        assert_eq!(
            m.nodes[0].fields,
            NodeFields::Pivot {
                from: Some("manual".to_string()),
                to: Some("automated".to_string()),
                trigger: Some("infeasible at scale".to_string()),
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

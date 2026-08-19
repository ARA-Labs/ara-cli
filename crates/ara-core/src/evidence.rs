//! Lenient reader for the `evidence/` layer plus node→exhibit / node→RW
//! resolution.
//!
//! The evidence layer is an index (`evidence/README.md`) plus body files under
//! `evidence/figures/*.md` and `evidence/tables/*.md`. The corpus index tables
//! drift heavily — eight distinct header shapes across the 34 real artifacts,
//! some with no claims column, one reordering columns, others using `Key refs`
//! or `Used by` in place of `Claims`. So [`parse_index`] is **column-name
//! tolerant**: it identifies columns by header substring, never by position.
//!
//! Everything is warn-never-fatal and source-order preserving. Bodies are stored
//! verbatim (`body`); rendering (tables, images) is a client concern. The two
//! resolution passes ([`resolve_node_exhibits`], [`resolve_built_on`]) are pure
//! and deterministic, iterating nodes in manifest order and exhibits / related
//! work in source order.

use std::collections::BTreeSet;

use crate::manifest::{
    Binding, BindingRole, BuiltOn, ClaimId, Exhibit, ExhibitKind, Node, NodeExhibit, NodeId,
    RelatedWork, is_canonical_id,
};

// ── index (`evidence/README.md`) ─────────────────────────────────────────────

/// One parsed index row, keyed by a normalized basename `id`. Fields are
/// whatever the row's columns carried; any may be absent.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IndexRow {
    /// Normalized basename id (file stem), used to match a body file.
    pub id: String,
    /// The raw file-cell path, when the row carried a resolvable file cell.
    pub file: Option<String>,
    /// Origin string from a `source`-like column.
    pub source: Option<String>,
    /// Description from a `description`/`what`-like column.
    pub description: Option<String>,
    /// `C##` tokens from a `claims`/`key ref`/`used by` column.
    pub claims: Vec<ClaimId>,
}

/// Parses an `evidence/README.md` into index rows, in source order.
///
/// A README may hold several GFM tables (commonly `## Tables` and `## Figures`);
/// each is parsed independently and its rows appended. Columns are identified by
/// header name, so reordered or renamed columns still resolve. Non-table prose
/// is ignored.
pub(crate) fn parse_index(md: &str) -> Vec<IndexRow> {
    let lines: Vec<&str> = md.lines().collect();
    let mut rows = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        // A table starts where a `|`-row is followed by a delimiter row.
        if is_table_row(lines[i]) && i + 1 < lines.len() && is_delimiter_row(lines[i + 1]) {
            let headers = split_row(lines[i]);
            let cols = ColumnMap::from_headers(&headers);
            let mut j = i + 2;
            while j < lines.len() && is_table_row(lines[j]) {
                let cells = split_row(lines[j]);
                if let Some(row) = cols.build_row(&cells) {
                    rows.push(row);
                }
                j += 1;
            }
            i = j;
        } else {
            i += 1;
        }
    }

    rows
}

/// Which column carries which field, resolved by header name.
struct ColumnMap {
    file: Option<usize>,
    claims: Option<usize>,
    source: Option<usize>,
    description: Option<usize>,
}

impl ColumnMap {
    /// Resolves columns from a header row by case-insensitive substring match.
    fn from_headers(headers: &[String]) -> Self {
        let lower: Vec<String> = headers.iter().map(|h| h.to_ascii_lowercase()).collect();
        let find = |pred: &dyn Fn(&str) -> bool| lower.iter().position(|h| pred(h));

        // File: first header containing `file`; fall back to column 0.
        let file = find(&|h| h.contains("file")).or(if lower.is_empty() { None } else { Some(0) });
        // Claims: `claim` | `key ref` | `used by`.
        let claims =
            find(&|h| h.contains("claim") || h.contains("key ref") || h.contains("used by"));
        let source = find(&|h| h.contains("source"));
        let description = find(&|h| h.contains("desc") || h.contains("what"));

        ColumnMap {
            file,
            claims,
            source,
            description,
        }
    }

    /// Builds an [`IndexRow`] from a data row's cells. Returns `None` when the
    /// row has no usable id (empty first/file cell).
    fn build_row(&self, cells: &[String]) -> Option<IndexRow> {
        let cell = |idx: Option<usize>| idx.and_then(|k| cells.get(k)).map(|s| s.trim());

        let file_cell = cell(self.file)?;
        if file_cell.is_empty() {
            return None;
        }
        let (id, file) = normalize_file_cell(file_cell);
        if id.is_empty() {
            return None;
        }

        let claims = self
            .claims
            .and_then(|k| cells.get(k))
            .map(|c| extract_claim_ids(c))
            .unwrap_or_default();

        Some(IndexRow {
            id,
            file,
            source: cell(self.source).and_then(non_empty),
            description: cell(self.description).and_then(non_empty),
            claims,
        })
    }
}

/// True for a line that looks like a GFM table row (`| ... |`).
fn is_table_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// True for a GFM delimiter row: only `|`, `-`, `:`, and spaces, with at least
/// one `-`.
fn is_delimiter_row(line: &str) -> bool {
    let t = line.trim();
    if !t.starts_with('|') {
        return false;
    }
    let mut saw_dash = false;
    for c in t.chars() {
        match c {
            '-' => saw_dash = true,
            '|' | ':' | ' ' | '\t' => {}
            _ => return false,
        }
    }
    saw_dash
}

/// Splits a `| a | b |` row into trimmed cell strings, dropping the empty edges
/// created by the leading/trailing pipes.
fn split_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

/// Normalizes a file cell to `(id, file)`. Handles a markdown link
/// `[x](path/foo.md)`, a backtick `` `table1.md` ``, a bare path `figures/foo.md`,
/// and a dual-ext `foo (png/md)` / `foo.(png/md)`. `id` is the basename stem;
/// `file` is the raw relative path when one is recoverable (a prose "fact" cell
/// yields `id` = slug, `file` = `None`).
fn normalize_file_cell(cell: &str) -> (String, Option<String>) {
    let cell = cell.trim();

    // Markdown link: take the link target.
    if let Some(path) = markdown_link_target(cell) {
        return (file_stem_id(&path), Some(path));
    }
    // Backtick-fenced path.
    let unticked = cell.trim_matches('`').trim();
    // A path-like cell contains a `/` or ends in a recognizable extension.
    if looks_like_path(unticked) {
        return (file_stem_id(unticked), Some(unticked.to_string()));
    }
    // Prose cell (e.g. a "Fact" column): no file, id is a slug for warnings.
    (slug(cell), None)
}

/// The `(target)` of a `[text](target)` markdown link, if the cell is one.
fn markdown_link_target(cell: &str) -> Option<String> {
    let open = cell.find("](")?;
    let rest = &cell[open + 2..];
    let close = rest.find(')')?;
    let target = rest[..close].trim();
    if target.is_empty() {
        None
    } else {
        Some(target.to_string())
    }
}

/// True when a cell looks like a file path rather than free prose: it has a `/`
/// separator or a short alphanumeric-ish extension, and no interior spaces
/// beyond an optional dual-ext marker.
fn looks_like_path(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.contains('/') {
        return true;
    }
    // A dual-ext marker like `foo (png/md)` or a trailing `.md`.
    if s.contains("(png") || s.contains("(md") || s.contains("/md)") || s.contains("/png)") {
        return true;
    }
    // A single trailing extension with no spaces in the name.
    match s.rsplit_once('.') {
        Some((stem, ext)) => {
            !stem.is_empty()
                && !stem.contains(' ')
                && (1..=5).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
        }
        None => false,
    }
}

/// The basename stem of a path, stripping directories, a dual-ext `(png/md)`
/// marker, and a single trailing extension.
fn file_stem_id(path: &str) -> String {
    // Trim once up front so every byte offset below indexes the same string
    // (a leading-whitespace mismatch could otherwise slice mid-char and panic).
    let path = path.trim();
    // Strip a trailing dual-ext group like `.(png/md)` or ` (png/md)` FIRST — it
    // may itself contain a `/` that would otherwise corrupt the basename split.
    let path = match path.rfind('(') {
        Some(open) => {
            let tail = &path[open..];
            if tail.ends_with(')')
                && (tail.contains("md") || tail.contains("png") || tail.contains('/'))
            {
                path[..open].trim_end_matches([' ', '.'])
            } else {
                path
            }
        }
        None => path,
    };

    let base = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .trim()
        .to_string();

    // Strip a single trailing extension (`.md`, `.png`, …).
    match base.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && !ext.is_empty()
                && ext.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            stem.to_string()
        }
        _ => base,
    }
}

/// A conservative slug of a prose cell, used only as a warning id when a row
/// carries no file. Lowercased, non-alphanumerics collapsed to `-`.
fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.chars().take(48) {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Extracts every `C\d+` token from a cell, ignoring `H##`/`N##`/prose tokens.
fn extract_claim_ids(value: &str) -> Vec<ClaimId> {
    let mut seen = BTreeSet::new();
    value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|tok| is_canonical_id(tok, 'C'))
        .filter(|tok| seen.insert(tok.to_string()))
        .map(ClaimId::new)
        .collect()
}

/// Trims and returns `None` for empty values.
fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

// ── body files + assembly (native) ───────────────────────────────────────────

/// Reads the `evidence/` layer of `dir` into exhibits, appending warnings.
///
/// Enumerates `evidence/figures/*.md` then `evidence/tables/*.md` (each sorted),
/// building one [`Exhibit`] per body file. Index rows enrich the matching body
/// by basename id (index wins over body for source/description). An index row
/// with no matching body, or a body with no index row, warns but never errors.
/// An absent `evidence/` dir or absent `README.md` is a silent skip.
#[cfg(feature = "native")]
pub(crate) fn read_evidence(
    dir: &std::path::Path,
    report: &mut crate::report::ParseReport,
) -> Vec<Exhibit> {
    let evidence_dir = dir.join("evidence");
    if !evidence_dir.is_dir() {
        return Vec::new();
    }

    // Index is optional: bodies still yield exhibits, just without index enrichment.
    let index: Vec<IndexRow> = std::fs::read_to_string(evidence_dir.join("README.md"))
        .ok()
        .map(|md| parse_index(&md))
        .unwrap_or_default();
    // First index row wins per id (deterministic).
    let mut index_by_id: std::collections::BTreeMap<String, IndexRow> =
        std::collections::BTreeMap::new();
    for row in &index {
        index_by_id
            .entry(row.id.clone())
            .or_insert_with(|| row.clone());
    }

    let mut exhibits = Vec::new();
    let mut consumed: BTreeSet<String> = BTreeSet::new();

    for (subdir, kind) in [
        ("figures", ExhibitKind::Figure),
        ("tables", ExhibitKind::Table),
    ] {
        for path in sorted_md_files(&evidence_dir.join(subdir)) {
            let Ok(body) = std::fs::read_to_string(&path) else {
                continue;
            };
            let id = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let file = format!(
                "evidence/{subdir}/{}",
                path.file_name()
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default()
            );

            let row = index_by_id.get(&id);
            if let Some(row) = row {
                consumed.insert(row.id.clone());
            } else {
                report.warn(
                    format!("evidence/{subdir}/{id}"),
                    "body file has no index row in evidence/README.md",
                );
            }

            let exhibit = assemble_exhibit(id, file, kind.clone(), row, &body);
            exhibits.push(exhibit);
        }
    }

    // Any index row that never matched a body file (and pointed at one) warns.
    for row in &index {
        if row.file.is_some() && !consumed.contains(&row.id) {
            report.warn(
                format!("evidence[{}]", row.id),
                "index row references a file with no body under evidence/",
            );
        }
    }

    exhibits
}

/// Merges an index row (if any) and a body into one exhibit. `source`/
/// `description` prefer the index, falling back to the body's `- **Source**:` /
/// `- **Caption**:` bullets. `claims` are the index row's `C##`, or — when the
/// index has none — the `Supports: C##` refs scanned from the body.
fn assemble_exhibit(
    id: String,
    file: String,
    kind: ExhibitKind,
    row: Option<&IndexRow>,
    body: &str,
) -> Exhibit {
    let source = row
        .and_then(|r| r.source.clone())
        .or_else(|| body_bullet(body, "source"));
    let description = row
        .and_then(|r| r.description.clone())
        .or_else(|| body_bullet(body, "caption"));

    let claims = match row.map(|r| r.claims.clone()).unwrap_or_default() {
        c if !c.is_empty() => c,
        _ => body_supports(body),
    };

    Exhibit {
        id,
        file,
        kind,
        source,
        description,
        claims,
        body: body.to_string(),
    }
}

/// The value of a `- **Label**: value` bullet in a body, matched by
/// case-insensitive label. Returns the first such bullet's value.
fn body_bullet(body: &str, label: &str) -> Option<String> {
    for line in body.lines() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) else {
            continue;
        };
        let Some(after) = rest.trim_start().strip_prefix("**") else {
            continue;
        };
        let Some((key, tail)) = after.split_once("**") else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case(label) {
            let tail = tail.trim_start();
            let value = tail.strip_prefix(':').unwrap_or(tail).trim();
            if let Some(v) = non_empty(value) {
                return Some(v);
            }
        }
    }
    None
}

/// `C##` refs from any body line mentioning `Supports` (the nanogpt_ara
/// convention, where claim linkage lives in the body, not the index).
fn body_supports(body: &str) -> Vec<ClaimId> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for line in body.lines() {
        if !line.contains("Supports") {
            continue;
        }
        for id in extract_claim_ids(line) {
            if seen.insert(id.as_str().to_string()) {
                out.push(id);
            }
        }
    }
    out
}

/// Enumerates `*.md` files in `dir`, sorted by path. Missing dir → empty.
#[cfg(feature = "native")]
fn sorted_md_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();
    paths
}

// ── resolution passes (pure) ─────────────────────────────────────────────────

/// Node → exhibit edges by shared claim. For each node (in `nodes` order) whose
/// evidence claim set intersects an exhibit's `claims` (exhibits in source
/// order), emits one [`NodeExhibit`]. Deterministic; each (node, exhibit) once.
pub(crate) fn resolve_node_exhibits(
    nodes: &[Node],
    bindings: &[Binding],
    exhibits: &[Exhibit],
) -> Vec<NodeExhibit> {
    let mut out = Vec::new();
    let mut seen: BTreeSet<(NodeId, String)> = BTreeSet::new();
    for node in nodes {
        let node_claims = evidence_claims(&node.id, bindings);
        if node_claims.is_empty() {
            continue;
        }
        for exhibit in exhibits {
            if exhibit.claims.iter().any(|c| node_claims.contains(c)) {
                let key = (node.id.clone(), exhibit.id.clone());
                if seen.insert(key) {
                    out.push(NodeExhibit {
                        node: node.id.clone(),
                        exhibit: exhibit.id.clone(),
                    });
                }
            }
        }
    }
    out
}

/// Node → related-work edges by shared claim. Same shape as
/// [`resolve_node_exhibits`], matching a node's evidence claims against each
/// related-work entry's `claims_affected`.
pub(crate) fn resolve_built_on(
    nodes: &[Node],
    bindings: &[Binding],
    related_work: &[RelatedWork],
) -> Vec<BuiltOn> {
    let mut out = Vec::new();
    let mut seen: BTreeSet<(NodeId, String)> = BTreeSet::new();
    for node in nodes {
        let node_claims = evidence_claims(&node.id, bindings);
        if node_claims.is_empty() {
            continue;
        }
        for rw in related_work {
            if rw.claims_affected.iter().any(|c| node_claims.contains(c)) {
                let key = (node.id.clone(), rw.id.clone());
                if seen.insert(key) {
                    out.push(BuiltOn {
                        node: node.id.clone(),
                        related_work: rw.id.clone(),
                    });
                }
            }
        }
    }
    out
}

/// The set of claims a node references via `Evidence`-role bindings.
fn evidence_claims(node: &NodeId, bindings: &[Binding]) -> BTreeSet<ClaimId> {
    bindings
        .iter()
        .filter(|b| &b.node == node && b.role == BindingRole::Evidence)
        .map(|b| b.claim.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(ids: &[&str]) -> Vec<ClaimId> {
        ids.iter().map(|s| ClaimId::new(*s)).collect()
    }

    #[test]
    fn index_canonical_file_source_claims_description() {
        let md = "\
## Figures
| File | Source | Claims | Description |
|------|--------|--------|-------------|
| [figures/fig3_scalability.md](figures/fig3_scalability.md) | Figure 3, §4.3 | C01, C04 | Growth demo. |
";
        let rows = parse_index(md);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "fig3_scalability");
        assert_eq!(rows[0].claims, claims(&["C01", "C04"]));
        assert_eq!(rows[0].source.as_deref(), Some("Figure 3, §4.3"));
        assert_eq!(rows[0].description.as_deref(), Some("Growth demo."));
    }

    #[test]
    fn index_reordered_columns_claims_in_col4() {
        // `File | Description | Source | Claims` — Claims moved to the last column.
        let md = "\
| File | Description | Source | Claims |
|------|-------------|--------|--------|
| [tables/reference_scores.md](tables/reference_scores.md) | ref scores | README | C01, C05 |
| [tables/human_baselines.md](tables/human_baselines.md) | humans | README:42-52 | — |
";
        let rows = parse_index(md);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "reference_scores");
        assert_eq!(rows[0].claims, claims(&["C01", "C05"]));
        assert_eq!(rows[0].description.as_deref(), Some("ref scores"));
        assert_eq!(rows[0].source.as_deref(), Some("README"));
        // Em-dash claims cell → no claims.
        assert_eq!(rows[1].id, "human_baselines");
        assert!(rows[1].claims.is_empty());
    }

    #[test]
    fn index_key_refs_header_carries_claims() {
        let md = "\
| File | Description | Key refs |
|------|-------------|---------|
| [tables/reference_scores.md](tables/reference_scores.md) | ref | C01, C05, C12 |
";
        let rows = parse_index(md);
        assert_eq!(rows[0].claims, claims(&["C01", "C05", "C12"]));
    }

    #[test]
    fn index_used_by_no_file_no_claims_column() {
        // `Fact | Source turns | Used by` — no File column (id falls back to col 0
        // slug, file None) and `Used by` carries the C## refs.
        let md = "\
| Fact | Source turns | Used by |
|------|--------------|---------|
| Move model A1 up, 5-cell steps | 0->4 | C01, C02, H01-H05 |
";
        let rows = parse_index(md);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].file.is_none());
        assert!(!rows[0].id.is_empty());
        assert_eq!(rows[0].claims, claims(&["C01", "C02"])); // H## ignored
    }

    #[test]
    fn index_backtick_and_dual_ext_file_cells() {
        let md = "\
| File | Type | Source object | What it shows |
|---|---|---|---|
| `tables/trajectory_summary.md` | table | run index | the arc |
| figures/v1_loss_curves.(png/md) | quantitative_plot | v1 png | loss curve |
";
        let rows = parse_index(md);
        assert_eq!(rows[0].id, "trajectory_summary");
        assert_eq!(rows[0].description.as_deref(), Some("the arc")); // `what` column
        assert_eq!(rows[1].id, "v1_loss_curves"); // dual-ext stripped
        assert_eq!(rows[1].source.as_deref(), Some("v1 png")); // `source object`
    }

    #[test]
    fn index_description_only_no_claims() {
        let md = "\
| File | Description |
|---|---|
| [tables/reference_scores.md](tables/reference_scores.md) | Starting score 1.81 |
";
        let rows = parse_index(md);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "reference_scores");
        assert!(rows[0].claims.is_empty());
        assert_eq!(rows[0].description.as_deref(), Some("Starting score 1.81"));
    }

    #[test]
    fn index_multiple_tables_parsed_independently() {
        let md = "\
## Tables
| File | Source | Claims | Description |
|------|--------|--------|-------------|
| [tables/t1.md](tables/t1.md) | T1 | C02 | table one |

## Figures
| File | Source | Claims | Description |
|------|--------|--------|-------------|
| [figures/f1.md](figures/f1.md) | F1 | C03 | fig one |
";
        let rows = parse_index(md);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "t1");
        assert_eq!(rows[1].id, "f1");
    }

    #[test]
    fn empty_index_is_empty() {
        assert!(parse_index("").is_empty());
        assert!(parse_index("# Just prose\n\nNo tables here.\n").is_empty());
    }

    #[test]
    fn body_bullet_source_and_caption() {
        let body = "# Fig\n- **Source**: Figure 3, Section 4.3\n- **Caption**: \"a caption\"\n";
        assert_eq!(
            body_bullet(body, "source").as_deref(),
            Some("Figure 3, Section 4.3")
        );
        assert_eq!(
            body_bullet(body, "caption").as_deref(),
            Some("\"a caption\"")
        );
        assert!(body_bullet(body, "missing").is_none());
    }

    #[test]
    fn body_supports_scans_claim_refs() {
        let body = "**Source:** `x`. Supports C06, C11; figure\n";
        assert_eq!(body_supports(body), claims(&["C06", "C11"]));
    }

    #[test]
    fn resolve_node_exhibits_matches_on_shared_claim() {
        let nodes = vec![node("N01"), node("N02")];
        let bindings = vec![binding("N01", "C01")];
        let exhibits = vec![
            exhibit("figA", &["C01", "C04"]),
            exhibit("figB", &["C02"]),
            exhibit("figC", &["C04", "C01"]),
        ];
        let out = resolve_node_exhibits(&nodes, &bindings, &exhibits);
        // N01 (claims {C01}) matches figA and figC in source order; N02 has none.
        assert_eq!(
            out.iter().map(|n| n.exhibit.as_str()).collect::<Vec<_>>(),
            vec!["figA", "figC"]
        );
        assert!(out.iter().all(|n| n.node == NodeId::new("N01")));
    }

    #[test]
    fn resolve_built_on_matches_on_shared_claim() {
        let nodes = vec![node("N07")];
        let bindings = vec![binding("N07", "C01")];
        let rw = vec![
            related_work("RW01", &["C01", "C04"]),
            related_work("RW02", &["C02", "C03"]),
            related_work("RW09", &["C01", "C02"]),
        ];
        let out = resolve_built_on(&nodes, &bindings, &rw);
        assert_eq!(
            out.iter()
                .map(|b| b.related_work.as_str())
                .collect::<Vec<_>>(),
            vec!["RW01", "RW09"]
        );
    }

    #[test]
    fn resolve_empty_when_node_has_no_matching_exhibit() {
        let nodes = vec![node("N01")];
        let bindings = vec![binding("N01", "C99")];
        let exhibits = vec![exhibit("figA", &["C01"])];
        assert!(resolve_node_exhibits(&nodes, &bindings, &exhibits).is_empty());
    }

    // ── test helpers ─────────────────────────────────────────────────────────

    fn node(id: &str) -> Node {
        Node {
            id: NodeId::new(id),
            kind: crate::manifest::NodeKind::Experiment,
            label: None,
            support_level: None,
            source_refs: Vec::new(),
            description: None,
            provenance: None,
            timestamp: None,
            fields: crate::manifest::NodeFields::Experiment {
                result: None,
                exploration: None,
                outcome: None,
                status: None,
            },
            evidence_notes: Vec::new(),
            isolated: false,
            pos: None,
        }
    }

    fn binding(node: &str, claim: &str) -> Binding {
        Binding {
            node: NodeId::new(node),
            claim: ClaimId::new(claim),
            role: BindingRole::Evidence,
        }
    }

    fn exhibit(id: &str, claim_ids: &[&str]) -> Exhibit {
        Exhibit {
            id: id.to_string(),
            file: format!("evidence/figures/{id}.md"),
            kind: ExhibitKind::Figure,
            source: None,
            description: None,
            claims: claims(claim_ids),
            body: String::new(),
        }
    }

    fn related_work(id: &str, claim_ids: &[&str]) -> RelatedWork {
        RelatedWork {
            id: id.to_string(),
            cite: String::new(),
            doi: None,
            kind: None,
            what_changed: None,
            why: None,
            adopted: None,
            claims_affected: claims(claim_ids),
        }
    }
}

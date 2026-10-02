//! Lenient reader for the `evidence/` layer plus node→exhibit / node→RW
//! resolution.
//!
//! The evidence layer is an index (`evidence/README.md`) plus body files under
//! `evidence/figures/*.md`, `evidence/proofs/*.md`, `evidence/results/*.md`,
//! and `evidence/tables/*.md` (enumerated in that fixed order, each sorted; a
//! basename duplicated across categories keeps both bodies and warns once).
//! Figures also discover PNG/JPEG companions and raster-only exhibits.
//! The corpus index tables
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
use crate::rules::RuleCode;

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
/// Enumerates direct children of figures, proofs, results, then tables (fixed
/// category order, sorted within each). Figures combine Markdown and local PNG/
/// JPEG assets; the other categories remain Markdown-only. Index rows enrich
/// each basename (index wins over body for source/description). Unmatched index
/// rows and unindexed exhibits warn but never error.
/// The same basename appearing in two categories also warns —
/// exhibit identity is basename-based, so both exhibits keep their bodies but
/// share index enrichment. An absent `evidence/` directory is a silent skip;
/// `README.md` metadata is optional.
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
    // First category each basename was read from, for the duplicate-id warning.
    let mut first_seen: std::collections::BTreeMap<String, &'static str> =
        std::collections::BTreeMap::new();

    for (subdir, kind) in [
        ("figures", ExhibitKind::Figure),
        ("proofs", ExhibitKind::Proof),
        ("results", ExhibitKind::Result),
        ("tables", ExhibitKind::Table),
    ] {
        let category_start = exhibits.len();
        let rasters = if kind == ExhibitKind::Figure {
            sorted_raster_files(&evidence_dir.join(subdir))
        } else {
            Vec::new()
        };
        let mut body_ids = BTreeSet::new();
        let mut referenced_images = BTreeSet::new();
        for path in sorted_md_files(&evidence_dir.join(subdir)) {
            let Ok(body) = std::fs::read_to_string(&path) else {
                continue;
            };
            let id = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            body_ids.insert(id.clone());
            let file = format!(
                "evidence/{subdir}/{}",
                path.file_name()
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default()
            );

            // Duplicate basename across categories: identity stays
            // basename-based and both bodies are kept, but the collision must
            // not be silent.
            match first_seen.entry(id.clone()) {
                std::collections::btree_map::Entry::Vacant(v) => {
                    v.insert(subdir);
                }
                std::collections::btree_map::Entry::Occupied(o) => {
                    if *o.get() != subdir {
                        report.warn(
                            RuleCode::DuplicateExhibitBasename,
                            format!("evidence/{subdir}/{id}"),
                            format!(
                                "duplicate exhibit basename: already read from evidence/{}/{id}; \
                                 both bodies kept, index enrichment is shared",
                                o.get()
                            ),
                        );
                    }
                }
            }

            let row = index_by_id.get(&id);
            if let Some(row) = row {
                consumed.insert(row.id.clone());
            } else {
                report.warn(
                    RuleCode::ExhibitMissingIndexRow,
                    format!("evidence/{subdir}/{id}"),
                    "body file has no index row in evidence/README.md",
                );
            }

            let mut exhibit = assemble_exhibit(id, file, kind.clone(), row, &body);
            if kind == ExhibitKind::Figure {
                let declared = body_bullet(&body, "image").or_else(|| {
                    row.and_then(|r| r.file.as_ref())
                        .filter(|file| crate::figure::image_mime(file).is_some())
                        .cloned()
                });
                let selected =
                    if declared.is_some() {
                        declared
                    } else {
                        let stem = path.file_stem();
                        let mut candidates = rasters.iter().filter(|p| p.file_stem() == stem);
                        match (candidates.next(), candidates.next()) {
                            (Some(candidate), None) => Some(format!(
                                "{subdir}/{}",
                                candidate.file_name().unwrap().to_string_lossy()
                            )),
                            (Some(_), Some(_)) => {
                                report.warn(RuleCode::InvalidFigureImage, &exhibit.file,
                                "ambiguous same-stem raster siblings; declare an Image explicitly");
                                None
                            }
                            _ => None,
                        }
                    };
                if let Some(reference) = selected {
                    match crate::figure::resolve_image(&evidence_dir, &reference) {
                        Ok(_) => exhibit.image = Some(format!("evidence/{reference}")),
                        Err(reason) => report.warn(
                            RuleCode::InvalidFigureImage,
                            &exhibit.file,
                            format!("invalid figure image {reference:?}: {reason}"),
                        ),
                    }
                    referenced_images.insert(reference);
                }
            }
            exhibits.push(exhibit);
        }
        // Raster companions and explicitly selected differently named assets are
        // not standalone exhibits. Remaining rasters retain their own identities.
        let mut raster_groups: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for path in rasters {
            let id = path.file_stem().unwrap().to_string_lossy().into_owned();
            raster_groups.entry(id).or_default().push(format!(
                "{subdir}/{}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
        for (id, candidates) in raster_groups {
            let row = index_by_id.get(&id);
            if let Some(row) = row {
                consumed.insert(row.id.clone());
            }
            if body_ids.contains(&id) {
                continue;
            }
            let declaration = row
                .and_then(|r| r.file.as_ref())
                .filter(|file| crate::figure::image_mime(file).is_some());
            let relative = if let Some(declared) = declaration {
                declared.as_str()
            } else {
                let mut remaining = candidates
                    .iter()
                    .filter(|p| !referenced_images.contains(*p));
                match (remaining.next(), remaining.next()) {
                    (Some(candidate), None) => candidate.as_str(),
                    (None, _) => continue,
                    _ => {
                        report.warn(RuleCode::InvalidFigureImage, format!("evidence/{subdir}/{id}"),
                            "ambiguous raster files; select a File explicitly in evidence/README.md");
                        continue;
                    }
                }
            };
            if referenced_images.contains(relative) {
                continue;
            }
            let file = format!("evidence/{relative}");
            match crate::figure::resolve_image(&evidence_dir, relative) {
                Ok(_) => {
                    if row.is_none() {
                        report.warn(
                            RuleCode::ExhibitMissingIndexRow,
                            &file,
                            "image file has no index row in evidence/README.md",
                        );
                    }
                    first_seen.entry(id.clone()).or_insert(subdir);
                    let mut exhibit = assemble_exhibit(id, file.clone(), kind.clone(), row, "");
                    exhibit.image = Some(file);
                    exhibits.push(exhibit);
                }
                Err(reason) => report.warn(
                    RuleCode::InvalidFigureImage,
                    &file,
                    format!("invalid figure image: {reason}"),
                ),
            }
        }
        exhibits[category_start..].sort_by(|a, b| a.file.cmp(&b.file));
    }

    // Index rows without discovered exhibits still diagnose missing declarations.
    for row in &index {
        if row.file.is_some() && !consumed.contains(&row.id) {
            if row
                .file
                .as_deref()
                .is_some_and(|p| crate::figure::image_mime(p).is_some())
            {
                report.warn(RuleCode::InvalidFigureImage, format!("evidence[{}]", row.id),
                    "invalid figure image: indexed raster has no discovered figure under evidence/figures");
            } else {
                report.warn(
                    RuleCode::IndexRowMissingExhibit,
                    format!("evidence[{}]", row.id),
                    "index row references a file with no body under evidence/",
                );
            }
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
        image: None,
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
            if label == "image" {
                return Some(value.to_string());
            }
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

/// Direct-child raster discovery, sorted just like Markdown discovery.
#[cfg(feature = "native")]
fn sorted_raster_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.to_str()
                    .is_some_and(|s| crate::figure::image_mime(s).is_some())
        })
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

    // ── read_evidence (native, filesystem) ──────────────────────────────────

    /// Builds a temp artifact with an `evidence/` layer. `files` are
    /// `(subdir, name, body)`; `readme` is the optional index.
    #[cfg(feature = "native")]
    fn evidence_artifact(files: &[(&str, &str, &str)], readme: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        let evidence = dir.path().join("evidence");
        for (subdir, name, body) in files {
            let d = evidence.join(subdir);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(name), body).unwrap();
        }
        if let Some(md) = readme {
            std::fs::create_dir_all(&evidence).unwrap();
            std::fs::write(evidence.join("README.md"), md).unwrap();
        }
        dir
    }

    #[cfg(feature = "native")]
    #[test]
    fn read_evidence_figure_companion_preserves_body_and_image() {
        let body =
            "# Loss\n\nSupporting measurements.\n\n| Step | Loss |\n|---|---|\n| 1 | 0.5 |\n";
        let dir = evidence_artifact(
            &[("figures", "loss.md", body)],
            Some(
                "| File | Claims | Description |\n|---|---|---|\n| figures/loss.md | C01 | Loss curve |\n",
            ),
        );
        // A complete, decodable one-pixel PNG, not merely an extension fixture.
        let png: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 4, 0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 100, 248, 15,
            0, 1, 5, 1, 1, 39, 24, 227, 102, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ];
        std::fs::write(dir.path().join("evidence/figures/loss.png"), png).unwrap();

        let mut report = crate::report::ParseReport::default();
        let exhibits = read_evidence(dir.path(), &mut report);
        assert_eq!(exhibits.len(), 1, "companions form one exhibit");
        assert_eq!(exhibits[0].body, body);
        assert_eq!(exhibits[0].file, "evidence/figures/loss.md");
        assert_eq!(exhibits[0].claims, claims(&["C01"]));
        assert_eq!(
            serde_json::to_value(&exhibits[0]).unwrap().get("image"),
            Some(&serde_json::json!("evidence/figures/loss.png")),
        );
        assert!(report.warnings().is_empty());
    }

    #[cfg(feature = "native")]
    #[test]
    fn read_evidence_enumerates_four_categories_in_fixed_order() {
        let dir = evidence_artifact(
            &[
                ("tables", "t2.md", "table two"),
                ("results", "r2.md", "result two"),
                ("figures", "f2.md", "fig two"),
                ("proofs", "p2.md", "proof two"),
                ("tables", "t1.md", "table one"),
                ("figures", "f1.md", "fig one"),
                ("proofs", "p1.md", "proof one"),
                ("results", "r1.md", "result one"),
            ],
            None,
        );
        let mut report = crate::report::ParseReport::default();
        let exhibits = read_evidence(dir.path(), &mut report);
        let order: Vec<(ExhibitKind, &str)> = exhibits
            .iter()
            .map(|e| (e.kind.clone(), e.id.as_str()))
            .collect();
        assert_eq!(
            order,
            vec![
                (ExhibitKind::Figure, "f1"),
                (ExhibitKind::Figure, "f2"),
                (ExhibitKind::Proof, "p1"),
                (ExhibitKind::Proof, "p2"),
                (ExhibitKind::Result, "r1"),
                (ExhibitKind::Result, "r2"),
                (ExhibitKind::Table, "t1"),
                (ExhibitKind::Table, "t2"),
            ]
        );
        // Bodies are verbatim.
        assert_eq!(exhibits[0].body, "fig one");
        assert_eq!(exhibits[7].body, "table two");
        // No index README → one "no index row" warning per body, no duplicates.
        assert!(
            report
                .warnings()
                .iter()
                .all(|w| !w.message.contains("duplicate"))
        );
    }

    #[cfg(feature = "native")]
    #[test]
    fn read_evidence_results_and_proofs_enriched_from_index() {
        // Reordered-columns header — the tolerant parse resolves `Claims` by
        // name, not position, for results/proofs rows the same as figures.
        let readme = "\
| File | Description | Source | Claims |
|------|-------------|--------|--------|
| [results/main_result.md](results/main_result.md) | the number | run 42 | C01, C02 |
| [proofs/lemma1.md](proofs/lemma1.md) | lemma one | appendix A | C03 |
";
        let dir = evidence_artifact(
            &[
                ("results", "main_result.md", "result body"),
                ("proofs", "lemma1.md", "proof body"),
            ],
            Some(readme),
        );
        let mut report = crate::report::ParseReport::default();
        let exhibits = read_evidence(dir.path(), &mut report);
        assert_eq!(exhibits.len(), 2);
        // Fixed order: proofs before results.
        let proof = &exhibits[0];
        assert_eq!(proof.kind, ExhibitKind::Proof);
        assert_eq!(proof.id, "lemma1");
        assert_eq!(proof.file, "evidence/proofs/lemma1.md");
        assert_eq!(proof.source.as_deref(), Some("appendix A"));
        assert_eq!(proof.description.as_deref(), Some("lemma one"));
        assert_eq!(proof.claims, claims(&["C03"]));
        assert_eq!(proof.body, "proof body");
        let result = &exhibits[1];
        assert_eq!(result.kind, ExhibitKind::Result);
        assert_eq!(result.id, "main_result");
        assert_eq!(result.source.as_deref(), Some("run 42"));
        assert_eq!(result.description.as_deref(), Some("the number"));
        assert_eq!(result.claims, claims(&["C01", "C02"]));
        // Both bodies matched index rows → no warnings at all.
        assert!(report.warnings().is_empty());
    }

    #[cfg(feature = "native")]
    #[test]
    fn read_evidence_duplicate_basename_across_categories_warns_once() {
        let dir = evidence_artifact(
            &[
                ("figures", "shared.md", "figure body"),
                ("results", "shared.md", "result body"),
                ("tables", "solo.md", "table body"),
            ],
            None,
        );
        let mut report = crate::report::ParseReport::default();
        let exhibits = read_evidence(dir.path(), &mut report);

        // Both bodies survive, in fixed category order.
        let shared: Vec<&Exhibit> = exhibits.iter().filter(|e| e.id == "shared").collect();
        assert_eq!(shared.len(), 2);
        assert_eq!(shared[0].kind, ExhibitKind::Figure);
        assert_eq!(shared[0].body, "figure body");
        assert_eq!(shared[0].file, "evidence/figures/shared.md");
        assert_eq!(shared[1].kind, ExhibitKind::Result);
        assert_eq!(shared[1].body, "result body");
        assert_eq!(shared[1].file, "evidence/results/shared.md");

        // Exactly one duplicate warning, on the later file.
        let dup: Vec<_> = report
            .warnings()
            .iter()
            .filter(|w| w.message.contains("duplicate"))
            .collect();
        assert_eq!(dup.len(), 1);
        assert_eq!(dup[0].path, "evidence/results/shared");
    }

    #[cfg(feature = "native")]
    #[test]
    fn read_evidence_single_category_files_no_duplicate_warning() {
        let dir = evidence_artifact(
            &[("figures", "a.md", "alpha"), ("figures", "b.md", "beta")],
            None,
        );
        let mut report = crate::report::ParseReport::default();
        let exhibits = read_evidence(dir.path(), &mut report);
        assert_eq!(exhibits.len(), 2);
        assert!(
            report
                .warnings()
                .iter()
                .all(|w| !w.message.contains("duplicate"))
        );
    }

    #[cfg(feature = "native")]
    fn add_png(dir: &std::path::Path, name: &str) {
        let path = dir.join("evidence").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, include_bytes!("../tests/fixtures/images/pixel.png")).unwrap();
    }

    #[cfg(feature = "native")]
    #[test]
    fn raster_only_indexed_figure_links_to_claim() {
        let dir = evidence_artifact(
            &[],
            Some(
                "| File | Claims | Description |\n|---|---|---|\n| figures/plot.PNG | C01 | The plot |\n",
            ),
        );
        add_png(dir.path(), "figures/plot.PNG");
        let mut report = crate::report::ParseReport::default();
        let exhibits = read_evidence(dir.path(), &mut report);
        assert_eq!(exhibits.len(), 1);
        let ex = &exhibits[0];
        assert_eq!(ex.id, "plot");
        assert_eq!(ex.file, "evidence/figures/plot.PNG");
        assert_eq!(ex.image.as_deref(), Some(ex.file.as_str()));
        assert_eq!(ex.body, "");
        assert_eq!(ex.description.as_deref(), Some("The plot"));
        assert_eq!(
            resolve_node_exhibits(&[node("N01")], &[binding("N01", "C01")], &exhibits),
            vec![NodeExhibit {
                node: NodeId::new("N01"),
                exhibit: "plot".into()
            }]
        );
        assert!(report.warnings().is_empty());
    }

    #[cfg(feature = "native")]
    #[test]
    fn raster_only_ambiguous_formats_require_an_index_selection() {
        let dir = evidence_artifact(&[], None);
        add_png(dir.path(), "figures/loss.png");
        std::fs::write(
            dir.path().join("evidence/figures/loss.jpg"),
            include_bytes!("../tests/fixtures/images/pixel.jpg"),
        )
        .unwrap();
        let mut report = crate::report::ParseReport::default();
        assert!(read_evidence(dir.path(), &mut report).is_empty());
        assert!(
            report
                .warnings()
                .iter()
                .any(|w| w.code == RuleCode::InvalidFigureImage)
        );
        std::fs::write(
            dir.path().join("evidence/README.md"),
            "| File | Claims |\n|---|---|\n| figures/loss.jpg | C01 |\n",
        )
        .unwrap();
        let mut report = crate::report::ParseReport::default();
        let exhibits = read_evidence(dir.path(), &mut report);
        assert_eq!(exhibits.len(), 1);
        assert_eq!(
            exhibits[0].image.as_deref(),
            Some("evidence/figures/loss.jpg")
        );
        assert_eq!(exhibits[0].claims, claims(&["C01"]));
        assert!(report.warnings().is_empty());
    }

    #[cfg(feature = "native")]
    #[test]
    fn invalid_index_raster_does_not_replace_declaration_with_valid_sibling() {
        for file in ["../loss.png", "figures/loss.jpg"] {
            let readme = format!("| File | Claims |\n|---|---|\n| {file} | C01 |\n");
            let dir = evidence_artifact(&[("figures", "loss.md", "Keep body.")], Some(&readme));
            add_png(dir.path(), "figures/loss.png");
            let mut report = crate::report::ParseReport::default();
            let exhibits = read_evidence(dir.path(), &mut report);
            assert_eq!(exhibits.len(), 1);
            assert_eq!(exhibits[0].body, "Keep body.");
            assert!(exhibits[0].image.is_none());
            assert!(
                report
                    .warnings()
                    .iter()
                    .any(|w| w.code == RuleCode::InvalidFigureImage)
            );
        }
    }

    #[cfg(feature = "native")]
    #[test]
    fn raster_index_missing_or_unsafe_reference_uses_image_warning() {
        for reference in [
            "figures/missing.PNG",
            "../missing.png",
            "https://example.test/missing.png",
        ] {
            let index = format!("| File | Claims |\n|---|---|\n| {reference} | C01 |\n");
            let dir = evidence_artifact(&[], Some(&index));
            let mut report = crate::report::ParseReport::default();
            assert!(read_evidence(dir.path(), &mut report).is_empty());
            assert_eq!(report.warnings().len(), 1);
            assert_eq!(report.warnings()[0].code, RuleCode::InvalidFigureImage);
        }
    }

    #[cfg(feature = "native")]
    #[test]
    fn explicit_image_wins_and_asset_is_not_an_exhibit() {
        let body = "- **Image**: figures/other % # ?.png\n\nSupporting data.";
        let dir = evidence_artifact(
            &[("figures", "loss.md", body)],
            Some("| File | Claims |\n|---|---|\n| figures/loss.jpg | C01 |\n"),
        );
        add_png(dir.path(), "figures/loss.png");
        add_png(dir.path(), "figures/other % # ?.png");
        let mut report = crate::report::ParseReport::default();
        let exhibits = read_evidence(dir.path(), &mut report);
        assert_eq!(exhibits.len(), 1);
        assert_eq!(
            exhibits[0].image.as_deref(),
            Some("evidence/figures/other % # ?.png")
        );
        assert_eq!(exhibits[0].body, body);
        assert!(report.warnings().is_empty());
    }

    #[cfg(feature = "native")]
    #[test]
    fn rejected_explicit_images_do_not_fall_back_to_sibling() {
        for reference in [
            "figures/missing.png",
            "../outside.png",
            "/tmp/outside.png",
            "https://example.test/x.png",
            "//example.test/x.png",
            "C:/x.png",
            "figures\\loss.png",
            "figures/x.svg",
            "",
        ] {
            let body = format!("- **Image**: {reference}\n\nKeep this body.");
            let dir = evidence_artifact(&[("figures", "loss.md", &body)], None);
            add_png(dir.path(), "figures/loss.png");
            let mut report = crate::report::ParseReport::default();
            let exhibits = read_evidence(dir.path(), &mut report);
            assert_eq!(exhibits.len(), 1, "{reference}");
            assert_eq!(exhibits[0].body, body);
            assert!(exhibits[0].image.is_none(), "{reference}");
            assert!(
                report
                    .warnings()
                    .iter()
                    .any(|w| w.code == RuleCode::InvalidFigureImage),
                "{reference}"
            );
        }
    }

    #[cfg(feature = "native")]
    #[test]
    fn index_raster_beats_sibling_and_dual_extension_uses_real_pair() {
        for (file, expected) in [
            ("figures/loss.jpg", "evidence/figures/loss.jpg"),
            ("figures/loss.(png/md)", "evidence/figures/loss.png"),
        ] {
            let readme = format!("| File | Claims |\n|---|---|\n| {file} | C01 |\n");
            let dir = evidence_artifact(&[("figures", "loss.md", "body")], Some(&readme));
            add_png(dir.path(), "figures/loss.png");
            if file.ends_with(".jpg") {
                std::fs::write(
                    dir.path().join("evidence/figures/loss.jpg"),
                    include_bytes!("../tests/fixtures/images/pixel.jpg"),
                )
                .unwrap();
            }
            let mut report = crate::report::ParseReport::default();
            let exhibits = read_evidence(dir.path(), &mut report);
            assert_eq!(exhibits.len(), 1);
            assert_eq!(exhibits[0].image.as_deref(), Some(expected));
            assert!(report.warnings().is_empty());
        }
    }

    #[cfg(feature = "native")]
    #[test]
    fn ambiguous_siblings_keep_markdown_without_selecting_an_image() {
        let dir = evidence_artifact(&[("figures", "loss.md", "body")], None);
        add_png(dir.path(), "figures/loss.png");
        std::fs::write(
            dir.path().join("evidence/figures/loss.jpg"),
            include_bytes!("../tests/fixtures/images/pixel.jpg"),
        )
        .unwrap();
        let mut report = crate::report::ParseReport::default();
        let exhibits = read_evidence(dir.path(), &mut report);
        assert_eq!(exhibits.len(), 1);
        assert_eq!(exhibits[0].body, "body");
        assert!(exhibits[0].image.is_none());
        assert!(
            report
                .warnings()
                .iter()
                .any(|w| w.code == RuleCode::InvalidFigureImage)
        );
    }

    #[cfg(feature = "native")]
    #[test]
    fn raster_named_directory_does_not_hide_regular_sibling() {
        let dir = evidence_artifact(
            &[("figures", "loss.md", "body")],
            Some("| File | Claims |\n|---|---|\n| figures/loss.md | C01 |\n"),
        );
        add_png(dir.path(), "figures/loss.png");
        std::fs::create_dir(dir.path().join("evidence/figures/loss.jpg")).unwrap();
        let mut report = crate::report::ParseReport::default();
        let exhibits = read_evidence(dir.path(), &mut report);
        assert_eq!(
            exhibits[0].image.as_deref(),
            Some("evidence/figures/loss.png")
        );
        assert_eq!(exhibits[0].body, "body");
        assert!(report.warnings().is_empty());
    }

    #[cfg(feature = "native")]
    #[test]
    fn referenced_raster_does_not_suppress_distinct_indexed_format() {
        for jpeg_present in [true, false] {
            let dir = evidence_artifact(
                &[("figures", "overview.md", "- **Image**: figures/shared.png")],
                Some(
                    "| File | Claims |\n|---|---|\n| figures/overview.md | C01 |\n| figures/shared.jpg | C02 |\n",
                ),
            );
            add_png(dir.path(), "figures/shared.png");
            if jpeg_present {
                std::fs::write(
                    dir.path().join("evidence/figures/shared.jpg"),
                    include_bytes!("../tests/fixtures/images/pixel.jpg"),
                )
                .unwrap();
            }
            let mut report = crate::report::ParseReport::default();
            let exhibits = read_evidence(dir.path(), &mut report);
            assert_eq!(
                exhibits
                    .iter()
                    .find(|ex| ex.id == "overview")
                    .unwrap()
                    .image
                    .as_deref(),
                Some("evidence/figures/shared.png")
            );
            if jpeg_present {
                let indexed = exhibits
                    .iter()
                    .find(|ex| ex.id == "shared")
                    .expect("distinct indexed raster");
                assert_eq!(
                    indexed.image.as_deref(),
                    Some("evidence/figures/shared.jpg")
                );
                assert_eq!(
                    resolve_node_exhibits(&[node("N01")], &[binding("N01", "C02")], &exhibits),
                    vec![NodeExhibit {
                        node: NodeId::new("N01"),
                        exhibit: "shared".into()
                    }]
                );
                assert!(report.warnings().is_empty());
            } else {
                assert!(
                    report
                        .warnings()
                        .iter()
                        .any(|w| w.code == RuleCode::InvalidFigureImage)
                );
            }
        }
    }

    #[test]
    fn legacy_exhibit_json_and_optional_image_round_trip() {
        let legacy = serde_json::json!({
            "id": "loss", "file": "evidence/figures/loss.md", "kind": "figure",
            "source": null, "description": null, "claims": [], "body": "body"
        });
        let mut ex: Exhibit = serde_json::from_value(legacy.clone()).unwrap();
        assert!(ex.image.is_none());
        assert_eq!(serde_json::to_value(&ex).unwrap(), legacy);
        ex.image = Some("evidence/figures/loss.png".into());
        assert_eq!(
            serde_json::from_str::<Exhibit>(&serde_json::to_string(&ex).unwrap()).unwrap(),
            ex
        );
    }

    #[cfg(all(feature = "native", unix))]
    #[test]
    fn image_symlinks_allow_selected_root_and_contained_assets_only() {
        use std::os::unix::fs::symlink;
        let dir = evidence_artifact(
            &[("figures", "loss.md", "- **Image**: figures/alias.png")],
            None,
        );
        add_png(dir.path(), "figures/asset.png");
        symlink("asset.png", dir.path().join("evidence/figures/alias.png")).unwrap();
        let roots = tempfile::TempDir::new().unwrap();
        let selected = roots.path().join("selected");
        symlink(dir.path(), &selected).unwrap();
        let mut report = crate::report::ParseReport::default();
        let exhibits = read_evidence(&selected, &mut report);
        assert_eq!(
            exhibits
                .iter()
                .find(|e| e.id == "loss")
                .unwrap()
                .image
                .as_deref(),
            Some("evidence/figures/alias.png")
        );
        let outside = tempfile::TempDir::new().unwrap();
        std::fs::write(
            outside.path().join("secret.png"),
            include_bytes!("../tests/fixtures/images/pixel.png"),
        )
        .unwrap();
        std::fs::remove_file(dir.path().join("evidence/figures/alias.png")).unwrap();
        symlink(
            outside.path().join("secret.png"),
            dir.path().join("evidence/figures/alias.png"),
        )
        .unwrap();
        let mut report = crate::report::ParseReport::default();
        let exhibits = read_evidence(&selected, &mut report);
        assert!(
            exhibits
                .iter()
                .find(|e| e.id == "loss")
                .unwrap()
                .image
                .is_none()
        );
        assert!(
            report
                .warnings()
                .iter()
                .any(|w| w.code == RuleCode::InvalidFigureImage)
        );
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
            artifacts: vec![],
            concepts: vec![],
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
            image: None,
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

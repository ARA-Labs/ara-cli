//! `ara find`: BM25 ranking with source-mapped hit lines.
use super::headings::Sections;
use super::hits::{self, LineIndex};
use super::spans::SpanCache;
use super::{Artifact, Entry, ReadOptions, ShowArgs, show_loaded, window};
use crate::output::AgentError;
use crate::search::{Filter, SearchHit};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, clap::Args)]
pub struct FindArgs {
    pub query: String,
    #[arg(long = "type")]
    pub kind: Option<String>,
    #[arg(long, default_value_t = 10)]
    pub limit: usize,
    /// Source lines around each matched line (long form only: `-C` selects
    /// the artifact). `0` adds none.
    #[arg(long)]
    pub context: Option<usize>,
    /// Matched lines kept per hit; `0` keeps all. Brief text defaults to 3,
    /// JSON to 20.
    #[arg(long)]
    pub lines_per_hit: Option<usize>,
    /// Response byte budget; brief text defaults to 16 KiB. Whole hits are
    /// dropped from the end until the response fits.
    #[arg(long, allow_hyphen_values = true, conflicts_with = "fields")]
    pub max_bytes: Option<String>,
    /// Keep only hits whose source is this knowledge document (repeatable).
    #[arg(long)]
    pub document: Vec<String>,
    /// Keep only hits whose indexed text contains every query term.
    #[arg(long)]
    pub all: bool,
    #[command(flatten)]
    pub output: ReadOptions,
}

/// Matched lines brief text keeps per hit without `--lines-per-hit`.
pub const BRIEF_LINES_PER_HIT: usize = 3;

impl FindArgs {
    /// Matched lines kept per hit; `None` keeps all.
    fn cap(&self) -> Option<usize> {
        match self.lines_per_hit {
            Some(0) => None,
            Some(lines) => Some(lines),
            None if self.output.brief() => Some(BRIEF_LINES_PER_HIT),
            None => Some(hits::MAX_MATCHES),
        }
    }
    /// The response budget: explicit, or the brief default.
    fn budget(&self) -> Result<Option<usize>, AgentError> {
        let explicit = self
            .max_bytes
            .as_deref()
            .map(window::parse_max_bytes)
            .transpose()?;
        Ok(explicit.or(self.output.brief().then_some(window::DEFAULT_MAX_BYTES)))
    }
}

pub fn find(root: &Path, args: &FindArgs) -> Result<Value, AgentError> {
    let budget = args.budget()?;
    let artifact = Artifact::load(root)?;
    let documents = args
        .document
        .iter()
        .map(|document| artifact.list_document(document).map(|(path, _)| path))
        .collect::<Result<BTreeSet<String>, AgentError>>()?;
    let filter = Filter {
        sources: (!documents.is_empty()).then_some(&documents),
        all_terms: args.all,
    };
    let hits = crate::search::run_search_with_documents(
        &artifact.manifest,
        artifact.searchable_documents(),
        &args.query,
        args.kind.as_deref(),
        args.limit,
        filter,
    )
    .map_err(|message| AgentError::semantic("invalid_search", message))?;
    let mut results = Results {
        artifact: &artifact,
        entries: artifact.entries(),
        addresses: artifact.entry_addresses(),
        cache: SpanCache::default(),
        lines: BTreeMap::new(),
        args,
    };
    let rows = hits
        .iter()
        .map(|hit| results.row(hit, args.cap()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut value = super::validity::with_coverage(
        json!({"format":"ara.find/v1","results":rows,"diagnostics":artifact.diagnostics()}),
        &artifact.report,
    );
    if let Some(budget) = budget {
        fit(&mut value, budget, args.output.brief(), |cap| {
            results.row(&hits[0], Some(cap))
        })?;
    }
    Ok(value)
}

/// What one result row needs beyond its search hit.
struct Results<'a> {
    artifact: &'a Artifact,
    entries: Vec<Entry<'a>>,
    addresses: super::display::EntryAddresses<'a>,
    cache: SpanCache<'a>,
    lines: BTreeMap<String, LineIndex>,
    args: &'a FindArgs,
}
impl Results<'_> {
    /// The JSON row of one hit, keeping at most `cap` matched lines.
    fn row(&mut self, hit: &SearchHit, cap: Option<usize>) -> Result<Value, AgentError> {
        let (artifact, args) = (self.artifact, self.args);
        let mut result = serde_json::to_value(hit).expect("search result serialization");
        let object = result.as_object_mut().expect("search result object");
        let entry = located(&self.entries, hit);
        if args.output.brief()
            && let Some(entry) = entry
        {
            object.insert("address".into(), json!(self.addresses.address(entry)));
        }
        if let Some(span) = entry.and_then(|entry| artifact.search_span(entry, &mut self.cache)) {
            let index = self
                .lines
                .entry(span.path.to_string())
                .or_insert_with(|| LineIndex::new(span.text));
            object.extend(hits::fields(&span, index, &hit.terms, args.context, cap));
            if let Some(Entry::Document { path, content }) = entry {
                let sections = artifact.sections(path, content);
                containing_sections(object, &sections, index);
            }
        } else {
            object.insert("match_count".into(), json!(0));
        }
        let selector = entry
            .map(|entry| self.addresses.selector(entry))
            .or_else(|| hit.id.clone().or_else(|| hit.key.clone()));
        if args.output.full
            && let Some(id) = selector
        {
            // The embedded entry is the JSON projection, never brief data.
            let mut show = show_loaded(
                artifact,
                &ShowArgs {
                    ids: vec![id],
                    output: ReadOptions {
                        full: true,
                        json: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )?;
            object.insert(
                "entry".into(),
                show["entries"].as_array_mut().unwrap().remove(0),
            );
        }
        Ok(result)
    }
}

/// Fit a `find` response to `budget` bytes as it prints. Whole hits are
/// dropped from the end, and a `truncation` object counts them. When even
/// the first hit alone does not fit, `first` rebuilds it with fewer matched
/// lines; when one matched line does not fit, the read fails.
fn fit(
    value: &mut Value,
    budget: usize,
    brief: bool,
    mut first: impl FnMut(usize) -> Result<Value, AgentError>,
) -> Result<(), AgentError> {
    if window::measure(value, brief) <= budget {
        return Ok(());
    }
    let mut rows = match value["results"].take() {
        Value::Array(rows) => rows,
        _ => Vec::new(),
    };
    let total = rows.len();
    if total == 0 {
        let required = window::measure(value, brief);
        let hint = format!(
            "The empty result needs {required} bytes; rerun with --max-bytes {required} or more"
        );
        return Err(window::too_small(None, budget, required, None, hint));
    }
    // Response size with the first `kept` hits. Each added hit prints at
    // least its header line, more than the omitted count loses in digits,
    // so the size grows with `kept` and bisection finds the largest fit.
    let size = |value: &mut Value, rows: &[Value], cut: bool| {
        value["results"] = json!(rows);
        value["truncation"] = json!({
            "omitted_hits": total - rows.len(),
            "truncated_hit": cut,
        });
        window::measure(value, brief)
    };
    if total > 1 && size(value, &rows[..1], false) <= budget {
        let (mut fitting, mut over) = (1, total);
        while over - fitting > 1 {
            let middle = fitting + (over - fitting) / 2;
            if size(value, &rows[..middle], false) <= budget {
                fitting = middle;
            } else {
                over = middle;
            }
        }
        rows.truncate(fitting);
        size(value, &rows, false);
        return Ok(());
    }
    // The first hit alone overflows: keep fewer of its matched lines.
    let shown = rows[0]["matches"].as_array().map_or(0, Vec::len);
    let mut cut = |value: &mut Value, lines: usize| -> Result<usize, AgentError> {
        let row = first(lines)?;
        Ok(size(value, std::slice::from_ref(&row), true))
    };
    let required = if shown > 1 {
        cut(value, 1)?
    } else {
        size(value, &rows[..1], false)
    };
    if shown <= 1 || required > budget {
        value["results"] = json!(rows);
        value
            .as_object_mut()
            .expect("find value")
            .remove("truncation");
        let label = rows[0]["address"]
            .as_str()
            .or_else(|| rows[0]["id"].as_str())
            .or_else(|| rows[0]["key"].as_str())
            .unwrap_or("");
        let hint = format!(
            "The first hit and its first matched line need {required} bytes; rerun with --max-bytes {required} or more"
        );
        return Err(window::too_small(Some(label), budget, required, None, hint));
    }
    let (mut fitting, mut over) = (1, shown);
    while over - fitting > 1 {
        let middle = fitting + (over - fitting) / 2;
        if cut(value, middle)? <= budget {
            fitting = middle;
        } else {
            over = middle;
        }
    }
    cut(value, fitting)?;
    Ok(())
}

/// Give each matched line of a whole-document hit the address of the
/// innermost section holding it, as `in`; a line before the first heading
/// gets none.
fn containing_sections(
    object: &mut serde_json::Map<String, Value>,
    sections: &Sections<'_>,
    lines: &LineIndex,
) {
    for hit in object
        .get_mut("matches")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let Some(line) = hit["line"]
            .as_u64()
            .and_then(|line| usize::try_from(line).ok())
        else {
            continue;
        };
        if let Some(section) = sections.innermost(lines.start(line - 1)) {
            hit["in"] = json!(sections.cited(section));
        }
    }
}

/// The loaded entry a search result was indexed from.
fn located<'a>(entries: &[Entry<'a>], hit: &SearchHit) -> Option<Entry<'a>> {
    let key = hit.id.as_deref().or(hit.key.as_deref())?;
    entries.iter().copied().find(|entry| match entry {
        Entry::Document { path, .. } => hit.kind == "source_document" && *path == key,
        Entry::Recipe(_) => hit.kind == "solution" && entry.source_path() == key,
        _ => entry.kind() == hit.kind && entry.key() == key && entry.source_matches(&hit.source),
    })
}

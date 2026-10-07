//! `ara find`: BM25 ranking with source-mapped hit lines.
use super::hits::{self, LineIndex};
use super::spans::SpanCache;
use super::{Artifact, Entry, ReadOptions, ShowArgs, show_loaded};
use crate::output::AgentError;
use crate::search::SearchHit;
use serde_json::{Value, json};
use std::collections::BTreeMap;
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
    #[command(flatten)]
    pub output: ReadOptions,
}

pub fn find(root: &Path, args: &FindArgs) -> Result<Value, AgentError> {
    let artifact = Artifact::load(root)?;
    let hits = crate::search::run_search_with_documents(
        &artifact.manifest,
        artifact.searchable_documents(),
        &args.query,
        args.kind.as_deref(),
        args.limit,
    )
    .map_err(|message| AgentError::semantic("invalid_search", message))?;
    let entries = artifact.entries();
    let addresses = artifact.entry_addresses();
    let mut cache = SpanCache::default();
    let mut lines = BTreeMap::<String, LineIndex>::new();
    let mut results = Vec::with_capacity(hits.len());
    for hit in &hits {
        let mut result = serde_json::to_value(hit).expect("search result serialization");
        let object = result.as_object_mut().expect("search result object");
        let entry = located(&entries, hit);
        if args.output.brief()
            && let Some(entry) = entry
        {
            object.insert("address".into(), json!(addresses.address(entry)));
        }
        if let Some(span) = entry.and_then(|entry| artifact.search_span(entry, &mut cache)) {
            let index = lines
                .entry(span.path.to_string())
                .or_insert_with(|| LineIndex::new(span.text));
            object.extend(hits::fields(&span, index, &hit.terms, args.context));
        } else {
            object.insert("match_count".into(), json!(0));
        }
        let selector = entry
            .map(|entry| addresses.selector(entry))
            .or_else(|| hit.id.clone().or_else(|| hit.key.clone()));
        if args.output.full
            && let Some(id) = selector
        {
            // The embedded entry is the JSON projection, never brief data.
            let mut show = show_loaded(
                &artifact,
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
        results.push(result);
    }
    Ok(json!({"format":"ara.find/v1","results":results,"diagnostics":artifact.diagnostics()}))
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

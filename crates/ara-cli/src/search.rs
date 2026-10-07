//! Stateless keyword ranking and advisory lexical duplicate similarity.
//! Tokens are maximal Unicode alphanumeric runs, lowercased with Unicode's
//! deterministic lowercase mapping. Punctuation separates tokens (`N12` stays
//! intact); no stemming, accent folding, locale, model, or network is involved.
use ara_core::Manifest;
use serde::Serialize;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::ops::Range;
use std::sync::OnceLock;

const K1: f64 = 1.2;
const B: f64 = 0.75;
pub const DEFAULT_DUPLICATE_THRESHOLD: f64 = 0.8;
pub const DEFAULT_DUPLICATE_LIMIT: usize = 10;
const RETRIEVAL_LIMIT: usize = 64;
const MAX_LIMIT: usize = 10_000;

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub kind: String,
    pub source: String,
    pub score: f64,
    pub excerpt: String,
    /// Query terms this result's indexed text contains, in lexical order.
    #[serde(skip)]
    pub terms: Vec<String>,
}

struct Document<'a> {
    id: Option<String>,
    key: Option<String>,
    kind: String,
    source: String,
    text: Cow<'a, str>,
    terms: BTreeMap<String, usize>,
    length: usize,
    term_range: Range<usize>,
}
impl<'a> Document<'a> {
    fn new(
        id: Option<String>,
        key: Option<String>,
        kind: String,
        source: String,
        text: String,
    ) -> Self {
        Self::with_text(id, key, kind, source, Cow::Owned(text))
    }
    fn with_text(
        id: Option<String>,
        key: Option<String>,
        kind: String,
        source: String,
        text: Cow<'a, str>,
    ) -> Self {
        let mut terms = BTreeMap::new();
        let mut length = 0;
        for (token, _, _) in tokens(&text) {
            *terms.entry(token).or_default() += 1;
            length += 1;
        }
        Self {
            id,
            key,
            kind,
            source,
            text,
            terms,
            length,
            term_range: 0..0,
        }
    }
    fn identity(&self) -> &str {
        self.id.as_deref().or(self.key.as_deref()).unwrap_or("")
    }
}

/// Lowercased maximal alphanumeric runs with their byte ranges.
pub(crate) fn tokens(text: &str) -> impl Iterator<Item = (String, usize, usize)> + '_ {
    let mut start = None;
    text.char_indices()
        .chain(std::iter::once((text.len(), ' ')))
        .filter_map(move |(offset, character)| {
            if character.is_alphanumeric() {
                start.get_or_insert(offset);
                None
            } else {
                start
                    .take()
                    .map(|begin| (text[begin..offset].to_lowercase(), begin, offset))
            }
        })
}

#[derive(Default)]
struct TermFrequency {
    count: usize,
    weight: OnceLock<f64>,
}

struct Corpus<'a> {
    documents: Vec<Document<'a>>,
    term_positions: BTreeMap<String, usize>,
    frequencies: Vec<TermFrequency>,
    document_terms: Vec<usize>,
    postings: BTreeMap<String, Vec<(usize, usize)>>,
    average_length: f64,
    total_length: usize,
    normalizations: OnceLock<Vec<f64>>,
}
impl<'a> Corpus<'a> {
    fn new(documents: Vec<Document<'a>>) -> Self {
        let (total_length, term_count) =
            documents.iter().fold((0, 0), |(length, terms), document| {
                (length + document.length, terms + document.terms.len())
            });
        let average_length = if documents.is_empty() {
            1.0
        } else {
            (total_length as f64 / documents.len() as f64).max(1.0)
        };
        let mut corpus = Self {
            documents,
            term_positions: BTreeMap::new(),
            frequencies: Vec::new(),
            document_terms: Vec::with_capacity(term_count),
            postings: BTreeMap::new(),
            average_length,
            total_length,
            normalizations: OnceLock::new(),
        };
        for position in 0..corpus.documents.len() {
            corpus.index_document(position);
        }
        corpus
    }
    fn index_document(&mut self, position: usize) {
        let document = &mut self.documents[position];
        let start = self.document_terms.len();
        // Stable term positions follow each document's lexical key order.
        for (term, frequency) in &document.terms {
            let term_position = *self.term_positions.entry(term.clone()).or_insert_with(|| {
                let position = self.frequencies.len();
                self.frequencies.push(TermFrequency::default());
                position
            });
            self.frequencies[term_position].count += 1;
            self.document_terms.push(term_position);
            self.postings
                .entry(term.clone())
                .or_default()
                .push((position, *frequency));
        }
        document.term_range = start..self.document_terms.len();
    }
    fn weight(&self, term: &str) -> f64 {
        match self.term_positions.get(term) {
            Some(&position) => self.term_weight(position),
            None => self.frequency_weight(0),
        }
    }
    fn term_weight(&self, position: usize) -> f64 {
        let frequency = &self.frequencies[position];
        *frequency
            .weight
            .get_or_init(|| self.frequency_weight(frequency.count))
    }
    fn frequency_weight(&self, frequency: usize) -> f64 {
        let frequency = frequency as f64;
        (1.0 + (self.documents.len() as f64 - frequency + 0.5) / (frequency + 0.5)).ln()
    }
    fn append(&mut self, document: Document<'a>) {
        // IDF always changes. Length normalizations depend only on the average.
        let old_average = self.average_length;
        let added_length = document.length;
        for frequency in &mut self.frequencies {
            frequency.weight.take();
        }
        self.total_length += document.length;
        self.documents.push(document);
        self.index_document(self.documents.len() - 1);
        self.average_length = (self.total_length as f64 / self.documents.len() as f64).max(1.0);
        if self.average_length == old_average {
            if let Some(normalizations) = self.normalizations.get_mut() {
                normalizations.push(K1 * (1.0 - B + B * added_length as f64 / self.average_length));
            }
        } else {
            self.normalizations.take();
        }
    }
    fn ranked(
        &self,
        query: &BTreeSet<String>,
        kind: Option<&str>,
        limit: usize,
    ) -> Vec<Ranked<'_>> {
        let mut heap = BinaryHeap::with_capacity(limit.min(self.documents.len()));
        let normalizations = self.normalizations.get_or_init(|| {
            self.documents
                .iter()
                .map(|document| K1 * (1.0 - B + B * document.length as f64 / self.average_length))
                .collect()
        });
        let mut scores = vec![0.0; self.documents.len()];
        // Preserve the original lexical term summation order and exact formula.
        for term in query {
            let weight = self.weight(term);
            if let Some(postings) = self.postings.get(term) {
                for &(position, frequency) in postings {
                    let frequency = frequency as f64;
                    scores[position] +=
                        weight * frequency * (K1 + 1.0) / (frequency + normalizations[position]);
                }
            }
        }
        for (document, score) in self.documents.iter().zip(scores) {
            if kind.is_some_and(|kind| kind != document.kind) {
                continue;
            }
            if score <= 0.0 || !score.is_finite() {
                continue;
            }
            let candidate = Ranked { score, document };
            if heap.len() < limit {
                heap.push(candidate);
            } else if heap.peek().is_some_and(|worst| candidate < *worst) {
                *heap.peek_mut().expect("bounded ranking heap is nonempty") = candidate;
            }
        }
        let mut ranked = heap.into_vec();
        ranked.sort();
        ranked
    }
}
struct Ranked<'a> {
    score: f64,
    document: &'a Document<'a>,
}
impl PartialEq for Ranked<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Ranked<'_> {}
impl PartialOrd for Ranked<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Ranked<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .total_cmp(&self.score)
            .then_with(|| self.document.kind.cmp(&other.document.kind))
            .then_with(|| self.document.source.cmp(&other.document.source))
            .then_with(|| self.document.identity().cmp(other.document.identity()))
    }
}

fn excerpt(text: &str, query: &BTreeSet<String>) -> String {
    let first = tokens(text)
        .find(|(term, _, _)| query.contains(term))
        .map_or(0, |(_, start, _)| start);
    let start = text[..first]
        .char_indices()
        .rev()
        .nth(59)
        .map_or(0, |(offset, _)| offset);
    let leading = start > 0;
    let available = crate::output::EXCERPT_CHARS - usize::from(leading);
    let mut end = text[start..]
        .char_indices()
        .nth(available)
        .map_or(text.len(), |(offset, _)| start + offset);
    let trailing = end < text.len();
    if trailing {
        end = text[start..end]
            .char_indices()
            .next_back()
            .map_or(start, |(offset, _)| start + offset);
    }
    let mut result =
        String::with_capacity(end - start + usize::from(leading) + usize::from(trailing));
    if leading {
        result.push('…');
    }
    let mut space = false;
    for character in text[start..end].chars() {
        if character.is_whitespace() {
            space = !result.is_empty();
            continue;
        }
        if space {
            result.push(' ');
            space = false;
        }
        result.push(character);
    }
    if trailing {
        result.push('…');
    }
    result
}

pub fn run_search(
    manifest: &Manifest,
    query: &str,
    kind: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    rank(documents(manifest)?, query, kind, limit)
}
pub fn run_search_with_documents<'a>(
    manifest: &'a Manifest,
    source_documents: impl IntoIterator<Item = (&'a str, &'a str)>,
    query: &str,
    kind: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    let mut corpus: Vec<Document<'a>> = documents(manifest)?;
    for (source, text) in source_documents {
        if !source.ends_with(".md")
            || source
                .split('/')
                .any(|part| matches!(part, ".." | "." | ".git" | ".ara" | "src" | "evidence"))
        {
            return Err("source document outside knowledge boundary".into());
        }
        let document = Document::with_text(
            None,
            Some(source.into()),
            "source_document".into(),
            source.into(),
            Cow::Borrowed(text),
        );
        if let Some(position) = corpus.iter().position(|entry| {
            entry.kind == "source_document" && entry.key.as_deref() == Some(source)
        }) {
            corpus[position] = document;
        } else {
            corpus.push(document);
        }
    }
    rank(corpus, query, kind, limit)
}
fn rank(
    documents: Vec<Document<'_>>,
    query: &str,
    kind: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    validate_limit(limit).map_err(|error| error.message)?;
    let query: BTreeSet<_> = tokens(query).map(|(term, _, _)| term).collect();
    if query.is_empty() {
        return Err("search query must contain a Unicode letter or number".into());
    }
    let corpus = Corpus::new(documents);
    if let Some(kind) = kind
        && !supported_kind(kind)
        && !corpus
            .documents
            .iter()
            .any(|document| document.kind == kind)
    {
        return Err(format!("unknown search kind: {kind}"));
    }
    Ok(corpus
        .ranked(&query, kind, limit)
        .into_iter()
        .map(|ranked| SearchHit {
            id: ranked.document.id.clone(),
            key: ranked.document.key.clone(),
            kind: ranked.document.kind.clone(),
            source: ranked.document.source.clone(),
            score: ranked.score,
            excerpt: excerpt(&ranked.document.text, &query),
            terms: query
                .iter()
                .filter(|term| ranked.document.terms.contains_key(*term))
                .cloned()
                .collect(),
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct AdvisoryUnavailable {
    pub code: &'static str,
    pub message: String,
}
impl std::fmt::Display for AdvisoryUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for AdvisoryUnavailable {}
fn unavailable(message: impl Into<String>) -> AdvisoryUnavailable {
    AdvisoryUnavailable {
        code: "duplicate_advisory_unavailable",
        message: message.into(),
    }
}
fn validate_limit(limit: usize) -> Result<(), AdvisoryUnavailable> {
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(unavailable(format!(
            "limit must be between 1 and {MAX_LIMIT}"
        )));
    }
    Ok(())
}
fn validate_duplicate(threshold: f64, limit: usize) -> Result<(), AdvisoryUnavailable> {
    validate_limit(limit)?;
    if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
        return Err(unavailable(
            "similarity threshold must be finite and between 0 and 1",
        ));
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateCandidate {
    pub id: String,
    pub similarity: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct DuplicatePair {
    pub left: String,
    pub right: String,
    pub similarity: f64,
}
#[derive(Debug, Clone)]
pub struct DuplicateText {
    pub id: String,
    pub title: String,
    pub body: String,
}

/// Exact normalized title and substantive typed body used by duplicate scoring.
/// Identifiers, timestamps, provenance and evidence pointers are not similarity.
pub fn node_duplicate_text(node: &ara_core::Node) -> Result<String, String> {
    let mut text = String::new();
    for value in [node.description.as_deref(), node.thinking.as_deref()]
        .into_iter()
        .flatten()
    {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(value);
    }
    let fields = serde_json::to_value(&node.fields)
        .map_err(|error| format!("cannot index entry: {error}"))?;
    if !fields.is_string() {
        append_text(&fields, &mut text);
    }
    Ok(text)
}

/// Snapshot-owned index. Similarity is lexical weighted Jaccard, NOT a probability.
pub struct DuplicateIndex {
    corpus: Corpus<'static>,
}
impl DuplicateIndex {
    pub fn new(manifest: &Manifest) -> Result<Self, AdvisoryUnavailable> {
        let documents = manifest
            .nodes
            .iter()
            .map(|node| {
                entry_document(
                    node,
                    Some(node.id.as_str()),
                    None,
                    "node",
                    "trace/exploration_tree.yaml",
                    &["label", "description", "thinking", "fields"],
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(unavailable)?;
        Ok(Self {
            corpus: Corpus::new(documents),
        })
    }
    pub fn from_texts(texts: &[DuplicateText]) -> Self {
        let documents = texts
            .iter()
            .map(|text| {
                Document::new(
                    Some(text.id.clone()),
                    None,
                    "node".into(),
                    "trace/exploration_tree.yaml".into(),
                    format!("{}\n{}", text.title, text.body),
                )
            })
            .collect();
        Self {
            corpus: Corpus::new(documents),
        }
    }
    pub fn candidates(
        &self,
        proposed_text: &str,
        exclude_id: Option<&str>,
        threshold: f64,
        limit: usize,
    ) -> Result<Vec<DuplicateCandidate>, AdvisoryUnavailable> {
        validate_duplicate(threshold, limit)?;
        let terms: BTreeSet<_> = tokens(proposed_text).map(|(term, _, _)| term).collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let query_weight = terms.iter().map(|term| self.corpus.weight(term)).sum();
        let mut candidates = Vec::new();
        for ranked in self.corpus.ranked(
            &terms,
            None,
            RETRIEVAL_LIMIT + usize::from(exclude_id.is_some()),
        ) {
            let id = ranked.document.identity();
            if exclude_id == Some(id) {
                continue;
            }
            let positions = &self.corpus.document_terms[ranked.document.term_range.clone()];
            let similarity = weighted_jaccard(
                &terms,
                query_weight,
                ranked
                    .document
                    .terms
                    .keys()
                    .zip(positions)
                    .map(|(term, &position)| (term, self.corpus.term_weight(position))),
            );
            if similarity > 0.0 && similarity >= threshold {
                candidates.push(DuplicateCandidate {
                    id: id.into(),
                    similarity,
                });
            }
        }
        candidates.sort_by(|a, b| {
            b.similarity
                .total_cmp(&a.similarity)
                .then_with(|| a.id.cmp(&b.id))
        });
        candidates.truncate(limit);
        Ok(candidates)
    }
}
fn weighted_jaccard<'a>(
    left: &BTreeSet<String>,
    mut union: f64,
    right: impl Iterator<Item = (&'a String, f64)>,
) -> f64 {
    let mut intersection = 0.0;
    for (term, weight) in right {
        if left.contains(term) {
            intersection += weight;
        } else {
            union += weight;
        }
    }
    if union == 0.0 {
        return 0.0;
    }
    (intersection / union).clamp(0.0, 1.0)
}

/// Compare additions to committed entries and earlier additions, emitting each
/// unordered identity pair once. The output and retrieval working set are bounded.
pub fn duplicate_pairs(
    existing: &[DuplicateText],
    added: &[DuplicateText],
    threshold: f64,
    limit: usize,
) -> Result<Vec<DuplicatePair>, AdvisoryUnavailable> {
    validate_duplicate(threshold, limit)?;
    let mut index = DuplicateIndex::from_texts(existing);
    let identities: BTreeSet<_> = existing
        .iter()
        .chain(added)
        .map(|text| text.id.as_str())
        .collect();
    if identities.len() != existing.len() + added.len() {
        return Err(unavailable(
            "duplicate pair input contains repeated identities; exclude mapped or repeated imports before scoring",
        ));
    }
    let mut pairs = Vec::new();
    for text in added {
        let proposed = format!("{}\n{}", text.title, text.body);
        for candidate in index.candidates(&proposed, None, threshold, RETRIEVAL_LIMIT)? {
            let (left, right) = if text.id < candidate.id {
                (text.id.clone(), candidate.id)
            } else {
                (candidate.id, text.id.clone())
            };
            pairs.push(DuplicatePair {
                left,
                right,
                similarity: candidate.similarity,
            });
            pairs.sort_by(|a, b| {
                b.similarity
                    .total_cmp(&a.similarity)
                    .then_with(|| a.left.cmp(&b.left))
                    .then_with(|| a.right.cmp(&b.right))
            });
            pairs.truncate(limit);
        }
        let document = Document::new(
            Some(text.id.clone()),
            None,
            "node".into(),
            "trace/exploration_tree.yaml".into(),
            proposed,
        );
        index.corpus.append(document);
    }
    Ok(pairs)
}

fn supported_kind(kind: &str) -> bool {
    matches!(
        kind,
        "question"
            | "experiment"
            | "decision"
            | "dead_end"
            | "insight"
            | "pivot"
            | "other"
            | "claim"
            | "related_work"
            | "concept"
            | "source_document"
            | "solution"
            | "exhibit"
            | "observation"
            | "session"
            | "heuristic"
            | "experiment_plan"
            | "taste"
    )
}

fn node_document(node: &ara_core::Node) -> Result<Document<'static>, String> {
    let kind = match &node.kind {
        ara_core::NodeKind::Question => "question",
        ara_core::NodeKind::Experiment => "experiment",
        ara_core::NodeKind::Decision => "decision",
        ara_core::NodeKind::DeadEnd => "dead_end",
        ara_core::NodeKind::Insight => "insight",
        ara_core::NodeKind::Pivot => "pivot",
        ara_core::NodeKind::Other(name) => name.as_str(),
    };
    entry_document(
        node,
        Some(node.id.as_str()),
        None,
        kind,
        "trace/exploration_tree.yaml",
        &[
            "id",
            "label",
            "description",
            "thinking",
            "fields",
            "evidence_notes",
            "source_refs",
            "support_level",
            "provenance",
            "timestamp",
        ],
    )
}

// Only explicitly supported fields enter the index. Serialization is confined
// to each typed entry, never the manifest, and never reads any referenced file.
fn entry_document(
    value: &impl Serialize,
    id: Option<&str>,
    key: Option<&str>,
    kind: &str,
    source: &str,
    fields: &[&str],
) -> Result<Document<'static>, String> {
    Ok(Document::new(
        id.map(str::to_owned),
        key.map(str::to_owned),
        kind.into(),
        source.into(),
        entry_text(value, fields)?,
    ))
}
fn entry_text(value: &impl Serialize, fields: &[&str]) -> Result<String, String> {
    let value =
        serde_json::to_value(value).map_err(|error| format!("cannot index entry: {error}"))?;
    let mut text = String::new();
    for field in fields {
        if let Some(value) = value.get(*field)
            && (*field != "fields" || !value.is_string())
        {
            append_text(value, &mut text);
        }
    }
    Ok(text)
}

fn append_text(value: &serde_json::Value, text: &mut String) {
    match value {
        serde_json::Value::String(value) => {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(value);
        }
        serde_json::Value::Array(values) => {
            for value in values {
                append_text(value, text);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                append_text(value, text);
            }
        }
        _ => {}
    }
}

fn canonical_document<'a>(id: &str, body: &'a str, kind: &str, source: &str) -> Document<'a> {
    Document::with_text(
        Some(id.into()),
        None,
        kind.into(),
        source.into(),
        Cow::Borrowed(body),
    )
}
fn documents(manifest: &Manifest) -> Result<Vec<Document<'_>>, String> {
    let mut result = Vec::new();
    for node in &manifest.nodes {
        result.push(node_document(node)?);
    }
    for claim in &manifest.claims {
        if let Some(body) = &claim.body {
            result.push(canonical_document(
                claim.id.as_str(),
                body,
                "claim",
                "logic/claims.md",
            ));
        } else {
            result.push(entry_document(
                claim,
                Some(claim.id.as_str()),
                None,
                "claim",
                "logic/claims.md",
                &[
                    "id",
                    "title",
                    "statement",
                    "status",
                    "proof",
                    "deps",
                    "proof_content",
                    "provenance",
                    "falsification",
                    "conditions",
                    "sources",
                    "tags",
                    "last_revised",
                ],
            )?);
        }
    }
    for work in &manifest.related_work {
        result.push(entry_document(
            work,
            Some(&work.id),
            None,
            "related_work",
            "logic/related_work.md",
            &[
                "id",
                "cite",
                "doi",
                "kind",
                "what_changed",
                "why",
                "adopted",
                "claims_affected",
            ],
        )?);
    }
    for concept in &manifest.concepts {
        result.push(entry_document(
            concept,
            None,
            Some(&concept.term),
            "concept",
            "logic/concepts.md",
            &["term", "notation", "definition", "boundary", "related"],
        )?);
    }
    if let Some(problem) = &manifest.problem {
        result.push(entry_document(
            problem,
            None,
            Some("logic/problem.md"),
            "source_document",
            "logic/problem.md",
            &["statement", "observations", "gaps", "insights"],
        )?);
    }
    if let Some(paper) = &manifest.paper {
        result.push(entry_document(
            paper,
            None,
            Some("PAPER.md"),
            "source_document",
            "PAPER.md",
            &[
                "title", "authors", "year", "venue", "doi", "abstract", "keywords",
            ],
        )?);
    }
    for recipe in &manifest.recipes {
        // Typed H entries take the place of the raw heuristic file.
        if recipe.name == "heuristics" {
            continue;
        }
        let source = format!("logic/solution/{}.md", recipe.name);
        result.push(entry_document(
            recipe,
            None,
            Some(&source),
            "solution",
            &source,
            &["name", "title", "body"],
        )?);
    }
    for exhibit in &manifest.exhibits {
        result.push(entry_document(
            exhibit,
            Some(&exhibit.id),
            None,
            "exhibit",
            "evidence/README.md",
            &[
                "id",
                "file",
                "kind",
                "source",
                "description",
                "claims",
                "image",
            ],
        )?);
    }
    for entry in &manifest.observations {
        result.push(entry_document(
            entry,
            Some(entry.id.as_str()),
            None,
            "observation",
            &entry.source_file,
            &[
                "id",
                "content",
                "timestamp",
                "provenance",
                "context",
                "potential_type",
                "promoted_to",
                "crystallized_via",
                "bound_to",
                "extra",
            ],
        )?);
    }
    for entry in &manifest.sessions {
        if !entry.body.is_empty() {
            result.push(canonical_document(
                entry.id.as_str(),
                &entry.body,
                "session",
                &entry.source_file,
            ));
        } else {
            result.push(entry_document(
                entry,
                Some(entry.id.as_str()),
                None,
                "session",
                &entry.source_file,
                &[
                    "id",
                    "date",
                    "started",
                    "last_turn",
                    "summary",
                    "events_logged",
                    "ai_actions",
                    "claims_touched",
                    "logic_revisions",
                    "key_context",
                    "open_threads",
                    "ai_suggestions_pending",
                    "metadata_extra",
                    "extra",
                ],
            )?);
        }
    }
    for entry in &manifest.heuristics {
        if !entry.body.is_empty() {
            result.push(canonical_document(
                entry.id.as_str(),
                &entry.body,
                "heuristic",
                &entry.source_file,
            ));
        } else {
            result.push(entry_document(
                entry,
                Some(entry.id.as_str()),
                None,
                "heuristic",
                &entry.source_file,
                &[
                    "id",
                    "title",
                    "rationale",
                    "sources",
                    "status",
                    "provenance",
                    "sensitivity",
                    "code_ref",
                    "last_revised",
                ],
            )?);
        }
    }
    for entry in &manifest.experiment_plans {
        if !entry.body.is_empty() {
            result.push(canonical_document(
                entry.id.as_str(),
                &entry.body,
                "experiment_plan",
                &entry.source_file,
            ));
        } else {
            result.push(entry_document(
                entry,
                Some(entry.id.as_str()),
                None,
                "experiment_plan",
                &entry.source_file,
                &[
                    "id",
                    "title",
                    "status",
                    "evidence_output",
                    "question",
                    "setup",
                    "prediction",
                    "falsification",
                    "provenance",
                    "last_revised",
                ],
            )?);
        }
    }
    for entry in &manifest.taste_comments {
        result.push(entry_document(
            entry,
            Some(entry.id.as_str()),
            None,
            "taste",
            &entry.source_file,
            &[
                "id",
                "target",
                "comment",
                "timestamp",
                "tag",
                "object",
                "extra",
            ],
        )?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(id: &str, kind: &str, text: &str) -> Document<'static> {
        Document::new(
            Some(id.into()),
            None,
            kind.into(),
            "trace/exploration_tree.yaml".into(),
            text.into(),
        )
    }
    fn query(text: &str) -> BTreeSet<String> {
        tokens(text).map(|(term, _, _)| term).collect()
    }

    #[test]
    fn posting_scores_match_full_scan_formula_and_tie_order_exactly() {
        let corpus = Corpus::new(vec![
            Document::new(
                Some("N03".into()),
                None,
                "node".into(),
                "trace".into(),
                "雪 rare rare common".into(),
            ),
            Document::new(
                Some("N01".into()),
                None,
                "node".into(),
                "trace".into(),
                "common long repeated repeated words".into(),
            ),
            Document::new(
                Some("N02".into()),
                None,
                "node".into(),
                "trace".into(),
                "雪 rare rare common".into(),
            ),
        ]);
        let query = tokens("rare 雪 common")
            .map(|(term, _, _)| term)
            .collect::<BTreeSet<_>>();
        let mut reference = corpus
            .documents
            .iter()
            .filter_map(|document| {
                let normalization =
                    K1 * (1.0 - B + B * document.length as f64 / corpus.average_length);
                let mut score = 0.0;
                for term in &query {
                    let frequency = *document.terms.get(term).unwrap_or(&0) as f64;
                    if frequency != 0.0 {
                        score += corpus.weight(term) * frequency * (K1 + 1.0)
                            / (frequency + normalization);
                    }
                }
                (score > 0.0).then_some(Ranked { score, document })
            })
            .collect::<Vec<_>>();
        reference.sort();
        let observed = corpus.ranked(&query, None, 10);
        assert_eq!(
            observed
                .iter()
                .map(|ranked| (ranked.document.identity(), ranked.score))
                .collect::<Vec<_>>(),
            reference
                .iter()
                .map(|ranked| (ranked.document.identity(), ranked.score))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn rare_terms_length_and_repetition_affect_order() {
        let corpus = Corpus::new(vec![
            document("N1", "question", "common common common"),
            document("N2", "question", "rare"),
            document(
                "N3",
                "question",
                "common padding padding padding padding padding",
            ),
        ]);
        let ranked = corpus.ranked(&query("common rare"), None, 3);
        assert_eq!(
            ranked
                .iter()
                .map(|entry| entry.document.identity())
                .collect::<Vec<_>>(),
            ["N2", "N1", "N3"]
        );
        let corpus = Corpus::new(vec![
            document("N1", "question", "target filler"),
            document("N2", "question", "target target"),
        ]);
        assert_eq!(
            corpus.ranked(&query("target"), None, 2)[0]
                .document
                .identity(),
            "N2"
        );
    }

    #[test]
    fn unicode_punctuation_ids_and_exact_snippets() {
        assert_eq!(query("N12—CAFÉ/实验"), query("n12 café 实验"));
        let text = format!("{}\\nCAFÉ—实验", "é".repeat(100));
        let snippet = excerpt(&text, &query("café"));
        assert!(snippet.contains("CAFÉ—实验"));
        assert!(!snippet.contains('\n'));
        assert!(!snippet.contains('\u{fffd}'));
    }

    #[test]
    fn filtering_preserves_scores_and_ties_are_stable() {
        let corpus = Corpus::new(vec![
            document("N2", "question", "same"),
            document("N1", "question", "same"),
            document("N3", "experiment", "same"),
        ]);
        let all = corpus.ranked(&query("same"), None, 3);
        let filtered = corpus.ranked(&query("same"), Some("question"), 1);
        assert_eq!(filtered[0].document.identity(), "N1");
        assert_eq!(
            filtered[0].score,
            all.iter()
                .find(|entry| entry.document.identity() == "N1")
                .unwrap()
                .score
        );
        assert!(corpus.ranked(&query("absent"), None, 1).is_empty());
        assert!(
            Corpus::new(Vec::new())
                .ranked(&query("same"), None, 1)
                .is_empty()
        );
    }

    #[test]
    fn overlap_is_bounded_and_not_a_probability() {
        let same = query("same text");
        let different = query("different words");
        assert_eq!(
            weighted_jaccard(&same, 2.0, same.iter().map(|term| (term, 1.0))),
            1.0
        );
        assert_eq!(
            weighted_jaccard(&same, 2.0, different.iter().map(|term| (term, 1.0))),
            0.0
        );
        assert_eq!(
            weighted_jaccard(
                &query("a b"),
                2.0,
                query("b c").iter().map(|term| (term, 1.0))
            ),
            1.0 / 3.0
        );
        assert_eq!(weighted_jaccard(&query(""), 0.0, std::iter::empty()), 0.0);
        assert!(validate_duplicate(f64::NAN, 1).is_err());
        assert!(validate_duplicate(0.8, 0).is_err());
    }

    #[test]
    fn batch_pairs_include_prior_added_entries_once_and_bound_output() {
        let existing = [DuplicateText {
            id: "N1".into(),
            title: "cache invalidation".into(),
            body: "source hashes invalidate stale artifacts".into(),
        }];
        let added = [
            DuplicateText {
                id: "N2".into(),
                title: existing[0].title.clone(),
                body: existing[0].body.clone(),
            },
            DuplicateText {
                id: "N3".into(),
                title: existing[0].title.clone(),
                body: existing[0].body.clone(),
            },
        ];
        let pairs = duplicate_pairs(&existing, &added, 0.8, 10).unwrap();
        assert_eq!(
            pairs
                .iter()
                .map(|pair| (pair.left.as_str(), pair.right.as_str()))
                .collect::<Vec<_>>(),
            [("N1", "N2"), ("N1", "N3"), ("N2", "N3")]
        );
        assert!(pairs.iter().all(|pair| pair.similarity == 1.0));
        assert_eq!(
            duplicate_pairs(&existing, &added, 0.8, 1).unwrap()[0].right,
            "N2"
        );
        assert!(duplicate_pairs(&existing, &existing, 0.8, 1).is_err());
        assert_eq!(existing[0].id, "N1");
        assert!(
            serde_json::to_string(&pairs)
                .unwrap()
                .contains("\"similarity\":1.0")
        );
    }

    #[test]
    fn duplicate_candidates_match_full_scan_after_snapshot_appends() {
        let texts = (0..72)
            .map(|position| DuplicateText {
                id: format!("N{position:02}"),
                title: "alpha 雪".into(),
                body: format!(
                    "{} unique{position} {}",
                    "alpha ".repeat(position % 7),
                    if position < 60 { "rare" } else { "other" }
                ),
            })
            .collect::<Vec<_>>();
        let uniform = (0..72)
            .map(|position| DuplicateText {
                id: format!("N{position:02}"),
                title: "alpha".into(),
                body: "rare".into(),
            })
            .collect::<Vec<_>>();
        for texts in [texts, uniform] {
            let mut index = DuplicateIndex::from_texts(&texts);
            let additions = [
                document("N72", "node", "雪 rare"),
                document(
                    "N73",
                    "node",
                    &format!("{} newterm", "alpha 雪 rare ".repeat(80)),
                ),
                document("N74", "node", ""),
                document("N75", "node", "雪 rare newterm newterm"),
            ];
            for addition in std::iter::once(None).chain(additions.into_iter().map(Some)) {
                if let Some(document) = addition {
                    index.corpus.append(document);
                }
                let terms = query("alpha 雪 rare newterm missing");
                let weight = |term: &str| {
                    let frequency = index
                        .corpus
                        .documents
                        .iter()
                        .filter(|document| document.terms.contains_key(term))
                        .count() as f64;
                    (1.0 + (index.corpus.documents.len() as f64 - frequency + 0.5)
                        / (frequency + 0.5))
                        .ln()
                };
                let mut reference = index
                    .corpus
                    .documents
                    .iter()
                    .filter_map(|document| {
                        let normalization = K1
                            * (1.0 - B + B * document.length as f64 / index.corpus.average_length);
                        let mut score = 0.0;
                        for term in &terms {
                            let frequency = *document.terms.get(term).unwrap_or(&0) as f64;
                            if frequency != 0.0 {
                                score += weight(term) * frequency * (K1 + 1.0)
                                    / (frequency + normalization);
                            }
                        }
                        (score > 0.0).then_some(Ranked { score, document })
                    })
                    .collect::<Vec<_>>();
                reference.sort();
                reference.truncate(RETRIEVAL_LIMIT + 1);
                let mut expected = reference
                    .iter()
                    .filter(|ranked| ranked.document.identity() != "N00")
                    .filter_map(|ranked| {
                        let mut union = terms.iter().map(|term| weight(term)).sum::<f64>();
                        let mut intersection = 0.0;
                        for term in ranked.document.terms.keys() {
                            if terms.contains(term) {
                                intersection += weight(term);
                            } else {
                                union += weight(term);
                            }
                        }
                        let similarity = (intersection / union).clamp(0.0, 1.0);
                        (similarity > 0.0).then_some((ranked.document.identity(), similarity))
                    })
                    .collect::<Vec<_>>();
                expected.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
                expected.truncate(10);
                for _ in 0..2 {
                    let ranked = index.corpus.ranked(&terms, None, RETRIEVAL_LIMIT + 1);
                    assert_eq!(
                        ranked
                            .iter()
                            .map(|ranked| (ranked.document.identity(), ranked.score))
                            .collect::<Vec<_>>(),
                        reference
                            .iter()
                            .map(|ranked| (ranked.document.identity(), ranked.score))
                            .collect::<Vec<_>>()
                    );
                    let observed = index
                        .candidates("alpha 雪 rare newterm missing", Some("N00"), 0.0, 10)
                        .unwrap();
                    assert_eq!(
                        observed
                            .iter()
                            .map(|candidate| (candidate.id.as_str(), candidate.similarity))
                            .collect::<Vec<_>>(),
                        expected
                    );
                }
            }
        }
    }

    #[test]
    fn same_title_and_contradictory_results_remain_distinct_candidates() {
        let texts = [DuplicateText {
            id: "N1".into(),
            title: "verification run".into(),
            body: "the treatment improves accuracy".into(),
        }];
        let index = DuplicateIndex::from_texts(&texts);
        assert!(
            index
                .candidates(
                    "verification run the treatment reduces accuracy",
                    None,
                    0.95,
                    10
                )
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            index
                .candidates(
                    "verification run the treatment improves accuracy",
                    None,
                    0.95,
                    10
                )
                .unwrap()[0]
                .id,
            "N1"
        );
        assert!(index.candidates("", None, 0.8, 10).unwrap().is_empty());
        assert!(
            index
                .candidates(
                    "verification run the treatment improves accuracy",
                    Some("N1"),
                    0.8,
                    10
                )
                .unwrap()
                .is_empty()
        );
    }

    fn fixture() -> Manifest {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ara-core/tests/fixtures/agent-cli");
        ara_core::parse_dir(&path)
            .expect("pinned real artifact must parse")
            .0
    }

    #[test]
    fn development_queries_find_attributed_native_identities() {
        let manifest = fixture();
        let labels: serde_json::Value = serde_json::from_str(include_str!(
            "../../ara-core/tests/fixtures/agent-cli/search/relevance.json"
        ))
        .unwrap();
        for example in labels["development"].as_array().unwrap() {
            let query = example["query"].as_str().unwrap();
            let hits = run_search(&manifest, query, None, 10).unwrap();
            for identity in example["relevant_ids"].as_array().unwrap() {
                assert!(
                    hits.iter()
                        .any(|hit| hit.id.as_deref() == identity.as_str()),
                    "{query}: missing {identity}"
                );
            }
        }
    }

    #[test]
    fn errors_empty_results_and_type_limits_follow_library_contract() {
        let manifest = fixture();
        assert!(run_search(&manifest, "paper", None, 0).is_err());
        assert!(run_search(&manifest, "paper", Some("imaginary"), 10).is_err());
        assert!(run_search(&manifest, "---", None, 10).is_err());
        assert!(
            run_search(&manifest, "zzzzunmatchabletoken", None, 10)
                .unwrap()
                .is_empty()
        );
        let hits = run_search(&manifest, "PaperBench", Some("decision"), 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "decision");
        assert!(hits[0].score.is_finite());
        let json = serde_json::to_value(hits).unwrap();
        assert!(json[0]["id"].as_str().unwrap().starts_with('N'));
        assert_eq!(json[0]["source"], "trace/exploration_tree.yaml");
    }

    #[test]
    fn evidence_body_is_never_searchable_but_metadata_is() {
        let mut manifest = fixture();
        manifest.exhibits = vec![ara_core::Exhibit {
            id: "E999".into(),
            file: "evidence/results/private.md".into(),
            kind: ara_core::ExhibitKind::Result,
            source: None,
            description: Some("metadataonlyneedle".into()),
            claims: Vec::new(),
            body: "privatebodyneedle".into(),
            image: None,
        }];
        assert!(
            run_search(&manifest, "privatebodyneedle", None, 10)
                .unwrap()
                .is_empty()
        );
        let hits = run_search(&manifest, "metadataonlyneedle", Some("exhibit"), 10).unwrap();
        assert_eq!(hits[0].id.as_deref(), Some("E999"));
    }
}

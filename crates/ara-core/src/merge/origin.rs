//! Peer-feedback provenance: proven inherited origins, repeated foreign source
//! facts, and the 3-way base of an entry that already arrived by another route.
//! The frozen representation is `docs/collaborative-research/provenance-contract.md`.
use super::identity::{self, Alias, Fact, Ledger, Record};
use super::types::{EntryIdentity, IdentityMap, ImportMapping, MergeError};
use std::collections::{BTreeMap, BTreeSet};

/// Reconciliation is pay-for-use: it runs only when the incoming ledger holds a
/// foreign source fact, or this destination received facts of the current
/// source through another route.
pub(crate) fn needed(ours: &Ledger, theirs: &Ledger, transport: &str) -> bool {
    theirs
        .records
        .iter()
        .filter_map(Record::fact)
        .any(|fact| fact.source_key != transport)
        || ours.records.iter().any(|record| {
            matches!(record, Record::InheritedRevision { source_key, .. } if source_key == transport)
        })
}

fn conflict(message: impl Into<String>) -> MergeError {
    MergeError::content("merge.foreign_mapping_conflict", message)
}
fn ambiguous(message: impl Into<String>) -> MergeError {
    MergeError::content("merge.ambiguous_origin", message)
}
fn originals<'a>(fact: &Fact<'a>) -> BTreeMap<&'a str, &'a ImportMapping> {
    fact.mappings
        .iter()
        .map(|mapping| (mapping.original.as_str(), mapping))
        .collect()
}
fn facts(ledger: &Ledger) -> BTreeMap<(&str, &str), Fact<'_>> {
    let mut result = BTreeMap::new();
    for fact in ledger.records.iter().filter_map(Record::fact) {
        result
            .entry((fact.source_key, fact.fingerprint))
            .or_insert(fact);
    }
    result
}

/// Incoming local identities whose origin this destination provably holds.
#[derive(Default)]
pub(crate) struct Origins {
    pub targets: BTreeMap<String, String>,
    /// Incoming local identity -> shared foreign fact used as its content base.
    /// Self origins never have one: their facts are asserted by the peer.
    bases: BTreeMap<String, BaseRef>,
}
struct BaseRef {
    key: String,
    fingerprint: String,
    original: String,
}
/// Inputs of origin reconciliation for one merge.
pub(crate) struct Context<'a> {
    pub ours: &'a Ledger,
    pub theirs: &'a Ledger,
    pub aliases: &'a [Alias],
    pub theirs_redirects: &'a BTreeMap<String, String>,
    pub theirs_entries: &'a [EntryIdentity],
    pub ours_entries: &'a [EntryIdentity],
    pub ours_redirects: &'a BTreeMap<String, String>,
    pub transport: &'a str,
    pub own: Option<&'a str>,
}
fn live(entries: &[EntryIdentity]) -> BTreeMap<&str, &EntryIdentity> {
    entries
        .iter()
        .filter(|entry| entry.layer != "historical_identity")
        .map(|entry| (entry.address.as_str(), entry))
        .collect()
}
/// Session files and their occurrences relocate with the session identity.
fn same_place(entry: &EntryIdentity, mapping: &ImportMapping) -> bool {
    entry.layer == mapping.layer
        && (matches!(
            entry.layer.as_str(),
            "document" | "session" | "session_occurrence"
        ) || entry.path == mapping.path)
}

/// Apply the proof rules. A foreign origin needs an incoming alias backed by an
/// incoming fact that this destination holds byte-identically. A self origin
/// needs an incoming alias under this destination's own key, backed by an
/// incoming fact of that key, whose original still exists here unchanged in
/// namespace, layer and path. Self facts prove identity only, never content.
pub(crate) fn reconcile(context: &Context) -> Result<Origins, MergeError> {
    let mut origins = Origins::default();
    let held = facts(context.ours);
    let mut shared = Vec::new();
    let mut mine = Vec::new();
    let mut seen = BTreeSet::new();
    for fact in context.theirs.records.iter().filter_map(Record::fact) {
        if fact.source_key == context.transport || !seen.insert((fact.source_key, fact.fingerprint))
        {
            continue;
        }
        if Some(fact.source_key) == context.own {
            mine.push(originals(&fact));
        } else if let Some(local) = held.get(&(fact.source_key, fact.fingerprint)) {
            if !identity::same_source_fact(&fact, local) {
                return Err(conflict(format!(
                    "foreign source `{}` revision {} has different source bytes or identities here",
                    fact.source_key, fact.fingerprint
                )));
            }
            shared.push((fact, originals(&fact), originals(local)));
        }
    }
    if shared.is_empty() && mine.is_empty() {
        return Ok(origins);
    }
    let incoming_live = live(context.theirs_entries);
    let present = live(context.ours_entries);
    for alias in context
        .aliases
        .iter()
        .filter(|alias| alias.source_key != context.transport)
    {
        let own = Some(alias.source_key.as_str()) == context.own;
        let (destination, proving, base) = if own {
            let Some(mapping) = mine.iter().rev().find_map(|incoming| {
                incoming
                    .get(alias.original.as_str())
                    .filter(|mapping| mapping.target == alias.target)
                    .copied()
            }) else {
                continue;
            };
            let destination = context
                .ours_redirects
                .get(&alias.original)
                .cloned()
                .unwrap_or_else(|| alias.original.clone());
            (destination, mapping, None)
        } else {
            let mut proven: Option<(&str, &ImportMapping, &Fact)> = None;
            for (fact, incoming, local) in &shared {
                if fact.source_key != alias.source_key {
                    continue;
                }
                let (Some(incoming), Some(local)) = (
                    incoming.get(alias.original.as_str()),
                    local.get(alias.original.as_str()),
                ) else {
                    continue;
                };
                if incoming.target != alias.target {
                    continue;
                }
                if proven.is_some_and(|(target, _, _)| target != local.target) {
                    return Err(ambiguous(format!(
                        "origin `{}:{}` has different destination identities across revisions",
                        alias.source_key, alias.original
                    )));
                }
                proven = Some((&local.target, incoming, fact));
            }
            let Some((destination, mapping, fact)) = proven else {
                continue;
            };
            (destination.to_owned(), mapping, Some(fact))
        };
        // Only live content can prove an origin. A historical identity was not
        // live in that source revision (a retired ID, or an external file the
        // source never installed), and external code/evidence is never
        // relocated: its identity is its path.
        if matches!(proving.layer.as_str(), "historical_identity" | "external") {
            continue;
        }
        let native = identity::normalize_local(&alias.target);
        let local = context
            .theirs_redirects
            .get(&native)
            .cloned()
            .unwrap_or(native);
        if let Some(entry) = incoming_live.get(local.as_str()) {
            // The proving mapping must describe the entry it is applied to.
            if !same_place(entry, proving) {
                return Err(ambiguous(format!(
                    "incoming `{local}` ({} in {}) is not the `{}` entry its origin `{}:{}` proves",
                    entry.layer, entry.path, proving.layer, alias.source_key, alias.original
                )));
            }
            // A live incoming entry that originated here must still exist here.
            if own
                && !present
                    .get(destination.as_str())
                    .is_some_and(|entry| same_place(entry, proving))
            {
                return Err(MergeError::content(
                    "merge.self_origin_missing",
                    format!(
                        "incoming `{local}` originated here as `{}:{}`, but `{destination}` no longer exists here with layer `{}` in `{}`",
                        alias.source_key, alias.original, proving.layer, proving.path
                    ),
                ));
            }
        }
        if let Some(old) = origins.targets.get(&local)
            && *old != destination
        {
            return Err(ambiguous(format!(
                "incoming `{local}` is proven to both `{old}` and `{destination}`"
            )));
        }
        origins.targets.insert(local.clone(), destination);
        if let Some(fact) = base {
            origins.bases.entry(local).or_insert_with(|| BaseRef {
                key: fact.source_key.to_owned(),
                fingerprint: fact.fingerprint.to_owned(),
                original: alias.original.clone(),
            });
        }
    }
    let mut owners = BTreeMap::new();
    for (local, destination) in &origins.targets {
        if let Some(other) = owners.insert(destination, local) {
            return Err(ambiguous(format!(
                "incoming `{other}` and `{local}` are both proven to `{destination}`"
            )));
        }
    }
    Ok(origins)
}

/// Transport foreign provenance. Source facts are compared, never import
/// events; facts this destination lacks become `inherited_revision` records.
pub(crate) fn foreign_history(
    ours: &Ledger,
    incoming: &Ledger,
    map: &IdentityMap,
    transport_key: &str,
    transport_revision: &str,
    own: Option<&str>,
) -> Result<Vec<Record>, MergeError> {
    let mut enrolled = BTreeSet::new();
    let mut labels = BTreeSet::new();
    let mut transports = BTreeMap::new();
    for record in &ours.records {
        match record {
            Record::Enrollment {
                source_key, label, ..
            } => {
                enrolled.insert(source_key.clone());
                labels.insert((source_key.clone(), label.clone()));
            }
            Record::Label {
                source_key, label, ..
            } => {
                labels.insert((source_key.clone(), label.clone()));
            }
            Record::Transport {
                source_key,
                revision,
                path,
                ..
            } => {
                transports.insert((source_key.clone(), revision.clone(), path.clone()), record);
            }
            _ => {}
        }
    }
    let held = facts(ours);
    // Every identity this destination already holds per (source, original).
    let mut known: BTreeMap<(String, String), String> = BTreeMap::new();
    for fact in ours.records.iter().filter_map(Record::fact) {
        for mapping in fact.mappings {
            known
                .entry((fact.source_key.to_owned(), mapping.original.clone()))
                .or_insert_with(|| mapping.target.clone());
        }
    }
    let mut added: BTreeMap<(String, String), Record> = BTreeMap::new();
    let mut additions = Vec::new();
    for record in &incoming.records {
        // This destination is the source of truth for its own history; its
        // facts arriving back stay only in the transported bytes.
        if own.is_some_and(|own| record_key(record) == Some(own)) {
            continue;
        }
        match record {
            Record::Enrollment {
                source_key, label, ..
            } if source_key != transport_key => {
                if enrolled.insert(source_key.clone()) {
                    labels.insert((source_key.clone(), label.clone()));
                    additions.push(record.clone());
                }
            }
            Record::Label {
                source_key, label, ..
            } if source_key != transport_key => {
                if labels.insert((source_key.clone(), label.clone())) {
                    additions.push(record.clone());
                }
            }
            Record::Revision { source_key, .. } | Record::InheritedRevision { source_key, .. }
                if source_key != transport_key =>
            {
                let fact = record.fact().expect("source fact record");
                let key = (fact.source_key.to_owned(), fact.fingerprint.to_owned());
                let existing = held
                    .get(&(fact.source_key, fact.fingerprint))
                    .copied()
                    .or_else(|| added.get(&key).and_then(Record::fact));
                if let Some(existing) = existing {
                    compare(&fact, &existing, map)?;
                    continue;
                }
                let mut mappings = Vec::with_capacity(fact.mappings.len());
                for mapping in fact.mappings {
                    let target = match map.get(&mapping.target) {
                        Some(target) => target.clone(),
                        // External code/evidence keeps its frozen source path.
                        None if mapping.layer == "external" => mapping.target.clone(),
                        None => {
                            return Err(conflict(format!(
                                "foreign historical target `{}` has no provable transport identity",
                                mapping.target
                            )));
                        }
                    };
                    // A fact this destination never imported must not assign a
                    // second identity to an origin it already holds; that would
                    // break every later import of the same source.
                    let slot = (fact.source_key.to_owned(), mapping.original.clone());
                    if let Some(held) = known.get(&slot)
                        && *held != target
                    {
                        return Err(MergeError::content(
                            "merge.unshared_origin_revision",
                            format!(
                                "origin `{}:{}` is `{held}` here, but the unshared revision {} would map it to `{target}`; import that source revision here first",
                                fact.source_key, mapping.original, fact.fingerprint
                            ),
                        ));
                    }
                    known.insert(slot, target.clone());
                    mappings.push(ImportMapping {
                        target,
                        ..mapping.clone()
                    });
                }
                let inherited = Record::InheritedRevision {
                    source_key: key.0.clone(),
                    fingerprint: key.1.clone(),
                    files: fact.files.clone(),
                    mappings,
                    via_source_key: transport_key.into(),
                    via_revision: transport_revision.into(),
                };
                additions.push(inherited.clone());
                added.insert(key, inherited);
            }
            Record::Transport {
                source_key,
                revision,
                path,
                ..
            } if source_key != transport_key => {
                let key = (source_key.clone(), revision.clone(), path.clone());
                if let Some(existing) = transports.get(&key) {
                    if *existing != record {
                        return Err(conflict(
                            "foreign source revision has differing exact portable metadata",
                        ));
                    }
                } else {
                    additions.push(record.clone());
                    transports.insert(key, record);
                }
            }
            _ => {}
        }
    }
    Ok(additions)
}
fn record_key(record: &Record) -> Option<&str> {
    match record {
        Record::Enrollment { source_key, .. }
        | Record::Label { source_key, .. }
        | Record::Revision { source_key, .. }
        | Record::InheritedRevision { source_key, .. }
        | Record::Transport { source_key, .. } => Some(source_key),
        _ => None,
    }
}
/// Same source bytes and identity set; each incoming target that maps into this
/// destination must agree with the destination's own target. Import-event
/// fields (time, base, predecessor, Git) are deliberately not compared.
fn compare(incoming: &Fact, existing: &Fact, map: &IdentityMap) -> Result<(), MergeError> {
    if !identity::same_source_fact(incoming, existing) {
        return Err(conflict(format!(
            "the same foreign source revision `{}` {} has differing source bytes or identities",
            incoming.source_key, incoming.fingerprint
        )));
    }
    let local = originals(existing);
    for mapping in incoming.mappings {
        let Some(target) = map.get(&mapping.target) else {
            continue;
        };
        if local
            .get(mapping.original.as_str())
            .is_some_and(|existing| existing.target != *target && mapping.layer != "external")
        {
            return Err(conflict(format!(
                "foreign origin `{}:{}` maps to `{target}` here but this destination holds it as `{}`",
                mapping.source_key,
                mapping.original,
                local[mapping.original.as_str()].target
            )));
        }
    }
    Ok(())
}

/// A rebuilt source fact used as the 3-way base of inherited entries.
struct FactBase {
    /// Markdown documents in the incoming namespace.
    markdown: Option<super::markdown::Inventory>,
    /// YAML records in the fact's own namespace.
    yaml: super::yaml::Inventory,
    /// Fact namespace -> this destination.
    destination: IdentityMap,
}
/// Incoming entries whose identity already exists in ours through another route.
#[derive(Default)]
pub(crate) struct Inherited {
    entries: BTreeMap<String, Option<(usize, String)>>,
    facts: Vec<FactBase>,
}
impl Inherited {
    pub(crate) fn contains(&self, local: &str) -> bool {
        self.entries.contains_key(local)
    }
    pub(crate) fn markdown(&self, local: &str) -> Option<&super::markdown::Inventory> {
        let (index, _) = self.entries.get(local)?.as_ref()?;
        self.facts[*index].markdown.as_ref()
    }
    pub(crate) fn yaml(
        &self,
        local: &str,
    ) -> Option<(&super::yaml::Inventory, &str, &IdentityMap)> {
        let (index, original) = self.entries.get(local)?.as_ref()?;
        let fact = &self.facts[*index];
        Some((&fact.yaml, original, &fact.destination))
    }
}
fn fact_map(fact: &Fact, entries: &[EntryIdentity]) -> IdentityMap {
    let mut map: IdentityMap = fact
        .mappings
        .iter()
        .map(|mapping| (mapping.original.clone(), mapping.target.clone()))
        .collect::<BTreeMap<_, _>>()
        .into();
    // A fact whose captured inventory lacks a mapping keeps exact tokens only.
    let _ = identity::reference_namespaces(entries, &mut map);
    map
}

/// Locate and rebuild the shared source fact for every incoming entry that is
/// absent from the effective base but already present in ours.
#[allow(clippy::too_many_arguments)]
pub(crate) fn inherited(
    origins: &Origins,
    ours: &Ledger,
    theirs: &Ledger,
    transport: &str,
    incoming: &[EntryIdentity],
    base_ids: &BTreeSet<String>,
    ours_ids: &BTreeSet<String>,
    map: &IdentityMap,
    root: &std::path::Path,
) -> Inherited {
    let mut result = Inherited::default();
    let held = facts(ours);
    let theirs_facts = facts(theirs);
    // Facts of the current source received earlier through another route.
    let mut received: BTreeMap<&str, Fact> = BTreeMap::new();
    for fact in ours
        .records
        .iter()
        .filter(|record| {
            matches!(record, Record::InheritedRevision { source_key, .. } if source_key == transport)
        })
        .filter_map(Record::fact)
    {
        // Later facts replace earlier ones: the latest containing fact is the base.
        for mapping in fact.mappings {
            received.insert(&mapping.original, fact);
        }
    }
    let mut built: BTreeMap<(String, String), Option<usize>> = BTreeMap::new();
    for entry in incoming {
        let local = &entry.address;
        if base_ids.contains(local)
            || !map
                .get(local)
                .is_some_and(|target| ours_ids.contains(target))
        {
            continue;
        }
        let (key, fingerprint, original, relocate) = if let Some(base) = origins.bases.get(local) {
            (
                base.key.clone(),
                base.fingerprint.clone(),
                base.original.clone(),
                true,
            )
        } else if let Some(fact) = received.get(local.as_str()) {
            (
                fact.source_key.to_owned(),
                fact.fingerprint.to_owned(),
                local.clone(),
                false,
            )
        } else {
            continue;
        };
        let index = *built
            .entry((key.clone(), fingerprint.clone()))
            .or_insert_with(|| {
                // Only facts this destination holds itself can be a base.
                let local_fact = held.get(&(key.as_str(), fingerprint.as_str()))?;
                let snapshot = super::captured(local_fact.files, root, ours, &key, &fingerprint);
                let view = super::live_inventory(&snapshot).ok()?;
                let mut destination = fact_map(local_fact, &view.entries);
                destination.local = map.local.clone();
                destination.references = map.references.clone();
                let markdown = if relocate {
                    theirs_facts
                        .get(&(key.as_str(), fingerprint.as_str()))
                        .and_then(|fact| {
                            super::markdown::relocated(
                                &view.markdown,
                                &fact_map(fact, &view.entries),
                            )
                            .ok()
                        })
                } else {
                    Some(view.markdown)
                };
                result.facts.push(FactBase {
                    markdown,
                    yaml: view.yaml,
                    destination,
                });
                Some(result.facts.len() - 1)
            });
        result
            .entries
            .insert(local.clone(), index.map(|index| (index, original)));
    }
    for local in origins.targets.keys() {
        result.entries.entry(local.clone()).or_insert(None);
    }
    result
}

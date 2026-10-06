//! Field origins authenticated by strict merge revisions and frozen source rows.
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use super::Source;
use super::collect::{Aliases, get, scalar};
use crate::merge::{ImportMapping, Record, captured_relocation_map, decode_ledger_bytes};
use crate::write::positions::{YamlDocument, YamlKind, YamlNode};

pub(super) fn read(
    raw: &[u8],
    sources: &[Source<'_>],
    aliases: &mut Aliases,
) -> Result<(), String> {
    let ledger = decode_ledger_bytes(raw).map_err(|error| error.message)?;
    let mut relocation_sources: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for record in &ledger.records {
        let Record::Revision {
            source_key,
            mappings,
            ..
        } = record
        else {
            continue;
        };
        let relocations = relocation_sources.entry(source_key.clone()).or_default();
        for mapping in mappings {
            if !aliases
                .redirects
                .get(source_key)
                .and_then(|originals| originals.get(&mapping.original))
                .is_some_and(|targets| targets.len() == 1 && targets.contains(&mapping.target))
            {
                return Err(format!(
                    "{}: mapping lacks its matching alias",
                    mapping.original
                ));
            }
            if let Some(previous) = relocations.get(&mapping.original) {
                if previous != &mapping.target {
                    return Err(format!(
                        "{}: conflicting captured relocation",
                        mapping.original
                    ));
                }
            } else {
                relocations.insert(mapping.original.clone(), mapping.target.clone());
            }
        }
    }
    aliases.relocation_maps = relocation_sources
        .into_iter()
        .map(|(key, source)| (key, captured_relocation_map(source)))
        .collect();

    let mut destinations = BTreeMap::new();
    let mut destination_reasoning = BTreeMap::new();
    for source in sources {
        let Ok(root) = &source.root else { continue };
        if destinations.insert(source.path, *root).is_some() {
            return Err(format!("{}: duplicate destination document", source.path));
        }
        if source.path == super::super::records::REASONING {
            destination_reasoning = reasoning_index(root)?;
        }
    }
    let mut captured_sessions = BTreeSet::new();
    let mut mutable_owners = BTreeSet::new();
    for record in ledger.records.iter().rev() {
        let Record::Revision {
            source_key,
            files,
            mappings,
            ..
        } = record
        else {
            continue;
        };
        let by_original: BTreeMap<&str, &ImportMapping> = mappings
            .iter()
            .map(|mapping| (mapping.original.as_str(), mapping))
            .collect();
        let mut parsed = BTreeMap::new();
        for (path, bytes) in files {
            if !super::is_history_path(path) || path == super::super::sessions::INDEX {
                continue;
            }
            let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
            let document = YamlDocument::parse(text).map_err(|error| error.message)?;
            parsed.insert(path.as_str(), document);
        }
        // Both inventories are built once; resolving a reasoning occurrence
        // never scans the frozen or destination rows.
        let frozen_reasoning = parsed
            .get(super::super::records::REASONING)
            .map(|document| reasoning_index(&document.root))
            .transpose()?
            .unwrap_or_default();
        let mut covered = BTreeSet::new();
        for (path, document) in &parsed {
            let root = &document.root;
            let (arrays, layer, target_path): (&[&str], &str, Cow<'_, str>) =
                if *path == super::super::records::REASONING {
                    (&["entries"], "reasoning", Cow::Borrowed(*path))
                } else {
                    let id = get(root, "session")
                        .and_then(|node| scalar(node, "id"))
                        .ok_or_else(|| format!("{path}: captured session identity missing"))?;
                    if super::session_path_id(path) != Some(id) {
                        return Err(format!("{path}: captured session identity disagrees"));
                    }
                    let owner = expected_mapping(&by_original, id, "session", path)?;
                    super::validate_session_id(&owner.target).map_err(|error| error.message)?;
                    let destination_path = format!("trace/sessions/{}.yaml", owner.target);
                    let destination = destinations
                        .get(destination_path.as_str())
                        .ok_or_else(|| format!("{destination_path}: destination source missing"))?;
                    if get(destination, "session").and_then(|node| scalar(node, "id"))
                        != Some(owner.target.as_str())
                    {
                        return Err(format!(
                            "{destination_path}: destination session identity disagrees"
                        ));
                    }
                    covered.insert(owner.original.as_str());
                    captured_sessions.insert(owner.target.as_str());
                    // Mutable owner fields use the newest capture for this
                    // source/session; older revisions still prove append-only rows.
                    if mutable_owners.insert((source_key.as_str(), owner.original.as_str())) {
                        for name in ["session", "open_threads", "ai_suggestions_pending"] {
                            if let Some(node) = get(root, name) {
                                capture(aliases, &destination_path, name, node, source_key);
                            }
                        }
                    }
                    (
                        &super::TURN_ARRAYS,
                        "session_occurrence",
                        Cow::Owned(destination_path),
                    )
                };
            for name in arrays {
                let Some(node) = get(root, name) else {
                    continue;
                };
                let YamlKind::Sequence(rows) = &node.kind else {
                    return Err(format!(
                        "{path}#{name}: captured occurrences are not a sequence"
                    ));
                };
                for (index, row) in rows.iter().enumerate() {
                    let original = if *name == "entries" {
                        scalar(row, "id")
                            .map(Cow::Borrowed)
                            .unwrap_or_else(|| Cow::Owned(format!("{path}#{name}/{index}")))
                    } else {
                        Cow::Owned(format!("{path}#{name}/{index}"))
                    };
                    let mapping = expected_mapping(&by_original, &original, layer, path)?;
                    let target_field = destination_field(
                        mapping,
                        &target_path,
                        name,
                        &destinations,
                        &destination_reasoning,
                    )?;
                    covered.insert(mapping.original.as_str());
                    capture(aliases, &target_path, &target_field, row, source_key);
                }
            }
        }
        for mapping in mappings {
            if mapping.path == super::super::sessions::INDEX
                || !matches!(
                    mapping.layer.as_str(),
                    "session" | "session_occurrence" | "reasoning"
                )
            {
                continue;
            }
            if !covered.contains(mapping.original.as_str()) {
                // Indexed lookup also rejects claimed occurrences that are not
                // actually present in their frozen document.
                let source = parsed.get(mapping.path.as_str()).and_then(|document| {
                    locate(&document.root, &mapping.original, &frozen_reasoning)
                });
                return Err(format!(
                    "{}: captured occurrence {}",
                    mapping.original,
                    if source.is_some() {
                        "was not authenticated"
                    } else {
                        "missing"
                    }
                ));
            }
        }
    }
    // An alias proves a redirect, not the capture of its owner or its rows.
    for session in aliases.imported.keys() {
        if !captured_sessions.contains(session.as_str()) {
            return Err(format!(
                "trace/sessions/{session}.yaml: imported session has no captured origin"
            ));
        }
    }
    Ok(())
}

fn expected_mapping<'a>(
    mappings: &BTreeMap<&str, &'a ImportMapping>,
    original: &str,
    layer: &str,
    path: &str,
) -> Result<&'a ImportMapping, String> {
    let mapping = mappings
        .get(original)
        .ok_or_else(|| format!("{original}: captured occurrence has no mapping"))?;
    if mapping.layer != layer || mapping.path != path {
        return Err(format!(
            "{original}: captured occurrence mapping has wrong layer or document"
        ));
    }
    Ok(mapping)
}

fn reasoning_index(root: &YamlNode) -> Result<BTreeMap<&str, usize>, String> {
    let mut index = BTreeMap::new();
    let Some(node) = get(root, "entries") else {
        return Ok(index);
    };
    let YamlKind::Sequence(rows) = &node.kind else {
        return Err("reasoning entries are not a sequence".to_owned());
    };
    for (ordinal, row) in rows.iter().enumerate() {
        if let Some(id) = scalar(row, "id")
            && index.insert(id, ordinal).is_some()
        {
            return Err(format!("{id}: duplicate reasoning occurrence"));
        }
    }
    Ok(index)
}

fn destination_field(
    mapping: &ImportMapping,
    expected_path: &str,
    expected_array: &str,
    destinations: &BTreeMap<&str, &YamlNode>,
    reasoning: &BTreeMap<&str, usize>,
) -> Result<String, String> {
    let destination = destinations
        .get(expected_path)
        .ok_or_else(|| format!("{expected_path}: destination source missing"))?;
    let index = if let Some((path, address)) = mapping.target.split_once('#') {
        let (name, ordinal) = address
            .rsplit_once('/')
            .ok_or_else(|| format!("{}: invalid occurrence address", mapping.target))?;
        if path != expected_path || name != expected_array {
            return Err(format!(
                "{}: occurrence destination has wrong document or array",
                mapping.target
            ));
        }
        ordinal
            .parse::<usize>()
            .map_err(|_| format!("{}: invalid occurrence ordinal", mapping.target))?
    } else if mapping.layer == "reasoning" {
        *reasoning
            .get(mapping.target.as_str())
            .ok_or_else(|| format!("{}: destination occurrence missing", mapping.target))?
    } else {
        return Err(format!(
            "{}: occurrence destination has no address",
            mapping.target
        ));
    };
    let Some(YamlKind::Sequence(rows)) = get(destination, expected_array).map(|node| &node.kind)
    else {
        return Err(format!(
            "{expected_path}: destination {expected_array} missing"
        ));
    };
    if rows.get(index).is_none() {
        return Err(format!(
            "{}: destination occurrence missing",
            mapping.target
        ));
    }
    Ok(format!("{expected_array}[{index}]"))
}

fn locate<'a>(
    root: &'a YamlNode,
    address: &str,
    reasoning: &BTreeMap<&str, usize>,
) -> Option<&'a YamlNode> {
    let (name, index) = if let Some((_, suffix)) = address.split_once('#') {
        let (name, index) = suffix.rsplit_once('/')?;
        (name, index.parse::<usize>().ok()?)
    } else {
        ("entries", *reasoning.get(address)?)
    };
    let YamlKind::Sequence(rows) = &get(root, name)?.kind else {
        return None;
    };
    rows.get(index)
}

fn capture(aliases: &mut Aliases, path: &str, prefix: &str, node: &YamlNode, key: &str) {
    aliases
        .imported_fields
        .entry(path.to_owned())
        .or_default()
        .insert(prefix.to_owned());
    match &node.kind {
        YamlKind::Scalar { value, .. } => {
            aliases
                .origins
                .entry(path.to_owned())
                .or_default()
                .entry(prefix.to_owned())
                .or_default()
                .insert((key.to_owned(), value.clone()));
        }
        YamlKind::Sequence(rows) => {
            for (index, row) in rows.iter().enumerate() {
                capture(aliases, path, &format!("{prefix}[{index}]"), row, key);
            }
        }
        YamlKind::Mapping(entries) => {
            for (name, value) in entries {
                if let Some(name) = name.scalar() {
                    capture(aliases, path, &format!("{prefix}.{name}"), value, key);
                }
            }
        }
        YamlKind::Alias(_) => {}
    }
}

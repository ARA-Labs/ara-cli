use super::types::*;
use crate::write::positions::YamlDocument;
use crate::write::{ArtifactSnapshot, WorkingArtifact};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const LOG: &str = "trace/merge_log.yaml";
pub(crate) const ALIASES: &str = "trace/aliases.yaml";
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn value_fingerprint(value: Option<&[u8]>) -> String {
    let mut h = Sha256::new();
    h.update(if value.is_some() {
        b"present\0".as_slice()
    } else {
        b"absent\0".as_slice()
    });
    if let Some(value) = value {
        h.update(value);
    }
    format!("{:x}", h.finalize())
}
pub fn fingerprint(snapshot: &ArtifactSnapshot) -> String {
    let mut h = Sha256::new();
    h.update(b"ara.artifact/v1\0");
    for (path, file) in &snapshot.files {
        if !file.existed || private_path(path) {
            continue;
        }
        h.update((path.len() as u64).to_be_bytes());
        h.update(path.as_bytes());
        h.update((file.bytes.len() as u64).to_be_bytes());
        h.update(&file.bytes);
    }
    format!("{:x}", h.finalize())
}
pub(crate) use crate::write::source::private_path;
pub(crate) fn bytes<'a>(snapshot: &'a ArtifactSnapshot, path: &str) -> Option<&'a [u8]> {
    snapshot
        .files
        .get(path)
        .filter(|file| file.existed)
        .map(|file| file.bytes.as_slice())
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Alias {
    pub source_key: String,
    pub label: String,
    pub original: String,
    pub target: String,
    pub revision: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Record {
    Enrollment {
        source_key: String,
        label: String,
        time: String,
    },
    Label {
        source_key: String,
        label: String,
        time: String,
    },
    Revision {
        source_key: String,
        fingerprint: String,
        base: String,
        predecessor: Option<String>,
        time: String,
        git: Option<GitMergeProvenance>,
        #[serde(
            serialize_with = "serialize_file_payloads",
            deserialize_with = "unique_file_payloads"
        )]
        files: BTreeMap<String, Vec<u8>>,
        mappings: Vec<ImportMapping>,
    },
    Conflict {
        conflict: MergeConflict,
    },
    Resolution {
        conflict_id: String,
        take: String,
        time: String,
        session: String,
        turn: u64,
        signal: String,
        provenance: String,
        prior_fingerprint: String,
        selected_fingerprint: String,
        applied_fingerprint: String,
    },
    ProtectedDecision {
        conflict: MergeConflict,
        decision: String,
        reason: String,
        expected_current: String,
        session: String,
        turn: u64,
        signal: String,
        provenance: String,
    },
    Transport {
        source_key: String,
        revision: String,
        path: String,
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
    },
    ImportedResolution {
        source_key: String,
        revision: String,
        conflict_id: String,
        #[serde(with = "base64_bytes")]
        evidence: Vec<u8>,
    },
}
mod base64_bytes {
    use base64::{Engine as _, display::Base64Display, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Visitor};

    pub(super) fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&Base64Display::new(bytes, &STANDARD))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<u8>, D::Error> {
        struct Bytes;
        impl Visitor<'_> for Bytes {
            type Value = Vec<u8>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a canonical padded RFC 4648 standard base64 string")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                // STANDARD requires canonical padding and rejects unused trailing
                // bits, whitespace, and characters outside the standard alphabet.
                STANDARD.decode(value).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(Bytes)
    }

    pub(super) struct Encoded<'a>(pub &'a [u8]);
    impl Serialize for Encoded<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serialize(self.0, serializer)
        }
    }

    pub(super) struct Decoded(pub Vec<u8>);
    impl<'de> Deserialize<'de> for Decoded {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserialize(deserializer).map(Self)
        }
    }
}

fn serialize_file_payloads<S: serde::Serializer>(
    files: &BTreeMap<String, Vec<u8>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(files.len()))?;
    for (path, bytes) in files {
        map.serialize_entry(path, &base64_bytes::Encoded(bytes))?;
    }
    map.end()
}

fn unique_file_payloads<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Vec<u8>>, D::Error> {
    struct Files;
    impl<'de> serde::de::Visitor<'de> for Files {
        type Value = BTreeMap<String, Vec<u8>>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a source payload mapping with unique paths and base64 values")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut files = BTreeMap::new();
            while let Some(path) = map.next_key::<String>()? {
                if files.contains_key(&path) {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate source payload path {path}"
                    )));
                }
                files.insert(path, map.next_value::<base64_bytes::Decoded>()?.0);
            }
            Ok(files)
        }
    }
    deserializer.deserialize_map(Files)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Ledger {
    pub records: Vec<Record>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceHistory {
    pub source_key: String,
    pub label: String,
    pub fingerprint: String,
    pub base: String,
    pub git: Option<GitMergeProvenance>,
}
impl Ledger {
    pub(crate) fn history(&self, key: &str) -> Option<SourceHistory> {
        let mut label = None;
        let mut latest = None;
        for record in &self.records {
            match record {
                Record::Enrollment {
                    source_key,
                    label: name,
                    ..
                }
                | Record::Label {
                    source_key,
                    label: name,
                    ..
                } if source_key == key => label = Some(name.clone()),
                Record::Revision {
                    source_key,
                    fingerprint,
                    base,
                    git,
                    ..
                } if source_key == key => {
                    latest = Some((fingerprint.clone(), base.clone(), git.clone()))
                }
                _ => {}
            }
        }
        let (fingerprint, base, git) = latest?;
        Some(SourceHistory {
            source_key: key.into(),
            label: label.unwrap_or_else(|| key.into()),
            fingerprint,
            base,
            git,
        })
    }
    pub(crate) fn revision(&self, key: &str) -> Option<&Record> {
        self.records
            .iter()
            .rev()
            .find(|r| matches!(r, Record::Revision { source_key, .. } if source_key == key))
    }
    pub(crate) fn unresolved(&self) -> Vec<MergeConflict> {
        let resolved: BTreeSet<&str> = self
            .records
            .iter()
            .filter_map(|r| match r {
                Record::Resolution { conflict_id, .. }
                | Record::ImportedResolution { conflict_id, .. } => Some(conflict_id.as_str()),
                _ => None,
            })
            .collect();
        let mut conflicts = BTreeMap::new();
        for record in &self.records {
            if let Record::Conflict { conflict } = record
                && !resolved.contains(conflict.id.as_str())
            {
                conflicts
                    .entry(conflict.id.clone())
                    .or_insert_with(|| conflict.clone());
            }
        }
        conflicts.into_values().collect()
    }
    pub(crate) fn validate(&self) -> Result<(), MergeError> {
        let mut labels = BTreeMap::new();
        let mut enrolled = BTreeSet::new();
        let mut revisions: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        let mut last_revision = BTreeMap::new();
        let mut conflicts = BTreeMap::new();
        let mut resolved = BTreeSet::new();
        let mut transports = BTreeMap::new();
        for record in &self.records {
            match record {
                Record::Enrollment {
                    source_key, label, ..
                } => {
                    validate_name(source_key, false)?;
                    validate_name(label, true)?;
                    if !enrolled.insert(source_key.as_str()) {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "duplicate source enrollment",
                        ));
                    }
                    bind_label(&mut labels, label, source_key)?;
                }
                Record::Label {
                    source_key, label, ..
                } => {
                    if !enrolled.contains(source_key.as_str()) {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "label without source enrollment",
                        ));
                    }
                    validate_name(label, true)?;
                    bind_label(&mut labels, label, source_key)?;
                }
                Record::Revision {
                    source_key,
                    fingerprint,
                    predecessor,
                    files,
                    mappings,
                    ..
                } => {
                    if !enrolled.contains(source_key.as_str()) {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "revision without source enrollment",
                        ));
                    }
                    if let Some(previous) = last_revision.get(source_key.as_str()) {
                        if predecessor.as_deref() != Some(*previous) {
                            return Err(MergeError::content(
                                "merge.corrupt_ledger",
                                "broken source revision predecessor",
                            ));
                        }
                    } else if predecessor.is_some() {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "first source revision has predecessor",
                        ));
                    }
                    if !revisions.entry(source_key).or_default().insert(fingerprint) {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "duplicate revision",
                        ));
                    }
                    last_revision.insert(source_key.as_str(), fingerprint.as_str());
                    let mut origins = BTreeSet::new();
                    for mapping in mappings {
                        if mapping.source_key != *source_key || !origins.insert(&mapping.original) {
                            return Err(MergeError::content(
                                "merge.corrupt_ledger",
                                "invalid or duplicate import mapping",
                            ));
                        }
                    }
                    for path in files.keys() {
                        safe_path(path)?;
                        if private_path(path) || path == LOG || path == ALIASES {
                            return Err(MergeError::content(
                                "merge.corrupt_ledger",
                                "revision contains private or recursively nested provenance",
                            ));
                        }
                    }
                }
                Record::Conflict { conflict } => {
                    if let Some(previous) = conflicts.insert(conflict.id.as_str(), conflict)
                        && previous != conflict
                    {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "conflict ID has inconsistent evidence",
                        ));
                    }
                }
                Record::Resolution {
                    conflict_id,
                    take,
                    prior_fingerprint,
                    selected_fingerprint,
                    ..
                } => {
                    let Some(conflict) = conflicts.get(conflict_id.as_str()) else {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "resolution has no prior conflict",
                        ));
                    };
                    if !resolved.insert(conflict_id)
                        || !conflict.allowed.contains(take)
                        || *prior_fingerprint != conflict.ours.fingerprint
                    {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "invalid conflict resolution evidence",
                        ));
                    }
                    let selected = match take.as_str() {
                        "ours" => &conflict.ours,
                        "theirs" => &conflict.theirs,
                        "base" => &conflict.base,
                        _ => {
                            return Err(MergeError::content(
                                "merge.corrupt_ledger",
                                "invalid resolution choice",
                            ));
                        }
                    };
                    if *selected_fingerprint != selected.fingerprint {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "resolution selected fingerprint mismatch",
                        ));
                    }
                }
                Record::Transport {
                    source_key,
                    revision,
                    path,
                    bytes,
                } => {
                    if !enrolled.contains(source_key.as_str()) || (path != LOG && path != ALIASES) {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "invalid transport provenance source or path",
                        ));
                    }
                    if transports
                        .insert(
                            (source_key.as_str(), revision.as_str(), path.as_str()),
                            bytes,
                        )
                        .is_some_and(|old| old != bytes)
                    {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "source revision has inconsistent exact portable metadata",
                        ));
                    }
                }
                Record::ProtectedDecision {
                    conflict,
                    decision,
                    reason,
                    expected_current,
                    ..
                } => {
                    if !conflict.kind.starts_with("protected")
                        || !matches!(decision.as_str(), "reject_incoming" | "restore_base")
                        || reason.trim().is_empty()
                        || expected_current != &conflict.ours.fingerprint
                    {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "invalid protected decision",
                        ));
                    }
                }
                Record::ImportedResolution {
                    conflict_id,
                    evidence,
                    ..
                } => {
                    let Some(conflict) = conflicts.get(conflict_id.as_str()) else {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "imported resolution has no prior conflict",
                        ));
                    };
                    let original: Record = serde_json::from_slice(evidence).map_err(|error| {
                        MergeError::content("merge.corrupt_ledger", error.to_string())
                    })?;
                    let Record::Resolution {
                        conflict_id: origin,
                        take,
                        prior_fingerprint,
                        selected_fingerprint,
                        ..
                    } = original
                    else {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "imported resolution evidence is not a resolution",
                        ));
                    };
                    let selected = match take.as_str() {
                        "ours" => &conflict.ours,
                        "theirs" => &conflict.theirs,
                        "base" => &conflict.base,
                        _ => {
                            return Err(MergeError::content(
                                "merge.corrupt_ledger",
                                "invalid imported resolution choice",
                            ));
                        }
                    };
                    if conflict.kind != "imported_unresolved"
                        || origin != *conflict_id
                        || prior_fingerprint != conflict.ours.fingerprint
                        || selected_fingerprint != selected.fingerprint
                        || !resolved.insert(conflict_id)
                    {
                        return Err(MergeError::content(
                            "merge.corrupt_ledger",
                            "imported resolution evidence does not match original conflict",
                        ));
                    }
                }
            }
        }
        for ((source, revision, _), _) in transports {
            if !revisions
                .get(source)
                .is_some_and(|known| known.contains(revision))
            {
                return Err(MergeError::content(
                    "merge.corrupt_ledger",
                    "portable metadata has no associated source revision",
                ));
            }
        }
        Ok(())
    }
}
fn bind_label<'a>(
    labels: &mut BTreeMap<&'a str, &'a str>,
    label: &'a str,
    key: &'a str,
) -> Result<(), MergeError> {
    if labels
        .insert(label, key)
        .is_some_and(|previous| previous != key)
    {
        return Err(MergeError::content(
            "merge.ambiguous_label",
            format!("display label `{label}` is bound to different source keys"),
        ));
    }
    Ok(())
}
pub(crate) fn validate_name(name: &str, label: bool) -> Result<(), MergeError> {
    if name.is_empty()
        || name.len() > 128
        || name
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, ':' | '/' | '\\'))
    {
        return Err(MergeError::content(
            if label {
                "merge.invalid_label"
            } else {
                "merge.invalid_source_key"
            },
            "source keys and labels must be nonempty unqualified portable tokens",
        ));
    }
    Ok(())
}
pub(crate) fn safe_path(path: &str) -> Result<(), MergeError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(MergeError::content(
            "merge.unsafe_path",
            format!("unsafe portable path `{path}`"),
        ));
    }
    Ok(())
}
fn json_yaml(
    snapshot: &ArtifactSnapshot,
    path: &str,
) -> Result<Option<serde_json::Value>, MergeError> {
    let Some(raw) = bytes(snapshot, path) else {
        return Ok(None);
    };
    let text = std::str::from_utf8(raw)
        .map_err(|e| MergeError::content("merge.corrupt_ledger", e.to_string()))?;
    Ok(Some(YamlDocument::parse(text)?.root.to_json()?))
}
pub(crate) fn load(snapshot: &ArtifactSnapshot) -> Result<Ledger, MergeError> {
    let Some(raw) = bytes(snapshot, LOG) else {
        return Ok(Ledger::default());
    };
    load_bytes(raw)
}
pub(crate) fn load_bytes(raw: &[u8]) -> Result<Ledger, MergeError> {
    if raw.len() > crate::flat_yaml::MAX_INPUT_BYTES {
        return Err(MergeError::content(
            "merge.corrupt_ledger",
            "portable ledger exceeds the source byte budget",
        ));
    }
    let text = std::str::from_utf8(raw)
        .map_err(|cause| MergeError::content("merge.corrupt_ledger", cause.to_string()))?;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Stored {
        format: String,
        records: Vec<Record>,
    }
    // The native renderer writes compact JSON records in this YAML envelope.
    // Decode canonical base64 payloads through the same typed schema without
    // building another lossless source tree. Other YAML layouts stay strict.
    let native = text
        .strip_prefix("format: ara.merge-log/v1\nrecords:\n")
        .and_then(|tail| {
            let rows = tail
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect::<Vec<_>>();
            (!rows.is_empty() && rows.iter().all(|line| line.starts_with("  - {\"")))
                .then_some(rows)
        });
    let stored: Stored = if let Some(rows) = native {
        let records = rows
            .into_iter()
            .map(|line| {
                serde_json::from_str::<Record>(&line[4..])
                    .map_err(|error| MergeError::content("merge.corrupt_ledger", error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Stored {
            format: "ara.merge-log/v1".into(),
            records,
        }
    } else if let Ok(stored) = serde_json::from_str::<Stored>(text) {
        stored
    } else {
        let value = YamlDocument::parse(text)?.root.to_json()?;
        serde_json::from_value(value)
            .map_err(|error| MergeError::content("merge.corrupt_ledger", error.to_string()))?
    };
    if stored.format != "ara.merge-log/v1" {
        return Err(MergeError::content(
            "merge.corrupt_ledger",
            "unsupported portable merge ledger format",
        ));
    }
    let ledger = Ledger {
        records: stored.records,
    };
    ledger.validate()?;
    Ok(ledger)
}
pub fn source_history(
    snapshot: &ArtifactSnapshot,
    key: &str,
) -> Result<Option<SourceHistory>, MergeError> {
    Ok(load(snapshot)?.history(key))
}
pub(crate) fn metadata_append_only(
    base: &ArtifactSnapshot,
    current: &ArtifactSnapshot,
) -> Result<Option<&'static str>, MergeError> {
    for (path, key) in [(LOG, "records"), (ALIASES, "aliases")] {
        let Some(old) = bytes(base, path) else {
            continue;
        };
        let Some(new) = bytes(current, path) else {
            return Ok(Some(path));
        };
        if old == new {
            continue;
        }
        let old = std::str::from_utf8(old).map_err(|error| {
            MergeError::content("merge.corrupt_ledger", error.to_string()).at(path)
        })?;
        let new = std::str::from_utf8(new).map_err(|error| {
            MergeError::content("merge.corrupt_ledger", error.to_string()).at(path)
        })?;
        let old_document = YamlDocument::parse(old)?;
        let new_document = YamlDocument::parse(new)?;
        let old_list = old_document.root.get(key)?.ok_or_else(|| {
            MergeError::content(
                "merge.corrupt_ledger",
                "missing protected metadata sequence",
            )
            .at(path)
        })?;
        let new_list = new_document.root.get(key)?.ok_or_else(|| {
            MergeError::content(
                "merge.corrupt_ledger",
                "missing protected metadata sequence",
            )
            .at(path)
        })?;
        let old_items = old_list.sequence()?;
        let new_items = new_list.sequence()?;
        if old_items.len() > new_items.len() {
            return Ok(Some(path));
        }
        if old_items.is_empty() {
            let old_prefix = old[..old_list.start].trim_end_matches([' ', '\r', '\n']);
            let new_prefix = new[..new_list.start].trim_end_matches([' ', '\r', '\n']);
            if old_prefix != new_prefix || !new.ends_with(&old[old_list.end..]) {
                return Ok(Some(path));
            }
            continue;
        }
        if old[..old_list.start] != new[..new_list.start] {
            return Ok(Some(path));
        }
        for (index, item) in old_items.iter().enumerate() {
            let next = &new_items[index];
            let old_range = if old_list.flow {
                item.start..item.end
            } else {
                crate::write::positions::line_start(old, item.start)
                    ..old_items.get(index + 1).map_or(old.len(), |next| {
                        crate::write::positions::line_start(old, next.start)
                    })
            };
            let new_range = if new_list.flow {
                next.start..next.end
            } else {
                crate::write::positions::line_start(new, next.start)
                    ..new_items.get(index + 1).map_or(new.len(), |next| {
                        crate::write::positions::line_start(new, next.start)
                    })
            };
            if old[old_range] != new[new_range] {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}
pub(crate) fn aliases(snapshot: &ArtifactSnapshot) -> Result<Vec<Alias>, MergeError> {
    let Some(raw) = bytes(snapshot, ALIASES) else {
        return Ok(Vec::new());
    };
    aliases_bytes(raw)
}
pub(crate) fn aliases_bytes(raw: &[u8]) -> Result<Vec<Alias>, MergeError> {
    let text = std::str::from_utf8(raw)
        .map_err(|cause| MergeError::content("merge.alias_data", cause.to_string()))?;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Stored {
        format: String,
        aliases: Vec<Alias>,
    }
    let native = text
        .strip_prefix("format: ara.aliases/v1\naliases:\n")
        .filter(|tail| {
            !tail.trim().is_empty()
                && tail
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .all(|line| line.starts_with("  - {\""))
        });
    let stored: Stored = if let Some(tail) = native {
        Stored {
            format: "ara.aliases/v1".into(),
            aliases: tail
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    serde_json::from_str::<Alias>(&line[4..])
                        .map_err(|error| MergeError::content("merge.alias_data", error.to_string()))
                })
                .collect::<Result<_, _>>()?,
        }
    } else {
        let value = YamlDocument::parse(text)?.root.to_json()?;
        serde_json::from_value(value)
            .map_err(|error| MergeError::content("merge.alias_data", error.to_string()))?
    };
    if stored.format != "ara.aliases/v1" {
        return Err(MergeError::content(
            "merge.alias_data",
            "unsupported portable aliases format",
        ));
    }
    Ok(stored.aliases)
}
pub(crate) fn validate_alias_names(records: &[Alias]) -> Result<(), MergeError> {
    let mut labels = BTreeMap::new();
    for record in records {
        validate_name(&record.source_key, false)?;
        validate_name(&record.label, true)?;
        bind_label(&mut labels, &record.label, &record.source_key)?;
    }
    Ok(())
}
pub(crate) fn alias_index(
    records: &[Alias],
    identities: &BTreeSet<String>,
    redirects: &BTreeMap<String, String>,
    markdown: &super::markdown::Inventory,
) -> Result<BTreeMap<String, String>, MergeError> {
    let mut labels = BTreeMap::new();
    let mut edges = redirects.clone();
    let mut alternatives = Vec::new();
    let suffixes = heading_suffixes(markdown);
    let scopes: BTreeSet<&str> = records
        .iter()
        .flat_map(|record| [record.source_key.as_str(), record.label.as_str()])
        .collect();
    for record in records {
        validate_name(&record.source_key, false)?;
        validate_name(&record.label, true)?;
        bind_label(&mut labels, &record.label, &record.source_key)?;
        for prefix in [&record.source_key, &record.label] {
            if prefix == "trace" || (identities.contains(prefix) && prefix.ends_with(".md")) {
                return Err(MergeError::content(
                    "merge.ambiguous_label",
                    "source scope collides with a native reference namespace",
                ));
            }
            let address = format!("{prefix}:{}", record.original);
            let target = if record
                .target
                .split_once(':')
                .is_some_and(|(scope, _)| scopes.contains(scope))
            {
                record.target.clone()
            } else {
                normalize_local(&record.target)
            };
            if let Some(old) = edges.get(&address) {
                if old != &target {
                    alternatives.push((address, target));
                }
            } else {
                edges.insert(address, target);
            }
        }
    }
    let mut displayed = BTreeMap::<String, Option<(String, String)>>::new();
    for record in records {
        let display = super::markdown::display_address(&record.original);
        for scope in [&record.source_key, &record.label] {
            let address = format!("{scope}:{display}");
            let exact = format!("{scope}:{}", record.original);
            displayed
                .entry(address)
                .and_modify(|found| {
                    if found
                        .as_ref()
                        .is_some_and(|(previous, _)| previous != &exact)
                    {
                        *found = None;
                    }
                })
                .or_insert(Some((exact, record.target.clone())));
        }
    }
    for (address, choice) in displayed {
        if let Some((_, target)) = choice {
            edges.entry(address).or_insert(target);
        } else {
            // A flattened scoped locator is unavailable; typed keys remain.
            edges.remove(&address);
        }
    }
    let mut remapped = Vec::new();
    for (origin, target) in &edges {
        if !edges.contains_key(target) {
            let canonical = canonical_heading(target, identities, &suffixes)?;
            if canonical != target {
                remapped.push((origin.clone(), canonical.to_owned()));
            }
        }
    }
    for (origin, target) in remapped {
        edges.insert(origin, target);
    }
    let mut resolved: BTreeMap<String, String> = BTreeMap::new();
    for origin in edges.keys() {
        if resolved.contains_key(origin) {
            continue;
        }
        let mut current = origin.clone();
        let mut chain = Vec::new();
        let mut seen = BTreeSet::new();
        let target = loop {
            if let Some(target) = resolved.get(&current) {
                break target.clone();
            }
            if !seen.insert(current.clone()) {
                return Err(MergeError::content(
                    "merge.alias_cycle",
                    format!("alias cycle at `{current}`"),
                ));
            }
            if let Some(next) = edges.get(&current) {
                chain.push(current);
                current = next.clone();
            } else if identities.contains(&current) {
                break current;
            } else {
                return Err(MergeError::content(
                    "merge.alias_dangling",
                    format!("alias `{origin}` targets missing `{current}`"),
                ));
            }
        };
        for address in chain {
            resolved.insert(address, target.clone());
        }
    }
    for (origin, target) in alternatives {
        let target = canonical_heading(&target, identities, &suffixes)?;
        let terminal = resolved.get(target).map_or(target, String::as_str);
        if resolved.get(&origin).map(String::as_str) != Some(terminal) {
            return Err(MergeError::content(
                "merge.alias_conflict",
                format!("conflicting alias `{origin}`"),
            ));
        }
    }
    Ok(resolved)
}
pub(crate) fn heading_suffixes(
    markdown: &super::markdown::Inventory,
) -> BTreeMap<String, Option<String>> {
    let mut result = BTreeMap::new();
    for entry in &markdown.entries {
        if entry.heading.is_empty() || entry.numeric.is_some() {
            continue;
        }
        for start in 0..entry.heading.len() {
            let components: Vec<&str> = entry.heading[start..]
                .iter()
                .map(|(literal, numeric)| numeric.as_deref().unwrap_or(literal))
                .collect();
            let short = format!("{}#{}", entry.path, components.join("/"));
            match result.entry(short) {
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(Some(entry.address.clone()));
                }
                std::collections::btree_map::Entry::Occupied(mut slot) => {
                    if slot.get().as_ref() != Some(&entry.address) {
                        slot.insert(None);
                    }
                }
            }
        }
    }
    result
}
fn canonical_heading<'a>(
    address: &'a str,
    identities: &BTreeSet<String>,
    suffixes: &'a BTreeMap<String, Option<String>>,
) -> Result<&'a str, MergeError> {
    if identities.contains(address) {
        return Ok(address);
    }
    match suffixes.get(address) {
        Some(Some(target)) => Ok(target),
        Some(None) => Err(MergeError::content(
            "merge.redirect_ambiguous",
            format!("native heading `{address}` is ambiguous"),
        )),
        None => Ok(address),
    }
}
pub(crate) fn reject_ambiguous_display(
    address: &str,
    markdown: &super::markdown::Inventory,
    rows: &[serde_json::Value],
    aliases: &[Alias],
) -> Result<(), MergeError> {
    let normalized = normalize_local(address);
    let mut shapes = BTreeSet::new();
    if let Some((scope, original)) = address.split_once(':').filter(|(scope, _)| {
        aliases
            .iter()
            .any(|alias| alias.source_key == *scope || alias.label == *scope)
    }) {
        for alias in aliases
            .iter()
            .filter(|alias| alias.source_key == scope || alias.label == scope)
        {
            if super::markdown::display_address(&alias.original) == original {
                shapes.insert(alias.original.clone());
            }
        }
    } else if let Some((path, name)) = normalized.split_once('#') {
        for entry in markdown.entries.iter().filter(|entry| entry.path == path) {
            for start in 0..entry.heading.len() {
                let components: Vec<&str> = entry.heading[start..]
                    .iter()
                    .map(|(literal, numeric)| numeric.as_deref().unwrap_or(literal))
                    .collect();
                if components.join("/") == name {
                    shapes.insert(entry.address.clone());
                }
            }
        }
        for row in rows {
            let Some(value) = row.get("from_selector") else {
                continue;
            };
            let selector = crate::write::EntrySelector::deserialize(value)
                .map_err(|error| MergeError::content("merge.redirect_data", error.to_string()))?;
            if let crate::write::EntrySelector::Document {
                document,
                heading,
                entry: None,
            } = &selector
                && document == path
            {
                for start in 0..heading.len() {
                    if heading[start..].join("/") == name
                        && let Some(key) = archived_selector_key(&selector, rows)
                    {
                        shapes.insert(key);
                    }
                }
            }
        }
    }
    if shapes.len() > 1 {
        Err(MergeError::content(
            "merge.redirect_ambiguous",
            format!(
                "native display locator `{address}` matches different literal heading vectors; use a document selector"
            ),
        ))
    } else {
        Ok(())
    }
}
pub(crate) fn native_numeric(document: &str, id: &str) -> bool {
    match numeric_prefix(id) {
        Some('N') => matches!(document, "trace" | "trace/exploration_tree.yaml"),
        Some('C') => document == "logic/claims.md",
        Some('H') => document == "logic/solution/heuristics.md",
        Some('E') => document == "logic/experiments.md",
        Some('O') => document == "staging/observations.yaml",
        Some('T') => matches!(document, "trace/taste.yaml" | "trace/taste_log.yaml"),
        _ => false,
    }
}
pub(crate) fn normalize_local(address: &str) -> String {
    if let Some((document, entry)) = address.split_once('#')
        && !document.contains(':')
        && (document.ends_with(".md")
            || document.starts_with("trace/")
            || document.starts_with("staging/"))
    {
        return if native_numeric(document, entry) {
            entry.into()
        } else {
            address.into()
        };
    }
    if let Some((document, entry)) = address.split_once(':') {
        let external = document.starts_with("src/") || document.starts_with("evidence/");
        if !external
            && (document == "trace"
                || document.starts_with("logic/")
                || document == "PAPER.md"
                || document.starts_with("rubric/")
                || document.ends_with(".md") && !document.contains("://"))
        {
            if native_numeric(document, entry) {
                return entry.into();
            }
            return format!("{document}#{entry}");
        }
    }
    if let Some((document, entry)) = address.rsplit_once('#')
        && native_numeric(document, entry)
    {
        return entry.into();
    }
    address.into()
}
pub(crate) fn source_qualified(records: &[Alias], address: &str) -> bool {
    address.split_once(':').is_some_and(|(scope, _)| {
        records
            .iter()
            .any(|record| record.source_key == scope || record.label == scope)
    })
}
pub(crate) fn mutation_rows(
    snapshot: &ArtifactSnapshot,
) -> Result<Vec<serde_json::Value>, MergeError> {
    let Some(mut value) = json_yaml(snapshot, "trace/logic_mutations.yaml")? else {
        return Ok(Vec::new());
    };
    match value.get_mut("mutations").map(serde_json::Value::take) {
        Some(serde_json::Value::Array(rows)) => Ok(rows),
        _ => Err(MergeError::content(
            "merge.redirect_data",
            "logic mutation ledger requires mutations sequence",
        )
        .at("trace/logic_mutations.yaml")),
    }
}
/// A collision is a property of the complete archived vector inventory, not
/// a malformed row. Keep both keys exact while retaining ordinary ledger keys.
pub(crate) fn archived_selector_key(
    selector: &crate::write::EntrySelector,
    rows: &[serde_json::Value],
) -> Option<String> {
    let key = super::markdown::selector_key(selector)?;
    let display = super::markdown::display_address(&key);
    let mut shapes = BTreeSet::new();
    for row in rows {
        let Some(value) = row.get("from_selector") else {
            continue;
        };
        let Ok(from) = crate::write::EntrySelector::deserialize(value) else {
            continue;
        };
        if super::markdown::selector_key(&from)
            .is_some_and(|key| super::markdown::display_address(&key) == display)
        {
            shapes.insert(super::markdown::exact_selector_key(&from));
        }
    }
    if shapes.len() > 1 && numeric_prefix(&key).is_none() {
        super::markdown::exact_selector_key(selector)
    } else {
        Some(key)
    }
}
pub(crate) fn local_redirects(
    snapshot: &ArtifactSnapshot,
    identities: &BTreeSet<String>,
    markdown: &super::markdown::Inventory,
    rows: &[serde_json::Value],
) -> Result<BTreeMap<String, String>, MergeError> {
    fn error(message: &str) -> MergeError {
        MergeError::content("merge.redirect_data", message).at("trace/logic_mutations.yaml")
    }
    fn literal(selector: &crate::write::EntrySelector) -> Option<String> {
        match selector {
            crate::write::EntrySelector::Id { id } => Some(id.clone()),
            crate::write::EntrySelector::Document {
                document,
                heading,
                entry,
            } => {
                let mut path = heading.join("/");
                if let Some(entry) = entry {
                    if !path.is_empty() {
                        path.push('/');
                    }
                    path.push_str(entry);
                }
                (!path.is_empty()).then(|| format!("{document}#{path}"))
            }
        }
    }
    fn underlying(selector: &crate::write::EntrySelector) -> Option<&str> {
        match selector {
            crate::write::EntrySelector::Id { id } => Some(id),
            crate::write::EntrySelector::Document { heading, entry, .. } => {
                entry.as_deref().or_else(|| {
                    heading.last().map(|heading| {
                        heading
                            .split_once(':')
                            .map_or(heading.as_str(), |(id, _)| id)
                            .trim()
                    })
                })
            }
        }
    }
    fn insert(
        edges: &mut BTreeMap<String, String>,
        origin: String,
        target: &str,
    ) -> Result<(), MergeError> {
        if edges
            .insert(origin.clone(), target.into())
            .is_some_and(|old| old != target)
        {
            return Err(MergeError::content(
                "merge.redirect_ambiguous",
                format!("different redirect targets for `{origin}`"),
            )
            .at("trace/logic_mutations.yaml"));
        }
        Ok(())
    }
    let registered: BTreeSet<String> = bytes(snapshot, "PAPER.md")
        .map(std::str::from_utf8)
        .transpose()
        .map_err(|cause| error(&cause.to_string()))?
        .map(crate::knowledge_paths)
        .transpose()
        .map_err(|cause| error(&cause))?
        .unwrap_or_default()
        .into_iter()
        .collect();
    let mut edges = BTreeMap::new();
    let mut suffixes: BTreeMap<String, Option<(String, String)>> = BTreeMap::new();
    let mut retired = BTreeSet::new();
    for row in rows {
        let from = row
            .get("from")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| error("mutation origin must be a native address"))?;
        let document = from
            .split_once(':')
            .map(|(document, _)| document)
            .ok_or_else(|| error("mutation origin must include its mutable document"))?;
        let archived = row
            .get("from_selector")
            .is_some_and(serde_json::Value::is_object)
            && ["before", "after", "signal", "provenance", "session", "turn"]
                .iter()
                .all(|key| row.get(*key).is_some());
        let safe_archived = archived
            && document.ends_with(".md")
            && safe_path(document).is_ok()
            && !private_path(document)
            && !["trace/", "staging/", "src/", "evidence/"]
                .iter()
                .any(|prefix| document.starts_with(prefix));
        if !crate::write::source::allowed_document(document)
            && !registered.contains(document)
            && !safe_archived
        {
            return Err(error(
                "local redirects cannot authorize immutable trace/staging/source/evidence or unaudited unregistered mutations",
            ));
        }
        let display_origin = normalize_local(from);
        let mut origin = display_origin.clone();
        let from_selector = row
            .get("from_selector")
            .map(|value| {
                crate::write::EntrySelector::deserialize(value)
                    .map_err(|cause| error(&cause.to_string()))
            })
            .transpose()?;
        if let Some(selector) = &from_selector {
            if let crate::write::EntrySelector::Document {
                document: scope, ..
            } = selector
                && scope != document
            {
                return Err(error(
                    "selector document differs from native mutation address",
                ));
            }
            if literal(selector).as_deref() != Some(display_origin.as_str())
                && underlying(selector) != Some(display_origin.as_str())
                && normalize_local(&literal(selector).unwrap_or_default()) != display_origin
            {
                return Err(error("native origin and literal selector disagree"));
            }
            origin = archived_selector_key(selector, rows).unwrap_or(display_origin);
        }
        let to = row
            .get("to")
            .ok_or_else(|| error("mutation destination presence is required"))?;
        if to.is_null() {
            if row.get("to_selector").is_some_and(|value| !value.is_null()) {
                return Err(error("removed entry has a nonnull destination selector"));
            }
            if row
                .get("historical_references")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|values| !values.is_empty())
            {
                return Err(MergeError::content(
                    "merge.redirect_dangling",
                    "removed identity retains historical references without a redirect",
                )
                .at("trace/logic_mutations.yaml"));
            }
            continue;
        }
        let native = normalize_local(
            to.as_str()
                .ok_or_else(|| error("mutation target must be string or null"))?,
        );
        let to_selector = row
            .get("to_selector")
            .map(|value| {
                crate::write::EntrySelector::deserialize(value)
                    .map_err(|cause| error(&cause.to_string()))
            })
            .transpose()?;
        if from_selector.is_some() != to_selector.is_some() {
            return Err(error(
                "authoritative mutation selectors must be supplied together",
            ));
        }
        let target = if let Some(selector) = &to_selector {
            if literal(selector).as_deref() != Some(native.as_str())
                && underlying(selector) != Some(native.as_str())
                && normalize_local(&literal(selector).unwrap_or_default()) != native
            {
                return Err(error("native target and literal selector disagree"));
            }
            super::markdown::selector_address(markdown, selector)?
                .or_else(|| archived_selector_key(selector, rows))
                .unwrap_or(native)
        } else {
            native
        };
        let source_type = numeric_prefix(&origin);
        if source_type != numeric_prefix(&target) {
            return Err(error(
                "native redirects must retain the original typed identity namespace",
            ));
        }
        retired.insert(origin.clone());
        insert(&mut edges, origin.clone(), &target)?;
        if let Some(crate::write::EntrySelector::Document {
            document: scope,
            heading,
            entry: None,
        }) = &from_selector
        {
            for start in 0..heading.len() {
                let suffix_heading = heading[start..].to_vec();
                let suffix_key = super::markdown::heading_address(scope, &suffix_heading);
                let display = format!("{scope}#{}", suffix_heading.join("/"));
                for suffix in BTreeSet::from([suffix_key, display]) {
                    match suffixes.entry(suffix) {
                        std::collections::btree_map::Entry::Vacant(slot) => {
                            slot.insert(Some((origin.clone(), target.clone())));
                        }
                        std::collections::btree_map::Entry::Occupied(mut slot) => {
                            if slot
                                .get()
                                .as_ref()
                                .is_some_and(|(previous, _)| previous != &origin)
                            {
                                slot.insert(None);
                            }
                        }
                    }
                }
            }
        }
    }
    for (suffix, choice) in suffixes {
        if edges.contains_key(&suffix) {
            continue;
        }
        if let Some((_, target)) = choice {
            edges.insert(suffix, target);
        }
    }
    let resolved = alias_index(&[], identities, &edges, markdown)?;
    for origin in &retired {
        if identities.contains(origin) {
            return Err(MergeError::content(
                "merge.redirect_ambiguous",
                "retired native identity was reused by live content",
            )
            .at("trace/logic_mutations.yaml"));
        }
    }
    Ok(resolved)
}
pub(crate) fn allocation(
    base: &[EntryIdentity],
    ours: &[EntryIdentity],
    theirs: &[EntryIdentity],
    ledger: &Ledger,
    options: &MergeOptions,
) -> Result<(IdentityMap, Vec<ImportMapping>), MergeError> {
    let base_ids: BTreeSet<&str> = base.iter().map(|e| e.address.as_str()).collect();
    let ours_ids: BTreeSet<&str> = ours.iter().map(|e| e.address.as_str()).collect();
    let mut reserved: BTreeSet<String> = ours.iter().map(|e| e.address.clone()).collect();
    let mut previous = BTreeMap::new();
    for record in &ledger.records {
        if let Record::Revision {
            source_key,
            mappings,
            ..
        } = record
        {
            for mapping in mappings {
                reserved.insert(mapping.target.clone());
                if source_key == &options.source_key
                    && previous
                        .insert(mapping.original.clone(), mapping.target.clone())
                        .is_some_and(|old| old != mapping.target)
                {
                    return Err(MergeError::content(
                        "merge.corrupt_ledger",
                        "source import mapping changed across revisions",
                    ));
                }
            }
        }
    }
    let mut maxima: BTreeMap<char, u64> = BTreeMap::new();
    for id in &reserved {
        if let Some(prefix) = numeric_prefix(id) {
            let number = id[1..]
                .parse::<u64>()
                .map_err(|_| MergeError::content("merge.id_overflow", "numeric ID overflow"))?;
            maxima
                .entry(prefix)
                .and_modify(|max| *max = (*max).max(number))
                .or_insert(number);
        }
    }
    let mut sessions = BTreeMap::new();
    for id in &reserved {
        if let Some((date, sequence)) = session_parts(id) {
            sessions
                .entry(date.to_string())
                .and_modify(|max: &mut u64| *max = (*max).max(sequence))
                .or_insert(sequence);
        }
    }
    let mut ordinals: BTreeMap<String, u64> = BTreeMap::new();
    for address in &reserved {
        if let Some((prefix, ordinal)) = ordinal_parts(address) {
            ordinals
                .entry(prefix.into())
                .and_modify(|max| *max = (*max).max(ordinal))
                .or_insert(ordinal);
        }
    }
    let mut map: IdentityMap = previous.clone().into();
    let mut imports = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in theirs {
        if !seen.insert(&entry.address) {
            return Err(MergeError::content(
                "merge.duplicate_identity",
                format!("duplicate incoming identity `{}`", entry.address),
            ));
        }
        let target = if let Some(target) = previous.get(&entry.address) {
            target.clone()
        } else if base_ids.contains(entry.address.as_str()) {
            entry.address.clone()
        } else if let Some(prefix) = entry.numeric {
            let next = maxima.entry(prefix).or_default();
            *next = next.checked_add(1).ok_or_else(|| {
                MergeError::content("merge.id_overflow", "destination ID space exhausted")
            })?;
            format!("{prefix}{next:02}")
        } else if entry.session && ours_ids.contains(entry.address.as_str()) {
            let (date, _) = session_parts(&entry.address).ok_or_else(|| {
                MergeError::content("merge.session_identity", "invalid session identity")
            })?;
            let next = sessions.entry(date.into()).or_default();
            *next = next.checked_add(1).ok_or_else(|| {
                MergeError::content("merge.id_overflow", "session sequence exhausted")
            })?;
            format!("{date}_{next:03}")
        } else if let Some((prefix, _)) = ordinal_parts(&entry.address).filter(|_| {
            !matches!(
                entry.layer.as_str(),
                "session_occurrence" | "annotation_occurrence"
            ) && ours_ids.contains(entry.address.as_str())
        }) {
            let next = ordinals.entry(prefix.into()).or_default();
            *next = next.checked_add(1).ok_or_else(|| {
                MergeError::content("merge.id_overflow", "append occurrence sequence exhausted")
            })?;
            format!("{prefix}/{next}")
        } else {
            entry.address.clone()
        };
        reserved.insert(target.clone());
        map.insert(entry.address.clone(), target.clone());
        imports.push(ImportMapping {
            source_key: options.source_key.clone(),
            original: entry.address.clone(),
            target,
            layer: entry.layer.clone(),
            path: entry.path.clone(),
        });
    }
    // Session file identities must relocate with their logical session identity.
    let session_relocations: Vec<(String, String)> = map
        .iter()
        .filter(|(old, _)| session_parts(old).is_some())
        .map(|(old, new)| {
            (
                format!("trace/sessions/{old}.yaml"),
                format!("trace/sessions/{new}.yaml"),
            )
        })
        .collect();
    let positions: BTreeMap<String, usize> = imports
        .iter()
        .enumerate()
        .map(|(index, mapping)| (mapping.original.clone(), index))
        .collect();
    for (old, new) in session_relocations {
        map.insert(old.clone(), new.clone());
        if let Some(index) = positions.get(&old) {
            imports[*index].target = new;
        }
    }
    for entry in theirs.iter().filter(|entry| {
        matches!(
            entry.layer.as_str(),
            "session_occurrence" | "annotation_occurrence"
        ) && !previous.contains_key(&entry.address)
    }) {
        let (prefix, ordinal) = ordinal_parts(&entry.address).ok_or_else(|| {
            MergeError::content("merge.occurrence", "invalid append occurrence identity")
        })?;
        let (owner, field) = prefix.split_once('#').ok_or_else(|| {
            MergeError::content("merge.occurrence", "append occurrence has no owner")
        })?;
        let owner = map.get(owner).map_or(owner, String::as_str);
        let prefix = format!("{owner}#{field}");
        let target = if base_ids.contains(entry.address.as_str()) {
            format!("{prefix}/{ordinal}")
        } else {
            let next = ordinals.get(&prefix).map_or(Ok(0), |max| {
                max.checked_add(1).ok_or_else(|| {
                    MergeError::content("merge.id_overflow", "append occurrence space exhausted")
                })
            })?;
            ordinals.insert(prefix.clone(), next);
            format!("{prefix}/{next}")
        };
        map.insert(entry.address.clone(), target.clone());
        imports[*positions
            .get(&entry.address)
            .expect("inventoried occurrence")]
        .target = target;
    }
    let nested: Vec<(String, String)> = theirs
        .iter()
        .filter(|entry| !previous.contains_key(&entry.address))
        .filter_map(|entry| {
            if entry.heading.is_empty()
                || entry.numeric.is_some()
                || (entry.path == "logic/related_work.md" && entry.address.starts_with("RW"))
            {
                return None;
            }
            let destination_path = map.get(&entry.path).unwrap_or(&entry.path);
            let components: Vec<String> = entry
                .heading
                .iter()
                .map(|(literal, numeric)| {
                    match numeric
                        .as_ref()
                        .and_then(|id| map.get(id).map(|target| (id, target)))
                    {
                        Some((_, target)) => target.clone(),
                        None => literal.clone(),
                    }
                })
                .collect();
            let target =
                if serde_json::from_str::<crate::write::EntrySelector>(&entry.address).is_ok() {
                    super::markdown::exact_heading_address(destination_path, &components)
                } else {
                    super::markdown::heading_address(destination_path, &components)
                };
            (target != entry.address).then(|| (entry.address.clone(), target))
        })
        .collect();
    for (original, target) in nested {
        map.insert(original.clone(), target.clone());
        if let Some(index) = positions.get(&original) {
            imports[*index].target = target;
        }
    }
    reference_namespaces(theirs, &mut map)?;
    Ok((map, imports))
}
pub(crate) fn reference_namespaces(
    entries: &[EntryIdentity],
    map: &mut IdentityMap,
) -> Result<(), MergeError> {
    for entry in entries {
        let target = map.get(&entry.address).cloned().ok_or_else(|| {
            MergeError::content(
                "merge.identity",
                "captured source entry has no recorded import mapping",
            )
        })?;
        let token = super::markdown::display_address(&entry.address);
        map.tokens
            .entry(token)
            .and_modify(|found| *found = None)
            .or_insert(Some(target));
        if entry.path == "logic/concepts.md" && entry.heading.len() == 2 {
            map.concepts
                .insert(entry.heading[1].0.clone(), entry.address.clone());
            map.concepts.insert(
                super::markdown::display_address(&entry.address),
                entry.address.clone(),
            );
            let full = entry
                .heading
                .iter()
                .map(|(literal, _)| literal.as_str())
                .collect::<Vec<_>>()
                .join("/");
            map.concepts
                .insert(format!("{}#{full}", entry.path), entry.address.clone());
        }
    }
    Ok(())
}
pub(crate) fn numeric_prefix(id: &str) -> Option<char> {
    let first = id.chars().next()?;
    (matches!(first, 'N' | 'O' | 'C' | 'H' | 'E' | 'T')
        && id.len() > 1
        && id[1..].bytes().all(|b| b.is_ascii_digit()))
    .then_some(first)
}
fn ordinal_parts(address: &str) -> Option<(&str, u64)> {
    let (prefix, ordinal) = address.rsplit_once('/')?;
    let (_, field) = prefix.split_once('#')?;
    if !matches!(
        field,
        "entries"
            | "records"
            | "mutations"
            | "events_logged"
            | "ai_actions"
            | "claims_touched"
            | "logic_revisions"
            | "key_context"
            | "conflict_annotations"
    ) {
        return None;
    }
    Some((prefix, ordinal.parse().ok()?))
}
pub(crate) fn session_parts(id: &str) -> Option<(&str, u64)> {
    let (date, sequence) = id.split_once('_')?;
    if date.len() != 10
        || date.as_bytes()[4] != b'-'
        || date.as_bytes()[7] != b'-'
        || !date
            .bytes()
            .enumerate()
            .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit())
        || sequence.len() < 3
        || !sequence.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some((date, sequence.parse().ok()?))
}
pub(crate) fn append<T: Serialize>(
    working: &mut WorkingArtifact,
    path: &str,
    format: &str,
    key: &str,
    records: &[T],
) -> Result<(), MergeError> {
    if records.is_empty() {
        return Ok(());
    }
    let mut lines = String::new();
    for record in records {
        lines.push_str("  - ");
        lines.push_str(
            &serde_json::to_string(record)
                .map_err(|e| MergeError::content("merge.journal_encoding", e.to_string()))?,
        );
        lines.push('\n');
    }
    let output = if working.exists(path) {
        let source = working.text(path)?;
        let prefix = format!("format: {format}\n{key}:\n");
        if source.strip_prefix(&prefix).is_some_and(|tail| {
            !tail.trim().is_empty()
                && tail
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .all(|line| line.starts_with("  - {\""))
        }) {
            // The validated native envelope already ends in its record list.
            // Preserve all preimage bytes without indexing captured payloads.
            let mut output = String::with_capacity(source.len() + lines.len() + 1);
            output.push_str(source);
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(&lines);
            working.stage_replace(
                path,
                output.as_bytes(),
                "append immutable portable merge records",
            )?;
            return Ok(());
        }
        let document = YamlDocument::parse(source)?;
        let node = document.root.get(key)?.ok_or_else(|| {
            MergeError::content("merge.corrupt_ledger", "portable record list missing")
        })?;
        let items = node.sequence()?;
        if node.flow {
            if !items.is_empty() {
                return Err(MergeError::content(
                    "merge.unsupported_ledger_layout",
                    "nonempty flow ledger cannot be appended losslessly",
                ));
            }
            let mut out = source[..node.start].trim_end_matches(' ').to_string();
            out.push('\n');
            out.push_str(&lines);
            out.push_str(&source[node.end..]);
            out
        } else {
            if !source[node.end..].trim().is_empty() {
                return Err(MergeError::content(
                    "merge.unsupported_ledger_layout",
                    "portable record list must be the final root field",
                ));
            }
            let mut out = source.to_string();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&lines);
            out
        }
    } else {
        format!("format: {format}\n{key}:\n{lines}")
    };
    if working.exists(path) {
        working.stage_replace(
            path,
            output.as_bytes(),
            "append immutable portable merge records",
        )?;
    } else {
        working.stage_create(path, output.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod native_record_decode_tests {
    use super::{aliases_bytes, load_bytes};

    #[test]
    fn native_json_records_reject_duplicate_captured_source_paths_before_validation() {
        let row = r#"{"kind":"revision","source_key":"peer","fingerprint":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","base":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","predecessor":null,"time":"2026-10-01T10:00","git":null,"files":{"logic/claims.md":"","logic/claims.md":"AQ=="},"mappings":[]}"#;
        let native = format!("format: ara.merge-log/v1\nrecords:\n  - {row}\n");
        let error = load_bytes(native.as_bytes()).unwrap_err();
        assert_eq!(error.code, "merge.corrupt_ledger");
        assert!(error.message.contains("duplicate source payload path"));
    }

    #[test]
    fn native_alias_rows_preserve_values_and_reject_ambiguous_fields() {
        let native = "format: ara.aliases/v1\naliases:\n  - {\"source_key\":\"peer\",\"label\":\"peer display\",\"original\":\"logic/concepts.md#α/β\",\"target\":\"logic/concepts.md#γ/δ\",\"revision\":\"revision\"}\n";
        let block = "format: ara.aliases/v1\naliases:\n  - source_key: peer\n    label: peer display\n    original: logic/concepts.md#α/β\n    target: logic/concepts.md#γ/δ\n    revision: revision\n";
        assert_eq!(
            aliases_bytes(native.as_bytes()).unwrap(),
            aliases_bytes(block.as_bytes()).unwrap()
        );
        for extra in [
            ",\"target\":\"different\"",
            ",\"unknown\":\"silently discarded\"",
        ] {
            let malformed = native.replace("}\n", &format!("{extra}}}\n"));
            assert_eq!(
                aliases_bytes(malformed.as_bytes()).unwrap_err().code,
                "merge.alias_data"
            );
        }
    }
}

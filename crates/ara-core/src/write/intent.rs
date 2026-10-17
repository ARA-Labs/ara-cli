//! Exact source edits with complete YAML-value delta validation.
use super::{
    WriteError,
    positions::{PathPart, SourceValue, YamlDocument, YamlKind, YamlNode},
};
use std::ops::Range;

#[derive(Debug, Clone)]
pub struct Intent {
    pub path: String,
    pub range: Range<usize>,
    pub before_digest: String,
    pub after_digest: String,
    pub reason: String,
}

/// Native merge staging supplies these reasons; public authoring operations
/// cannot select them. A captured, validated portable revision must accompany
/// the import. Generic edits mixed into that path revoke this history boundary.
pub(crate) fn imports_history(
    working: &super::WorkingArtifact,
    path: &str,
) -> Result<bool, WriteError> {
    let mut imported = false;
    for intent in working.intents.iter().filter(|intent| intent.path == path) {
        if matches!(
            intent.reason.as_str(),
            "compose exact lossless YAML merge spans"
                | "merge exact Markdown span composition"
                | "create exact imported knowledge source"
        ) {
            imported = true;
        } else if !intent.reason.starts_with("node.")
            && !intent.reason.starts_with("observation.stale:")
            && !intent
                .reason
                .starts_with("append immutable node conflict annotation:")
        {
            return Ok(false);
        }
    }
    const LOG: &str = "trace/merge_log.yaml";
    if !imported || !working.exists(LOG) {
        return Ok(false);
    }
    // Reuse the merger's strict ledger decoder and validation, rather than
    // introducing a second portable-record grammar in the writer.
    crate::merge::imported_path_present(working, path)
        .map_err(|error| WriteError::semantic(&error.code, error.message))
}

pub fn validate_bytes(
    before: &[u8],
    after: &[u8],
    range: Range<usize>,
    replacement: &[u8],
) -> Result<(), WriteError> {
    if range.start > range.end || range.end > before.len() {
        return Err(reject("edit range lies outside preimage"));
    }
    let suffix_start = range
        .start
        .checked_add(replacement.len())
        .ok_or_else(|| reject("edit size overflow"))?;
    if after.len() != before.len() - range.len() + replacement.len()
        || before[..range.start] != after[..range.start]
        || before[range.end..] != after[suffix_start..]
        || replacement != &after[range.start..suffix_start]
    {
        return Err(reject("bytes outside declared source edit changed"));
    }
    Ok(())
}

pub fn validate_yaml(
    before: &YamlDocument,
    after: &YamlDocument,
    path: &[PathPart],
    change: YamlDelta,
) -> Result<(), WriteError> {
    if !compare_path(&before.root, &after.root, path, &change)? {
        return Err(reject(
            "complete source values changed outside the operation's declared semantic delta",
        ));
    }
    Ok(())
}
pub enum YamlDelta {
    Append(SourceValue),
    AppendMany(Vec<SourceValue>),
    Field(String, SourceValue),
}

// Walk borrowed source trees. A complete guard does not require cloning the
// entire artifact representation for every append.
fn compare_path(
    mut before: &YamlNode,
    mut after: &YamlNode,
    path: &[PathPart],
    change: &YamlDelta,
) -> Result<bool, WriteError> {
    for part in path {
        if before.anchor != after.anchor || before.tag != after.tag {
            return Ok(false);
        }
        let (next_before, next_after) = match (part, &before.kind, &after.kind) {
            (PathPart::Index(index), YamlKind::Sequence(old), YamlKind::Sequence(new)) => {
                if old.len() != new.len() || *index >= old.len() {
                    return Ok(false);
                }
                if old
                    .iter()
                    .zip(new)
                    .enumerate()
                    .any(|(i, (a, b))| i != *index && !same(a, b))
                {
                    return Ok(false);
                }
                (&old[*index], &new[*index])
            }
            (PathPart::Key(key), YamlKind::Mapping(old), YamlKind::Mapping(new)) => {
                let index =
                    unique_key(old, key)?.ok_or_else(|| reject("intent mapping key missing"))?;
                if old.len() != new.len() {
                    return Ok(false);
                }
                if old
                    .iter()
                    .zip(new)
                    .enumerate()
                    .any(|(i, ((ak, av), (bk, bv)))| !same(ak, bk) || (i != index && !same(av, bv)))
                {
                    return Ok(false);
                }
                (&old[index].1, &new[index].1)
            }
            _ => return Err(reject("intent source path has incompatible value")),
        };
        before = next_before;
        after = next_after;
    }
    if before.anchor != after.anchor || before.tag != after.tag {
        return Ok(false);
    }
    match change {
        YamlDelta::Append(value) => match (&before.kind, &after.kind) {
            (YamlKind::Sequence(old), YamlKind::Sequence(new)) => Ok(new.len() == old.len() + 1
                && old.iter().zip(new).all(|(a, b)| same(a, b))
                && new.last().is_some_and(|node| node.semantic() == *value)),
            _ => Err(reject("append intent target is not a sequence")),
        },
        YamlDelta::AppendMany(values) => match (&before.kind, &after.kind) {
            (YamlKind::Sequence(old), YamlKind::Sequence(new)) => Ok(new.len()
                == old.len() + values.len()
                && old.iter().zip(new).all(|(a, b)| same(a, b))
                && new[old.len()..]
                    .iter()
                    .zip(values)
                    .all(|(node, value)| node.semantic() == *value)),
            _ => Err(reject("append intent target is not a sequence")),
        },
        YamlDelta::Field(key, value) => match (&before.kind, &after.kind) {
            (YamlKind::Mapping(old), YamlKind::Mapping(new)) => {
                let index = unique_key(old, key)?;
                if let Some(index) = index {
                    Ok(old.len() == new.len()
                        && old
                            .iter()
                            .zip(new)
                            .enumerate()
                            .all(|(i, ((ak, av), (bk, bv)))| {
                                same(ak, bk)
                                    && if i == index {
                                        bv.semantic() == *value
                                    } else {
                                        same(av, bv)
                                    }
                            }))
                } else {
                    let appended = new.last();
                    Ok(new.len() == old.len() + 1
                        && old
                            .iter()
                            .zip(new)
                            .all(|((ak, av), (bk, bv))| same(ak, bk) && same(av, bv))
                        && appended.is_some_and(|(k, v)| {
                            k.scalar() == Some(key.as_str())
                                && k.anchor == 0
                                && k.tag.is_none()
                                && k.style == Some(yaml_rust2::scanner::TScalarStyle::Plain)
                                && v.semantic() == *value
                        }))
                }
            }
            _ => Err(reject("field intent target is not a mapping")),
        },
    }
}
fn unique_key(entries: &[(YamlNode, YamlNode)], key: &str) -> Result<Option<usize>, WriteError> {
    let mut indexes = entries
        .iter()
        .enumerate()
        .filter(|(_, (k, _))| k.scalar() == Some(key))
        .map(|(i, _)| i);
    let index = indexes.next();
    if indexes.next().is_some() {
        return Err(reject("intent mapping key ambiguous"));
    }
    Ok(index)
}
fn same(a: &YamlNode, b: &YamlNode) -> bool {
    a.same_source_value(b)
}
fn reject(message: &str) -> WriteError {
    WriteError::semantic("write.intent", message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn omitted_unknown_mapping_cannot_change() {
        let before = YamlDocument::parse("x: old\nunknown: {nested: [1, 2]}\n").unwrap();
        let after = YamlDocument::parse("x: new\nunknown: {nested: [1, 9]}\n").unwrap();
        let new = YamlDocument::parse("new\n").unwrap().root.semantic();
        assert!(validate_yaml(&before, &after, &[], YamlDelta::Field("x".into(), new)).is_err());
    }
    #[test]
    fn undeclared_bytes_reject() {
        assert!(validate_bytes(b"abc", b"axz", 1..2, b"x").is_err());
    }

    #[test]
    fn imported_history_authentication_tracks_exact_candidate_and_preimage_ledger_bytes() {
        use super::super::{
            ArtifactSnapshot,
            source::{FileSnapshot, digest},
        };
        use crate::merge::{MergeOptions, plan_merge};
        use std::{collections::BTreeMap, path::PathBuf};

        const TREE: &str = "trace/exploration_tree.yaml";
        const LOG: &str = "trace/merge_log.yaml";
        let snapshot = |tree: &str| ArtifactSnapshot {
            root: PathBuf::from("/nonexistent/ledger-cache-fixture"),
            identity_paths: Default::default(),
            files: BTreeMap::from([(
                TREE.into(),
                FileSnapshot {
                    bytes: tree.as_bytes().to_vec(),
                    existed: true,
                    permissions: None,
                    digest: digest(tree.as_bytes()),
                },
            )]),
        };
        let base = snapshot("tree: []\n");
        let theirs = snapshot(
            "tree:\n  - id: N77\n    type: question\n    title: Imported source\n    description: Captured original history\n",
        );
        let mut working = plan_merge(
            &base,
            &base,
            &theirs,
            &MergeOptions {
                source_key: "peer".into(),
                label: "peer".into(),
                time: "2026-10-01T12:00Z".into(),
                git: None,
                predecessor: None,
            },
        )
        .unwrap()
        .working;
        let original = working.bytes(LOG).unwrap().to_vec();
        assert!(imports_history(&working, TREE).unwrap());

        let malformed = b"format: ara.merge-log/v1\nrecords: [malformed]\n";
        assert_eq!(
            working
                .stage_replace(LOG, malformed, "append immutable portable merge records")
                .unwrap_err()
                .code,
            "merge.corrupt_ledger"
        );
        assert_eq!(working.bytes(LOG).unwrap(), original);
        assert!(imports_history(&working, TREE).unwrap());

        // Direct public-map writes deliberately bypass cache invalidation.
        working.files.insert(LOG.into(), malformed.to_vec());
        assert_eq!(
            imports_history(&working, TREE).unwrap_err().code,
            "merge.corrupt_ledger"
        );
        working.files.insert(LOG.into(), original.clone());
        assert!(imports_history(&working, TREE).unwrap());
        working.files.insert(
            LOG.into(),
            b"format: ara.merge-log/v1\nrecords: []\n".to_vec(),
        );
        assert!(!imports_history(&working, TREE).unwrap());

        working.files.remove(LOG);
        working.base.files.insert(
            LOG.into(),
            FileSnapshot {
                digest: digest(&original),
                bytes: original,
                existed: true,
                permissions: None,
            },
        );
        assert!(imports_history(&working, TREE).unwrap());
        // The stale public digest is not proof of the changed preimage.
        working.base.files.get_mut(LOG).unwrap().bytes = malformed.to_vec();
        assert_eq!(
            imports_history(&working, TREE).unwrap_err().code,
            "merge.corrupt_ledger"
        );
        working.deleted_paths.insert(LOG.into());
        assert!(!imports_history(&working, TREE).unwrap());
    }
}

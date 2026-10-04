//! Pure lossless directory merge planning. Native adapters own snapshots, locks,
//! source rechecks, captured time, and the single shared durable transaction.
//! The portable policy is the proposed agent CLI contract, pending upstream review.
mod identity;
mod markdown;
mod origin;
mod rewrite;
mod types;
mod yaml;
use crate::write::source::FileSnapshot;
use crate::write::{ArtifactSnapshot, WorkingArtifact, WriteOperation};
use identity::{ALIASES, Alias, LOG, Record, bytes};
pub(crate) use identity::{Ledger, load_bytes as decode_ledger_bytes};
pub use identity::{SourceHistory, fingerprint, source_history};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
pub use types::{
    ConflictLocator, GitMergeProvenance, ImportMapping, MergeConflict, MergeError, MergeOptions,
    MergePlan, MergeReport, MergeValue, RewriteFact,
};
use types::{EntryIdentity, IdentityMap, conflict};

struct Inventory {
    yaml: yaml::Inventory,
    markdown: markdown::Inventory,
    entries: Vec<EntryIdentity>,
    paths: BTreeSet<String>,
    ledger: Ledger,
    redirects: BTreeMap<String, String>,
    mutations: Vec<serde_json::Value>,
}

pub(crate) fn imported_path_present(
    working: &WorkingArtifact,
    path: &str,
) -> Result<bool, MergeError> {
    Ok(working
        .merge_ledger()?
        .records
        .iter()
        .any(|record| matches!(record, Record::Revision { files, .. } if files.contains_key(path))))
}

fn inventory(snapshot: &ArtifactSnapshot) -> Result<Inventory, MergeError> {
    inventory_view(snapshot, true)
}
fn live_inventory(snapshot: &ArtifactSnapshot) -> Result<Inventory, MergeError> {
    inventory_view(snapshot, false)
}
fn inventory_view(
    snapshot: &ArtifactSnapshot,
    include_history: bool,
) -> Result<Inventory, MergeError> {
    inventory_with_yaml(snapshot, include_history, yaml::inventory(snapshot)?)
}
fn inventory_with_yaml(
    snapshot: &ArtifactSnapshot,
    include_history: bool,
    yaml: yaml::Inventory,
) -> Result<Inventory, MergeError> {
    let markdown = markdown::inventory(snapshot)?;
    let mut entries: Vec<EntryIdentity> = yaml
        .entries
        .iter()
        .chain(&markdown.entries)
        .cloned()
        .collect();
    let paths: BTreeSet<String> = yaml.paths.union(&markdown.paths).cloned().collect();
    let mut known: BTreeSet<String> = entries.iter().map(|entry| entry.address.clone()).collect();
    if known.len() != entries.len() {
        return Err(MergeError::content(
            "merge.duplicate_identity",
            "artifact has duplicate native entry identities; lossless merge refuses normalization",
        ));
    }
    let inventory_paths: BTreeSet<&String> = snapshot
        .identity_paths
        .iter()
        .chain(
            snapshot
                .files
                .iter()
                .filter(|(_, file)| file.existed)
                .map(|(path, _)| path),
        )
        .collect();
    for path in inventory_paths {
        identity::safe_path(path)?;
        if !identity::private_path(path) && known.insert(path.clone()) {
            entries.push(EntryIdentity {
                address: path.clone(),
                layer: if path.starts_with("src/") || path.starts_with("evidence/") {
                    "external"
                } else {
                    "document"
                }
                .into(),
                path: path.clone(),
                numeric: None,
                session: false,
                heading: Vec::new(),
            });
        }
    }
    let mutations = identity::mutation_rows(snapshot)?;
    let redirects = identity::local_redirects(snapshot, &known, &markdown, &mutations)?;
    // Retired imports reserve identity slots, but are not live alias terminals.
    let ledger = if include_history {
        identity::load(snapshot)?
    } else {
        Ledger::default()
    };
    for fact in ledger.records.iter().filter_map(Record::fact) {
        for mapping in fact.mappings {
            if known.insert(mapping.target.clone()) {
                entries.push(EntryIdentity {
                    address: mapping.target.clone(),
                    layer: "historical_identity".into(),
                    path: mapping.path.clone(),
                    numeric: identity::numeric_prefix(&mapping.target),
                    session: identity::session_parts(&mapping.target).is_some(),
                    heading: Vec::new(),
                });
            }
        }
    }
    if include_history {
        for entry in yaml::mutation_origins(&yaml)? {
            if known.insert(entry.address.clone()) {
                entries.push(entry);
            }
        }
        for origin in redirects.keys() {
            if known.insert(origin.clone()) {
                let path = origin.split_once('#').map_or_else(
                    || match identity::numeric_prefix(origin) {
                        Some('C') => "logic/claims.md",
                        Some('H') => "logic/solution/heuristics.md",
                        Some('E') => "logic/experiments.md",
                        _ => "trace/logic_mutations.yaml",
                    },
                    |(path, _)| path,
                );
                entries.push(EntryIdentity {
                    address: origin.clone(),
                    layer: "historical_identity".into(),
                    path: path.into(),
                    numeric: identity::numeric_prefix(origin),
                    session: false,
                    heading: Vec::new(),
                });
            }
        }
    }
    Ok(Inventory {
        yaml,
        markdown,
        entries,
        paths,
        ledger,
        redirects,
        mutations,
    })
}
fn ids(view: &Inventory) -> BTreeSet<String> {
    view.entries
        .iter()
        .filter(|entry| entry.layer != "historical_identity")
        .map(|entry| entry.address.clone())
        .collect()
}
fn captured(
    files: &BTreeMap<String, Vec<u8>>,
    root: &std::path::Path,
    ledger: &Ledger,
    source_key: &str,
    revision: &str,
) -> ArtifactSnapshot {
    let mut snapshot = ArtifactSnapshot {
        root: root.into(),
        identity_paths: files.keys().cloned().collect(),
        files: files
            .iter()
            .map(|(path, bytes)| {
                (
                    path.clone(),
                    FileSnapshot {
                        bytes: bytes.clone(),
                        existed: true,
                        permissions: None,
                        digest: crate::write::source::digest(bytes),
                    },
                )
            })
            .collect(),
    };
    for record in &ledger.records {
        if let Record::Transport {
            source_key: key,
            revision: at,
            path,
            bytes,
        } = record
            && key == source_key
            && at == revision
        {
            snapshot.files.insert(
                path.clone(),
                FileSnapshot {
                    bytes: bytes.clone(),
                    existed: true,
                    permissions: None,
                    digest: crate::write::source::digest(bytes),
                },
            );
        }
    }
    snapshot
}
fn candidate(working: &WorkingArtifact) -> ArtifactSnapshot {
    // External bodies stay read-only; their paths validate references without
    // copying code and evidence into another owned knowledge candidate.
    let identity_paths = working
        .base
        .identity_paths
        .iter()
        .cloned()
        .chain(
            working
                .base
                .files
                .iter()
                .filter(|(_, file)| file.existed)
                .map(|(path, _)| path.clone()),
        )
        .chain(working.files.keys().cloned())
        .filter(|path| !working.deleted_paths.contains(path))
        .collect();
    let mut result = ArtifactSnapshot {
        root: working.base.root.clone(),
        identity_paths,
        files: working
            .base
            .files
            .iter()
            .filter(|(path, _)| !path.starts_with("src/") && !path.starts_with("evidence/"))
            .map(|(path, file)| (path.clone(), file.clone()))
            .collect(),
    };
    for path in &working.deleted_paths {
        result.files.remove(path);
    }
    for (path, bytes) in &working.files {
        let permissions = result
            .files
            .get(path)
            .and_then(|file| file.permissions.clone());
        result.files.insert(
            path.clone(),
            FileSnapshot {
                bytes: bytes.clone(),
                existed: true,
                permissions,
                digest: crate::write::source::digest(bytes),
            },
        );
    }
    result
}
fn evidence(
    mut error: MergeError,
    base: &ArtifactSnapshot,
    ours: &ArtifactSnapshot,
    theirs: &ArtifactSnapshot,
    options: &MergeOptions,
    path: &str,
) -> MergeError {
    if error.evidence.is_empty() {
        let actual = error.field.as_deref().filter(|field| {
            base.files.contains_key(*field)
                || ours.files.contains_key(*field)
                || theirs.files.contains_key(*field)
        });
        let path = actual.unwrap_or_else(|| {
            if error.code.starts_with("merge.alias") {
                ALIASES
            } else if error.code.starts_with("merge.redirect") {
                "trace/logic_mutations.yaml"
            } else if error.code == "merge.session_index" {
                "trace/sessions/session_index.yaml"
            } else if error.code == "merge.corrupt_ledger" {
                LOG
            } else {
                path
            }
        });
        let mut report = MergeReport::new(options, fingerprint(theirs));
        let item = conflict(
            &mut report,
            path,
            path,
            "file",
            &error.code,
            bytes(base, path),
            bytes(ours, path),
            bytes(theirs, path),
            ConflictLocator::Document,
        );
        error.evidence.push(item);
    }
    error
}
fn validate_metadata<'a>(
    snapshot: &ArtifactSnapshot,
    view: &'a Inventory,
) -> Result<(&'a Ledger, Vec<Alias>), MergeError> {
    let aliases = identity::aliases(snapshot)?;
    identity::alias_index(&aliases, &ids(view), &view.redirects, &view.markdown)?;
    Ok((&view.ledger, aliases))
}
fn validate_candidate(working: &WorkingArtifact) -> Result<Arc<Ledger>, MergeError> {
    working.validate()?;
    let snapshot = candidate(working);
    let view = live_inventory(&snapshot)?;
    validate_candidate_view(working, &view)
}
fn validate_candidate_view(
    working: &WorkingArtifact,
    view: &Inventory,
) -> Result<Arc<Ledger>, MergeError> {
    let mut identities = ids(view);
    identities.extend(
        [LOG, ALIASES]
            .into_iter()
            .filter(|path| working.exists(path))
            .map(str::to_owned),
    );
    identities.extend(
        working
            .paths()
            .into_iter()
            .filter(|path| path.starts_with("src/") || path.starts_with("evidence/")),
    );
    let references = identity::alias_index(
        &if working.exists(ALIASES) {
            identity::aliases_bytes(working.bytes(ALIASES)?)?
        } else {
            Vec::new()
        },
        &identities,
        &view.redirects,
        &view.markdown,
    )?;
    yaml::validate_references(&view.yaml, &identities, &references, &view.markdown)?;
    markdown::validate_references(&view.markdown, &identities, &references)?;
    if working.exists(LOG) {
        working.merge_ledger()
    } else {
        Ok(Arc::new(Ledger::default()))
    }
}
/// The destination's own source key: explicit, recorded once, never inferred.
fn self_identity(ledger: &Ledger, options: &MergeOptions) -> Result<Option<String>, MergeError> {
    let recorded = ledger.self_key();
    if let Some(key) = &options.self_key {
        identity::validate_name(key, false)?;
        if recorded.is_some_and(|recorded| recorded != key) {
            return Err(MergeError::content(
                "merge.self_identity_conflict",
                format!(
                    "this destination is recorded as `{}`, not `{key}`",
                    recorded.unwrap_or_default()
                ),
            ));
        }
    }
    let own = options.self_key.as_deref().or(recorded);
    if own == Some(options.source_key.as_str()) {
        return Err(MergeError::content(
            "merge.self_identity_conflict",
            "the transport source key cannot be this destination's own key",
        ));
    }
    if own.is_some_and(|own| ledger.history(own).is_some()) {
        return Err(MergeError::content(
            "merge.self_identity_conflict",
            "this destination previously imported its own key as a source",
        ));
    }
    Ok(own.map(str::to_owned))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergePhase {
    PlanningFinished,
    ValidationFinished,
}
pub fn plan_merge(
    base: &ArtifactSnapshot,
    ours: &ArtifactSnapshot,
    theirs: &ArtifactSnapshot,
    options: &MergeOptions,
) -> Result<MergePlan, MergeError> {
    plan_merge_with_observer(base, ours, theirs, options, |_| {})
}
fn input_inventories(
    base: &ArtifactSnapshot,
    ours: &ArtifactSnapshot,
    theirs: &ArtifactSnapshot,
) -> Result<[Inventory; 3], MergeError> {
    #[cfg(not(target_family = "wasm"))]
    let inputs = [base, ours, theirs];
    #[cfg(not(target_family = "wasm"))]
    if inputs
        .iter()
        .filter_map(|snapshot| bytes(snapshot, "trace/exploration_tree.yaml"))
        .map(<[u8]>::len)
        .sum::<usize>()
        >= 3 * 1024 * 1024
    {
        return std::thread::scope(|scope| {
            let base = scope.spawn(|| inventory(base));
            let ours = scope.spawn(|| inventory(ours));
            let theirs = inventory(theirs);
            Ok([
                base.join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))?,
                ours.join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))?,
                theirs?,
            ])
        });
    }
    Ok([inventory(base)?, inventory(ours)?, inventory(theirs)?])
}

pub fn plan_merge_with_observer(
    base: &ArtifactSnapshot,
    ours: &ArtifactSnapshot,
    theirs: &ArtifactSnapshot,
    options: &MergeOptions,
    mut observer: impl FnMut(MergePhase),
) -> Result<MergePlan, MergeError> {
    identity::validate_name(&options.source_key, false)?;
    identity::validate_name(&options.label, true)?;
    crate::write::sessions::validate_timestamp(&options.time)?;
    for snapshot in [base, ours, theirs] {
        if bytes(snapshot, "trace/exploration_tree.yaml").is_none() {
            return Err(evidence(
                MergeError::content(
                    "merge.lineage",
                    "all inputs require a captured exploration tree from the same artifact lineage",
                ),
                base,
                ours,
                theirs,
                options,
                "trace/exploration_tree.yaml",
            ));
        }
    }
    let [original_base, ours_view, theirs_view] =
        input_inventories(base, ours, theirs).map_err(|error| {
            evidence(
                error,
                base,
                ours,
                theirs,
                options,
                "trace/exploration_tree.yaml",
            )
        })?;
    validate_metadata(base, &original_base)
        .map_err(|e| evidence(e, base, ours, theirs, options, LOG))?;
    let (ledger, ours_aliases) = validate_metadata(ours, &ours_view)
        .map_err(|e| evidence(e, base, ours, theirs, options, LOG))?;
    let (incoming_ledger, theirs_aliases) = validate_metadata(theirs, &theirs_view)
        .map_err(|e| evidence(e, base, ours, theirs, options, LOG))?;
    let revision = fingerprint(theirs);
    let supplied_base = fingerprint(base);
    let history = ledger.history(&options.source_key);
    let mut label_owner = BTreeMap::new();
    for record in &ledger.records {
        if let Record::Enrollment {
            source_key, label, ..
        }
        | Record::Label {
            source_key, label, ..
        } = record
        {
            label_owner.insert(label.as_str(), source_key.as_str());
        }
    }
    if label_owner
        .get(options.label.as_str())
        .is_some_and(|key| *key != options.source_key)
    {
        return Err(evidence(
            MergeError::content(
                "merge.ambiguous_label",
                "display label was previously enrolled for another source key",
            ),
            base,
            ours,
            theirs,
            options,
            LOG,
        ));
    }
    let own_key = self_identity(ledger, options)
        .map_err(|error| evidence(error, base, ours, theirs, options, LOG))?;
    let mut replay = false;
    let effective_base;
    if let Some(Record::Revision {
        fingerprint: previous,
        files,
        ..
    }) = ledger.revision(&options.source_key)
    {
        replay = previous == &revision;
        if !replay {
            if ledger.records.iter().any(|record|matches!(record,Record::Revision {source_key,fingerprint,..} if source_key==&options.source_key&&fingerprint==&revision)) { return Err(evidence(MergeError::content("merge.source_regression","an older enrolled source revision cannot replace the current revision"),base,ours,theirs,options,LOG)); }
            let directory_proof = supplied_base == *previous;
            let git_proof =
                options.git.is_some() && options.predecessor.as_deref() == Some(previous.as_str());
            if !directory_proof && !git_proof {
                return Err(evidence(
                    MergeError::content(
                        "merge.unproven_source_revision",
                        "advancing directory source requires its exact last imported revision as --base; Git requires proven local ancestry",
                    ),
                    base,
                    ours,
                    theirs,
                    options,
                    LOG,
                ));
            }
        }
        effective_base = if replay {
            std::borrow::Cow::Borrowed(theirs)
        } else if supplied_base == *previous {
            std::borrow::Cow::Borrowed(base)
        } else {
            std::borrow::Cow::Owned(captured(
                files,
                &base.root,
                ledger,
                &options.source_key,
                previous,
            ))
        };
    } else {
        if options.predecessor.is_some() {
            return Err(MergeError::content(
                "merge.unproven_source_revision",
                "new source cannot name an unenrolled predecessor",
            ));
        }
        effective_base = std::borrow::Cow::Borrowed(base);
    }
    if matches!(&effective_base, std::borrow::Cow::Owned(_))
        && history
            .as_ref()
            .is_some_and(|previous| fingerprint(&effective_base) != previous.fingerprint)
    {
        return Err(evidence(
            MergeError::content(
                "merge.corrupt_ledger",
                "captured prior source inventory does not match its exact revision fingerprint",
            ),
            base,
            ours,
            theirs,
            options,
            LOG,
        ));
    }
    let effective_inventory;
    let effective_view = if matches!(&effective_base, std::borrow::Cow::Borrowed(snapshot) if std::ptr::eq(*snapshot, base))
    {
        &original_base
    } else if matches!(&effective_base, std::borrow::Cow::Borrowed(snapshot) if std::ptr::eq(*snapshot, theirs))
    {
        &theirs_view
    } else {
        effective_inventory = inventory(&effective_base)?;
        &effective_inventory
    };
    for current in std::iter::once(theirs).chain(history.is_none().then_some(ours)) {
        if let Some(path) = identity::metadata_append_only(&effective_base, current)
            .map_err(|error| evidence(error, &effective_base, ours, theirs, options, LOG))?
        {
            let mut rejected = MergeReport::new(options, revision.clone());
            conflict(
                &mut rejected,
                path,
                path,
                "records",
                "protected_merge_metadata",
                bytes(&effective_base, path),
                bytes(ours, path),
                bytes(theirs, path),
                ConflictLocator::Document,
            );
            return Err(types::reject_protected(&rejected));
        }
    }
    let peer_feedback = origin::needed(ledger, incoming_ledger, &options.source_key);
    let origins = if peer_feedback {
        origin::reconcile(&origin::Context {
            ours: ledger,
            theirs: incoming_ledger,
            aliases: &theirs_aliases,
            theirs_redirects: &theirs_view.redirects,
            theirs_live: &ids(&theirs_view),
            ours_entries: &ours_view.entries,
            ours_redirects: &ours_view.redirects,
            transport: &options.source_key,
            own: own_key.as_deref(),
        })
        .map_err(|error| evidence(error, &effective_base, ours, theirs, options, ALIASES))?
    } else {
        origin::Origins::default()
    };
    let (mut map, imports) = identity::allocation(
        &effective_view.entries,
        &ours_view.entries,
        &theirs_view.entries,
        ledger,
        options,
        &origins.targets,
    )
    .map_err(|error| {
        if error.code == "merge.ambiguous_origin" {
            evidence(error, &effective_base, ours, theirs, options, ALIASES)
        } else {
            error
        }
    })?;
    let mut projected_ids = ids(&ours_view);
    projected_ids.extend(map.values().cloned());
    let local = ours_view.redirects.clone();
    let incoming_references = identity::alias_index(
        &theirs_aliases,
        &ids(&theirs_view),
        &theirs_view.redirects,
        &theirs_view.markdown,
    )?;
    let mut projected_redirects = local.clone();
    for (origin, target) in &theirs_view.redirects {
        let origin = map.get(origin).cloned().unwrap_or_else(|| origin.clone());
        let target = map.get(target).unwrap_or(target);
        let target = local.get(target).unwrap_or(target);
        if projected_redirects
            .insert(origin, target.clone())
            .is_some_and(|old| old != *target)
        {
            return Err(evidence(
                MergeError::content(
                    "merge.redirect_ambiguous",
                    "source and destination have incompatible audited identity redirects",
                ),
                &effective_base,
                ours,
                theirs,
                options,
                "trace/logic_mutations.yaml",
            ));
        }
    }
    let mut imported_aliases = Vec::with_capacity(theirs_aliases.len());
    for mut alias in theirs_aliases {
        let native = identity::normalize_local(&alias.target);
        let native = incoming_references
            .get(&alias.target)
            .or_else(|| incoming_references.get(&native))
            .unwrap_or(&native);
        alias.target = map.get(native).cloned().unwrap_or_else(|| native.clone());
        imported_aliases.push(alias);
    }
    let mut projected_aliases = ours_aliases.clone();
    projected_aliases.extend(imported_aliases.iter().cloned());
    projected_aliases.extend(imports.iter().map(|mapping| Alias {
        source_key: options.source_key.clone(),
        label: options.label.clone(),
        original: mapping.original.clone(),
        target: mapping.target.clone(),
        revision: revision.clone(),
    }));
    map.references = identity::alias_index(
        &projected_aliases,
        &projected_ids,
        &projected_redirects,
        &ours_view.markdown,
    )
    .map_err(|error| evidence(error, &effective_base, ours, theirs, options, ALIASES))?;
    map.local = local;
    let inherited = if peer_feedback {
        origin::inherited(
            &origins,
            ledger,
            incoming_ledger,
            &options.source_key,
            &theirs_view.entries,
            &ids(effective_view),
            &ids(&ours_view),
            &map,
            &base.root,
        )
    } else {
        origin::Inherited::default()
    };
    let mut report = MergeReport::new(options, revision.clone());
    report.imports = imports;
    report.renamed = report
        .imports
        .iter()
        .filter(|mapping| mapping.original != mapping.target)
        .cloned()
        .collect();
    let mut working = WorkingArtifact::new(ours.clone());
    ours_view.yaml.retain_preimages(&mut working);
    yaml::apply(
        &effective_view.yaml,
        &ours_view.yaml,
        &theirs_view.yaml,
        &map,
        &mut working,
        &mut report,
        &ledger.unresolved(),
        &inherited,
    )
    .map_err(|e| {
        evidence(
            e,
            &effective_base,
            ours,
            theirs,
            options,
            "trace/exploration_tree.yaml",
        )
    })?;
    markdown::apply(
        &effective_view.markdown,
        &ours_view.markdown,
        &theirs_view.markdown,
        &map,
        &mut working,
        &mut report,
        &inherited,
    )
    .map_err(|e| evidence(e, &effective_base, ours, theirs, options, "logic/claims.md"))?;
    let handled: BTreeSet<String> = effective_view
        .paths
        .union(&ours_view.paths)
        .cloned()
        .chain(theirs_view.paths.iter().cloned())
        .chain([LOG.into(), ALIASES.into()])
        .collect();
    if !replay {
        let all: BTreeSet<&str> = effective_base
            .files
            .keys()
            .chain(ours.files.keys())
            .chain(theirs.files.keys())
            .map(String::as_str)
            .filter(|path| !identity::private_path(path))
            .collect();
        for path in all {
            if handled.contains(path) {
                continue;
            }
            let b = bytes(&effective_base, path);
            let o = bytes(ours, path);
            let t = bytes(theirs, path);
            if t == b || t == o {
                continue;
            }
            let kind = if path.starts_with("src/") || path.starts_with("evidence/") {
                "external_read_only"
            } else {
                "opaque_file"
            };
            conflict(
                &mut report,
                path,
                path,
                "file",
                kind,
                b,
                o,
                t,
                ConflictLocator::Document,
            );
        }
    }
    // Foreign provenance is portable and complete, never interpreted as the current
    // transport source enrollment. Alias targets alone relocate to this checkout.
    let proposed = candidate(&working);
    let proposed_view = inventory_with_yaml(
        &proposed,
        false,
        yaml::inventory_cached(&proposed, &working)?,
    )?;
    let mut available = ids(&proposed_view);
    available.extend(
        working
            .paths()
            .into_iter()
            .filter(|path| path.starts_with("src/") || path.starts_with("evidence/")),
    );
    available.insert(LOG.into());
    available.insert(ALIASES.into());
    let redirects = &proposed_view.redirects;
    let mut aliases = ours_aliases.clone();
    let mut new_aliases = Vec::new();
    let signature = |item: &Alias| {
        (
            item.source_key.clone(),
            item.label.clone(),
            item.original.clone(),
            item.target.clone(),
        )
    };
    let mut known_aliases: BTreeSet<_> = aliases.iter().map(signature).collect();
    for imported in imported_aliases {
        // Incoming code/evidence is never installed; an inherited alias to an
        // external path this destination lacks stays only in transported bytes.
        let external =
            imported.target.starts_with("src/") || imported.target.starts_with("evidence/");
        if external && !available.contains(&imported.target) {
            continue;
        }
        if known_aliases.insert(signature(&imported)) {
            aliases.push(imported.clone());
            new_aliases.push(imported);
        }
    }
    for mapping in &report.imports {
        // Opaque/external incoming inventory is reported and retained as evidence,
        // not pretended to be an imported object when no destination exists.
        if !available.contains(&mapping.target) && !redirects.contains_key(&mapping.target) {
            continue;
        }
        let item = Alias {
            source_key: options.source_key.clone(),
            label: options.label.clone(),
            original: mapping.original.clone(),
            target: mapping.target.clone(),
            revision: revision.clone(),
        };
        if known_aliases.insert(signature(&item)) {
            aliases.push(item.clone());
            new_aliases.push(item);
        }
    }
    let previous_conflicts = ledger.unresolved();
    let mut new_conflicts = Vec::new();
    let previous_by_field: BTreeMap<_, _> = previous_conflicts
        .iter()
        .map(|old| {
            (
                (
                    old.source_key.as_str(),
                    old.path.as_str(),
                    old.selector.as_str(),
                    old.field.as_str(),
                ),
                old,
            )
        })
        .collect();
    if !replay {
        for mut item in std::mem::take(&mut report.conflicts) {
            if let Some(old) = previous_by_field.get(&(
                item.source_key.as_str(),
                item.path.as_str(),
                item.selector.as_str(),
                item.field.as_str(),
            )) {
                if old.theirs == item.theirs {
                    continue;
                }
                item.predecessor = Some(old.id.clone());
                item.id.clear();
                item.id = format!(
                    "MC{}",
                    identity::digest(&serde_json::to_vec(&item).expect("serializable conflict"))
                );
            }
            new_conflicts.push(item);
        }
    }
    report.conflicts = previous_conflicts;
    report.conflicts.extend(new_conflicts.iter().cloned());
    let mut new_records = Vec::new();
    if let Some(key) = &options.self_key
        && ledger.self_key().is_none()
    {
        new_records.push(Record::SelfIdentity {
            source_key: key.clone(),
            time: options.time.clone(),
        });
    }
    if !ledger.records.iter().any(|record|matches!(record,Record::Enrollment{source_key,..}if source_key==&options.source_key)){new_records.push(Record::Enrollment{source_key:options.source_key.clone(),label:options.label.clone(),time:options.time.clone()});}else if history.as_ref().is_none_or(|old|old.label!=options.label){new_records.push(Record::Label{source_key:options.source_key.clone(),label:options.label.clone(),time:options.time.clone()});}
    if !replay {
        for path in [LOG, ALIASES] {
            if let Some(raw) = bytes(theirs, path) {
                new_records.push(Record::Transport {
                    source_key: options.source_key.clone(),
                    revision: revision.clone(),
                    path: path.into(),
                    bytes: raw.to_vec(),
                });
            }
        }
        new_records.extend(
            origin::foreign_history(
                ledger,
                incoming_ledger,
                &map,
                &options.source_key,
                &revision,
                own_key.as_deref(),
            )
            .map_err(|error| evidence(error, base, ours, theirs, options, LOG))?,
        );
        let mut known_conflicts: BTreeSet<String> =
            report.conflicts.iter().map(|old| old.id.clone()).collect();
        for foreign in incoming_ledger.unresolved() {
            if known_conflicts.insert(foreign.id.clone()) {
                let mut imported = foreign;
                imported.kind = "imported_unresolved".into();
                imported.allowed.clear();
                imported.path = map.get(&imported.path).cloned().unwrap_or(imported.path);
                imported.selector = map
                    .get(&imported.selector)
                    .cloned()
                    .unwrap_or(imported.selector);
                new_conflicts.push(imported.clone());
                report.conflicts.push(imported);
            }
        }
        let source_files = theirs
            .files
            .iter()
            .filter(|(path, file)| {
                file.existed
                    && !identity::private_path(path)
                    && path.as_str() != LOG
                    && path.as_str() != ALIASES
            })
            .map(|(path, file)| (path.clone(), file.bytes.clone()))
            .collect();
        new_records.push(Record::Revision {
            source_key: options.source_key.clone(),
            fingerprint: revision.clone(),
            base: supplied_base,
            predecessor: history.as_ref().map(|old| old.fingerprint.clone()),
            time: options.time.clone(),
            git: options.git.clone(),
            files: source_files,
            mappings: report.imports.clone(),
        });
        new_records.extend(
            new_conflicts
                .into_iter()
                .map(|conflict| Record::Conflict { conflict }),
        );
        let resolved_ids: BTreeSet<&str> = ledger
            .records
            .iter()
            .filter_map(|record| match record {
                Record::Resolution { conflict_id, .. }
                | Record::ImportedResolution { conflict_id, .. } => Some(conflict_id.as_str()),
                _ => None,
            })
            .collect();
        let imported_open: BTreeSet<&str> = report
            .conflicts
            .iter()
            .filter(|item| item.kind == "imported_unresolved")
            .map(|item| item.id.as_str())
            .collect();
        for record in &incoming_ledger.records {
            if let Record::Resolution { conflict_id, .. } = record
                && imported_open.contains(conflict_id.as_str())
                && !resolved_ids.contains(conflict_id.as_str())
            {
                new_records.push(Record::ImportedResolution {
                    source_key: options.source_key.clone(),
                    revision: revision.clone(),
                    conflict_id: conflict_id.clone(),
                    evidence: serde_json::to_vec(record).expect("serializable original resolution"),
                });
            }
        }
    }
    identity::append(
        &mut working,
        ALIASES,
        "ara.aliases/v1",
        "aliases",
        &new_aliases,
    )?;
    identity::append(
        &mut working,
        LOG,
        "ara.merge-log/v1",
        "records",
        &new_records,
    )?;
    observer(MergePhase::PlanningFinished);
    let validation = working
        .validate()
        .map_err(MergeError::from)
        .map_err(|error| {
            evidence(
                error,
                &effective_base,
                ours,
                theirs,
                options,
                "trace/exploration_tree.yaml",
            )
        })?;
    report.conflicts = validate_candidate_view(&working, &proposed_view)
        .map_err(|error| {
            evidence(
                error,
                &effective_base,
                ours,
                theirs,
                options,
                "trace/exploration_tree.yaml",
            )
        })?
        .unresolved();
    observer(MergePhase::ValidationFinished);
    report.conflicts.sort_by(|a, b| a.id.cmp(&b.id));
    report.conflicts.dedup_by(|a, b| a.id == b.id);
    report.logic_conflicts = report
        .conflicts
        .iter()
        .filter(|item| item.path.starts_with("logic/") || item.path == "PAPER.md")
        .cloned()
        .collect();
    report.unresolved_count = report.conflicts.len();
    report.changed_paths = working.changed_paths();
    Ok(MergePlan {
        working,
        report,
        validation,
    })
}

pub fn resolve(snapshot: &ArtifactSnapshot, address: &str) -> Result<String, MergeError> {
    let view = inventory(snapshot)?;
    let identities = ids(&view);
    let aliases = identity::aliases(snapshot)?;
    let index = identity::alias_index(&aliases, &identities, &view.redirects, &view.markdown)?;
    identity::reject_ambiguous_display(address, &view.markdown, &view.mutations, &aliases)?;
    let normalized = identity::normalize_local(address);
    if let Some(target) = index.get(address) {
        return Ok(markdown::display_address(target));
    }
    if !identity::source_qualified(&aliases, address) {
        if let Some(target) = index.get(&normalized) {
            return Ok(markdown::display_address(target));
        }
        if identities.contains(&normalized) {
            return Ok(markdown::display_address(&normalized));
        }
        if let Some(Some(target)) = identity::heading_suffixes(&view.markdown).get(&normalized) {
            return Ok(markdown::display_address(target));
        }
    }
    Err(MergeError::content(
        "merge.unknown_identity",
        format!("unknown source-qualified or native address `{address}`"),
    ))
}
pub fn resolve_local(snapshot: &ArtifactSnapshot, address: &str) -> Result<String, MergeError> {
    resolve(snapshot, &identity::normalize_local(address))
}
/// Resolve an exact concept name/native reference against staged concepts and
/// authenticated rename archives, without copying or parsing the source tree.
pub fn concept_reference_exists(working: &WorkingArtifact, name: &str) -> Result<bool, MergeError> {
    use crate::write::{EntrySelector, logic, sessions};
    const CONCEPTS: &str = "logic/concepts.md";
    if !working.exists(CONCEPTS) {
        return Ok(false);
    }
    let view = markdown::inventory_document(CONCEPTS, working.text(CONCEPTS)?)?;
    if markdown::concept_address(&view, name)?.is_some() {
        return Ok(true);
    }
    let mut rows = Vec::new();
    if working.exists("trace/logic_mutations.yaml") {
        let mutations = working.yaml("trace/logic_mutations.yaml")?;
        let entries = mutations
            .root
            .get("mutations")?
            .ok_or_else(|| {
                MergeError::content("merge.mutation", "mutation archive requires mutations")
            })?
            .sequence()?;
        for row in entries {
            let Some(origin) = row
                .get("from")?
                .and_then(crate::write::positions::YamlNode::scalar)
            else {
                continue;
            };
            if origin.split_once(':').map(|(document, _)| document) != Some(CONCEPTS) {
                continue;
            }
            if row
                .get("action")?
                .and_then(crate::write::positions::YamlNode::scalar)
                != Some("rename")
            {
                continue;
            }
            let value = row.to_json()?;
            let from: EntrySelector =
                serde_json::from_value(value.get("from_selector").cloned().ok_or_else(|| {
                    MergeError::content(
                        "merge.mutation",
                        "concept rename needs a typed source selector",
                    )
                })?)
                .map_err(|cause| MergeError::content("merge.mutation", cause.to_string()))?;
            let to: EntrySelector =
                serde_json::from_value(value.get("to_selector").cloned().ok_or_else(|| {
                    MergeError::content(
                        "merge.mutation",
                        "concept rename needs a typed destination selector",
                    )
                })?)
                .map_err(|cause| MergeError::content("merge.mutation", cause.to_string()))?;
            if [&from,&to].iter().any(|selector|!matches!(selector,EntrySelector::Document{document,heading,entry:None}if document==CONCEPTS&&!heading.is_empty())){return Err(MergeError::content("merge.mutation","concept redirects must retain literal concept-heading selectors"));}
            let owner = value
                .get("session")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    MergeError::content(
                        "merge.mutation",
                        "concept rename needs its native owning session",
                    )
                })?;
            let turn = value
                .get("turn")
                .and_then(serde_json::Value::as_u64)
                .filter(|turn| *turn > 0)
                .ok_or_else(|| {
                    MergeError::content(
                        "merge.mutation",
                        "concept rename needs a positive owning turn",
                    )
                })?;
            sessions::require_turn(working, &format!("{owner}#{turn}"))?;
            let session = working.yaml(&format!("trace/sessions/{owner}.yaml"))?;
            let revisions = session
                .root
                .get("logic_revisions")?
                .ok_or_else(|| {
                    MergeError::content(
                        "merge.mutation",
                        "concept rename needs its exact owning revision",
                    )
                })?
                .sequence()?;
            let mut proven = false;
            for revision in revisions {
                let actual = revision.to_json()?;
                if actual.get("turn").and_then(serde_json::Value::as_u64) != Some(turn)
                    || actual.get("field").and_then(serde_json::Value::as_str) != Some("entry")
                {
                    continue;
                }
                if ["before", "after", "signal", "provenance"]
                    .iter()
                    .any(|field| {
                        value
                            .get(*field)
                            .and_then(serde_json::Value::as_str)
                            .is_none()
                            || value.get(*field) != actual.get(*field)
                    })
                {
                    continue;
                }
                let matches = match actual.get("entry") {
                    Some(serde_json::Value::String(entry)) => {
                        identity::normalize_local(entry) == identity::normalize_local(origin)
                    }
                    Some(entry) => serde_json::from_value::<EntrySelector>(entry.clone())
                        .is_ok_and(|selector| logic::selector_corresponds(&selector, &from)),
                    None => false,
                };
                if !matches {
                    continue;
                }
                logic::validate_revision_entry(working, owner, turn, &actual)?;
                proven = true;
                break;
            }
            if !proven {
                return Err(MergeError::content(
                    "merge.mutation",
                    "concept rename lacks its exact owning session revision",
                ));
            }
            rows.push(value);
        }
    }
    let identities: BTreeSet<String> = view
        .entries
        .iter()
        .map(|entry| entry.address.clone())
        .collect();
    let context = ArtifactSnapshot {
        root: working.base.root.clone(),
        files: BTreeMap::new(),
        identity_paths: BTreeSet::new(),
    };
    let redirects = identity::local_redirects(&context, &identities, &view, &rows)?;
    let normalized = identity::normalize_local(name);
    if normalized != name && !normalized.starts_with(&format!("{CONCEPTS}#")) {
        return Ok(false);
    }
    let aliases = if working.exists(ALIASES) {
        identity::aliases_bytes(working.bytes(ALIASES)?)?
    } else {
        Vec::new()
    };
    identity::validate_alias_names(&aliases)?;
    let address = if normalized.starts_with(&format!("{CONCEPTS}#"))
        || identity::source_qualified(&aliases, name)
    {
        normalized
    } else {
        format!("{CONCEPTS}#{name}")
    };
    let mut wanted = BTreeSet::from([address.clone()]);
    let mut retained = BTreeSet::new();
    loop {
        let mut advanced = false;
        for (index, alias) in aliases.iter().enumerate() {
            if retained.contains(&index) {
                continue;
            }
            if wanted.contains(&format!("{}:{}", alias.source_key, alias.original))
                || wanted.contains(&format!("{}:{}", alias.label, alias.original))
            {
                retained.insert(index);
                wanted.insert(alias.target.clone());
                wanted.insert(identity::normalize_local(&alias.target));
                advanced = true;
            }
        }
        if !advanced {
            break;
        }
    }
    let aliases: Vec<Alias> = aliases
        .into_iter()
        .enumerate()
        .filter_map(|(index, alias)| retained.contains(&index).then_some(alias))
        .collect();
    let index = identity::alias_index(&aliases, &identities, &redirects, &view)?;
    let Some(target) = index.get(&address) else {
        return Ok(false);
    };
    Ok(markdown::concept_address(&view, target)?.is_some())
}
pub fn resolve_selector(
    snapshot: &ArtifactSnapshot,
    selector: &crate::write::EntrySelector,
) -> Result<crate::write::EntrySelector, MergeError> {
    use crate::write::EntrySelector;
    fn matches_selector(archived: &EntrySelector, wanted: &EntrySelector) -> bool {
        if archived == wanted {
            return true;
        }
        match (archived, wanted) {
            (
                EntrySelector::Document {
                    document: a,
                    heading: before,
                    entry: None,
                },
                EntrySelector::Document {
                    document: b,
                    heading: wanted,
                    entry,
                },
            ) if a == b => {
                if let Some(entry) = entry {
                    let Some((last, parents)) = before.split_last() else {
                        return false;
                    };
                    (last == entry || last.split_once(':').is_some_and(|(id, _)| id == entry))
                        && parents.ends_with(wanted)
                } else {
                    !wanted.is_empty() && before.ends_with(wanted)
                }
            }
            _ => false,
        }
    }
    let view = inventory(snapshot)?;
    if let EntrySelector::Id { id } = selector {
        let identities = ids(&view);
        let aliases = identity::aliases(snapshot)?;
        let index = identity::alias_index(&aliases, &identities, &view.redirects, &view.markdown)?;
        identity::reject_ambiguous_display(id, &view.markdown, &view.mutations, &aliases)?;
        let normalized = identity::normalize_local(id);
        let address = index
            .get(id)
            .cloned()
            .or_else(|| {
                (!identity::source_qualified(&aliases, id))
                    .then(|| {
                        index
                            .get(&normalized)
                            .cloned()
                            .or_else(|| identities.contains(&normalized).then_some(normalized))
                    })
                    .flatten()
            })
            .ok_or_else(|| {
                MergeError::content(
                    "merge.unknown_identity",
                    "unknown native or source-qualified identity",
                )
            })?;
        return Ok(markdown::selector_for_address(&view.markdown, &address)?
            .unwrap_or(EntrySelector::Id { id: address }));
    }
    if let Some(address) = markdown::selector_address(&view.markdown, selector)? {
        return Ok(markdown::selector_for_address(&view.markdown, &address)?
            .unwrap_or_else(|| selector.clone()));
    }
    let mut current = selector.clone();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(serde_json::to_string(&current).expect("serializable selector")) {
            return Err(MergeError::content(
                "merge.alias_cycle",
                "archived selector redirect cycle",
            ));
        }
        let mut selected = None;
        for row in &view.mutations {
            let Some(from) = row.get("from_selector") else {
                continue;
            };
            let from = EntrySelector::deserialize(from)
                .map_err(|error| MergeError::content("merge.redirect_data", error.to_string()))?;
            if !matches_selector(&from, &current) {
                continue;
            }
            let to = row
                .get("to_selector")
                .filter(|value| !value.is_null())
                .ok_or_else(|| {
                    MergeError::content(
                        "merge.unknown_identity",
                        "archived selector was removed without a replacement",
                    )
                })?;
            let to = EntrySelector::deserialize(to)
                .map_err(|error| MergeError::content("merge.redirect_data", error.to_string()))?;
            if selected.as_ref().is_some_and(|previous| previous != &to) {
                return Err(MergeError::content(
                    "merge.redirect_ambiguous",
                    "archived selector suffix has multiple replacements",
                ));
            }
            selected = Some(to);
        }
        if let Some(next) = selected {
            current = next;
            if let Some(address) = markdown::selector_address(&view.markdown, &current)? {
                return Ok(
                    markdown::selector_for_address(&view.markdown, &address)?.unwrap_or(current)
                );
            }
            continue;
        }
        if let EntrySelector::Document {
            document,
            heading,
            entry,
        } = &current
        {
            let mut legacy = None;
            for row in &view.mutations {
                if row.get("from_selector").is_some() {
                    continue;
                }
                let Some((scope, name)) = row
                    .get("from")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|from| from.split_once(':'))
                else {
                    continue;
                };
                if scope != document {
                    continue;
                }
                let wanted = entry
                    .as_deref()
                    .or_else(|| heading.last().map(String::as_str));
                let mut matched = !name.contains('/') && wanted == Some(name);
                if let Some(before) = row.get("before").and_then(serde_json::Value::as_str) {
                    let archived = crate::markdown::headings(before);
                    let candidates: Vec<_> = archived
                        .iter()
                        .filter(|candidate| name.ends_with(&candidate.path.join("/")))
                        .collect();
                    if candidates.len() == 1 {
                        let path = &candidates[0].path;
                        matched |= entry.as_deref().is_none()
                            && !heading.is_empty()
                            && heading.len() >= path.len()
                            && heading[heading.len() - path.len()..]
                                .iter()
                                .zip(path)
                                .all(|(left, right)| left == right);
                    }
                }
                if !matched {
                    continue;
                }
                let key = identity::normalize_local(row["from"].as_str().expect("checked from"));
                let target = view.redirects.get(&key).ok_or_else(|| {
                    MergeError::content(
                        "merge.unknown_identity",
                        "archived selector has no current replacement",
                    )
                })?;
                if legacy.as_ref().is_some_and(|previous| previous != target) {
                    return Err(MergeError::content(
                        "merge.redirect_ambiguous",
                        "legacy archived suffix is ambiguous",
                    ));
                }
                legacy = Some(target.clone());
            }
            if let Some(address) = legacy {
                return markdown::selector_for_address(&view.markdown, &address)?.ok_or_else(
                    || {
                        MergeError::content(
                            "merge.unknown_identity",
                            "archived replacement is not a current document selector",
                        )
                    },
                );
            }
        }
        return Err(MergeError::content(
            "merge.unknown_identity",
            "unknown current or archived entry selector",
        ));
    }
}
fn validate_values(item: &MergeConflict) -> Result<(), MergeError> {
    for value in [&item.base, &item.ours, &item.theirs] {
        if value.fingerprint
            != identity::value_fingerprint(value.present.then_some(value.bytes.as_slice()))
            || (!value.present && !value.bytes.is_empty())
        {
            return Err(MergeError::content(
                "merge.conflict_evidence",
                "conflict value fingerprint or presence is invalid",
            ));
        }
    }
    Ok(())
}
fn selected<'a>(item: &'a MergeConflict, take: &str) -> Result<&'a MergeValue, MergeError> {
    if !item.allowed.iter().any(|allowed| allowed == take) {
        return Err(MergeError::content(
            "merge.resolution_not_allowed",
            "this conflict does not permit the requested mutable resolution",
        ));
    }
    match take {
        "ours" => Ok(&item.ours),
        "theirs" => Ok(&item.theirs),
        "base" => Ok(&item.base),
        _ => Err(MergeError::content(
            "merge.resolution_not_allowed",
            "expected ours, theirs, or base",
        )),
    }
}
// Keep the complete three-source/audit context explicit at this internal boundary.
#[allow(clippy::too_many_arguments)]
fn audit(
    working: &mut WorkingArtifact,
    item: &MergeConflict,
    value: &MergeValue,
    session: &str,
    turn: u64,
    signal: &str,
    provenance: &str,
    note: &str,
) -> Result<String, MergeError> {
    let path = format!("trace/sessions/{session}.yaml");
    let document = working.yaml(&path)?;
    let metadata = document.root.get("session")?.ok_or_else(|| {
        MergeError::content("merge.resolution_session", "missing session metadata")
    })?;
    let count = metadata
        .get("turn_count")?
        .and_then(|node| node.scalar())
        .and_then(|number| number.parse::<u64>().ok())
        .ok_or_else(|| {
            MergeError::content("merge.resolution_session", "session requires turn_count")
        })?;
    if count.checked_add(1) != Some(turn) {
        return Err(MergeError::content(
            "merge.resolution_session",
            "resolution must explicitly name the next session turn",
        ));
    }
    let time = metadata
        .get("last_turn")?
        .and_then(|node| node.scalar())
        .ok_or_else(|| {
            MergeError::content("merge.resolution_session", "session requires last_turn")
        })?
        .to_string();
    working
        .base
        .files
        .entry(crate::write::records::REASONING.into())
        .or_insert_with(|| FileSnapshot {
            bytes: vec![],
            existed: false,
            permissions: None,
            digest: crate::write::source::digest(&[]),
        });
    crate::write::sessions::plan(
        working,
        &WriteOperation::SessionLog {
            session: session.into(),
            timestamp: time.clone(),
            summary: None,
            events: vec![],
            ai_actions: vec![],
            claims_touched: vec![],
            logic_revisions: vec![],
            key_context: vec![],
            open_threads: None,
            ai_suggestions_pending: None,
        },
    )?;
    let record = serde_json::json!({"entry":item.selector,"field":item.field,"before":item.ours,"after":value,"signal":signal,"provenance":provenance,"note":note});
    crate::write::sessions::append_revision(working, session, turn, &record)?;
    Ok(time)
}
fn apply_resolution(
    working: &mut WorkingArtifact,
    item: &MergeConflict,
    value: &MergeValue,
) -> Result<(), MergeError> {
    match &item.locator {
        ConflictLocator::Yaml { .. } => yaml::resolve(working, item, value),
        ConflictLocator::Markdown { .. } => markdown::resolve(working, item, value),
        ConflictLocator::Document => {
            let current = if working.exists(&item.path) {
                Some(working.bytes(&item.path)?)
            } else {
                None
            };
            if identity::value_fingerprint(current) != item.ours.fingerprint {
                return Err(MergeError::content(
                    "merge.stale_conflict",
                    "current document fingerprint differs from captured ours",
                ));
            }
            if value == &item.ours {
                return Ok(());
            }
            if item.path == "PAPER.md" || !working.is_allowed_document(&item.path)? {
                return Err(MergeError::content(
                    "merge.resolution_not_allowed",
                    "opaque/external or bounded root content requires an approved field adapter, not arbitrary copying",
                ));
            }
            if value.present {
                let text = std::str::from_utf8(&value.bytes).map_err(|_| {
                    MergeError::content("merge.encoding", "selected mutable document is not UTF-8")
                })?;
                if working.exists(&item.path) {
                    working.replace_document(
                        &item.path,
                        text,
                        "explicit fingerprint-checked merge resolution",
                    )?;
                } else {
                    working.stage_create(&item.path, &value.bytes)?;
                }
            } else if working.exists(&item.path) {
                working.delete(&item.path, "explicit fingerprint-checked merge deletion")?;
            }
            Ok(())
        }
    }
}
fn relocated_choice(
    snapshot: &ArtifactSnapshot,
    ledger: &Ledger,
    item: &MergeConflict,
    take: &str,
) -> Result<MergeValue, MergeError> {
    let value = selected(item, take)?;
    if take == "ours" || !value.present {
        return Ok(value.clone());
    }
    let mut map = IdentityMap::new();
    for fact in ledger.records.iter().filter_map(Record::fact) {
        if fact.source_key == item.source_key {
            for mapping in fact.mappings {
                map.insert(mapping.original.clone(), mapping.target.clone());
            }
        }
    }
    let source_files = ledger
        .records
        .iter()
        .find_map(|record| match record {
            Record::Revision {
                source_key,
                fingerprint,
                files,
                ..
            } if source_key == &item.source_key && fingerprint == &item.source_revision => {
                Some(files)
            }
            _ => None,
        })
        .ok_or_else(|| {
            MergeError::content(
                "merge.corrupt_ledger",
                "conflict source revision has no frozen inventory",
            )
        })?;
    let source = captured(
        source_files,
        &snapshot.root,
        ledger,
        &item.source_key,
        &item.source_revision,
    );
    let source_markdown = markdown::inventory(&source)?;
    identity::reference_namespaces(&source_markdown.entries, &mut map)?;
    let live = live_inventory(snapshot)?;
    let current_ids = ids(&live);
    map.local = live.redirects;
    map.references = identity::alias_index(
        &identity::aliases(snapshot)?,
        &current_ids,
        &map.local,
        &live.markdown,
    )?;
    let options = MergeOptions {
        source_key: item.source_key.clone(),
        label: item.source_key.clone(),
        time: String::new(),
        git: None,
        predecessor: None,
        self_key: None,
    };
    let mut report = MergeReport::new(&options, item.source_revision.clone());
    let mut candidate = value.clone();
    if let Ok(text) = std::str::from_utf8(&value.bytes) {
        candidate.bytes = match &item.locator {
            ConflictLocator::Yaml { .. } => yaml::rewrite_choice(
                text,
                &item.path,
                &item.selector,
                &item.field,
                &map,
                &mut report,
            )?,
            ConflictLocator::Markdown { .. } => markdown::rewrite_choice(
                text,
                &item.path,
                &item.selector,
                &item.field,
                &map,
                &mut report,
            )?,
            ConflictLocator::Document => markdown::rewrite_choice(
                text,
                &item.path,
                &item.selector,
                "$document",
                &map,
                &mut report,
            )?,
        }
        .into_bytes();
    }
    candidate.fingerprint = identity::value_fingerprint(Some(&candidate.bytes));
    Ok(candidate)
}

/// Authenticate the exceptional native locator in a locally authored merge
/// audit by joining its immutable decision and complete captured candidates.
pub fn authenticates_resolution_audit(
    working: &WorkingArtifact,
    session: &str,
    turn: u64,
    audit: &serde_json::Value,
) -> Result<bool, MergeError> {
    if !["before", "after"].iter().all(|key| {
        audit.get(*key).is_some_and(|value| {
            value
                .get("fingerprint")
                .is_some_and(serde_json::Value::is_string)
                && value.get("bytes").is_some_and(serde_json::Value::is_array)
                && value
                    .get("present")
                    .is_some_and(serde_json::Value::is_boolean)
        })
    }) {
        return Ok(false);
    }
    if !working.exists(LOG) {
        return Ok(false);
    }
    let ledger = identity::load_bytes(working.bytes(LOG)?)?;
    let mut snapshot = None;
    for record in &ledger.records {
        let (item, after, signal, provenance, note) = match record {
            Record::Resolution {
                conflict_id,
                take,
                session: owner,
                turn: at,
                signal,
                provenance,
                applied_fingerprint,
                selected_fingerprint,
                ..
            } if owner == session && *at == turn => {
                let Some(item) = ledger.records.iter().find_map(|record| {
                    if let Record::Conflict { conflict } = record {
                        (conflict.id == *conflict_id).then_some(conflict)
                    } else {
                        None
                    }
                }) else {
                    return Ok(false);
                };
                let after = relocated_choice(
                    snapshot.get_or_insert_with(|| candidate(working)),
                    &ledger,
                    item,
                    take,
                )?;
                if after.fingerprint != *applied_fingerprint {
                    return Ok(false);
                }
                (
                    item,
                    after,
                    signal,
                    provenance,
                    format!(
                        "explicit resolution {} take {take}; selected source fingerprint {selected_fingerprint}",
                        item.id
                    ),
                )
            }
            Record::ProtectedDecision {
                conflict,
                decision,
                reason,
                session: owner,
                turn: at,
                signal,
                provenance,
                ..
            } if owner == session && *at == turn => {
                let after = if decision == "restore_base" {
                    conflict.base.clone()
                } else {
                    conflict.ours.clone()
                };
                (conflict, after, signal, provenance, reason.clone())
            }
            _ => continue,
        };
        validate_values(item)?;
        if audit
            == &serde_json::json!({"entry":item.selector,"field":item.field,"before":item.ours,"after":after,"signal":signal,"provenance":provenance,"note":note})
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn plan_resolution(
    snapshot: &ArtifactSnapshot,
    conflict_id: &str,
    take: &str,
    session: &str,
    turn: u64,
    signal: &str,
    provenance: &str,
) -> Result<WorkingArtifact, MergeError> {
    let ledger = identity::load(snapshot)?;
    let item = ledger
        .unresolved()
        .into_iter()
        .find(|item| item.id == conflict_id)
        .ok_or_else(|| {
            MergeError::content(
                "merge.unknown_conflict",
                "unknown or already resolved conflict",
            )
        })?;
    validate_values(&item)?;
    let value = selected(&item, take)?.clone();
    let candidate_value = relocated_choice(snapshot, &ledger, &item, take)?;
    let mut working = WorkingArtifact::new(snapshot.clone());
    apply_resolution(&mut working, &item, &candidate_value)?;
    let time = audit(
        &mut working,
        &item,
        &candidate_value,
        session,
        turn,
        signal,
        provenance,
        &format!(
            "explicit resolution {} take {take}; selected source fingerprint {}",
            item.id, value.fingerprint
        ),
    )?;
    identity::append(
        &mut working,
        LOG,
        "ara.merge-log/v1",
        "records",
        &[Record::Resolution {
            conflict_id: item.id,
            take: take.into(),
            time,
            session: session.into(),
            turn,
            signal: signal.into(),
            provenance: provenance.into(),
            prior_fingerprint: item.ours.fingerprint,
            selected_fingerprint: value.fingerprint,
            applied_fingerprint: candidate_value.fingerprint,
        }],
    )?;
    validate_candidate(&working)?;
    Ok(working)
}
#[allow(clippy::too_many_arguments)]
pub fn plan_protected_resolution(
    snapshot: &ArtifactSnapshot,
    item: &MergeConflict,
    decision: &str,
    expected_current: &str,
    session: &str,
    turn: u64,
    signal: &str,
    provenance: &str,
    reason: &str,
) -> Result<WorkingArtifact, MergeError> {
    validate_values(item)?;
    if !item.kind.starts_with("protected")
        || !matches!(decision, "reject_incoming" | "restore_base")
        || reason.trim().is_empty()
        || expected_current != item.ours.fingerprint
    {
        return Err(MergeError::content(
            "merge.protected_resolution",
            "protected repair requires exact captured current fingerprint, explicit reason, and reject_incoming or restore_base",
        ));
    }
    let mut working = WorkingArtifact::new(snapshot.clone());
    let chosen = if decision == "restore_base" {
        &item.base
    } else {
        &item.ours
    };
    // Protected spans, including complete immutable entries, are located by the
    // dedicated corrective adapter, never by an ordinary mutable setter.
    match &item.locator {
        ConflictLocator::Yaml { .. } | ConflictLocator::Document => {
            if decision == "restore_base" {
                yaml::restore_base(&mut working, item)?;
            } else {
                yaml::verify_current(&working, item)?;
            }
        }
        _ => {
            return Err(MergeError::content(
                "merge.protected_resolution",
                "captured protected content has no approved corrective adapter",
            ));
        }
    }
    audit(
        &mut working,
        item,
        chosen,
        session,
        turn,
        signal,
        provenance,
        reason,
    )?;
    identity::append(
        &mut working,
        LOG,
        "ara.merge-log/v1",
        "records",
        &[Record::ProtectedDecision {
            conflict: item.clone(),
            decision: decision.into(),
            reason: reason.into(),
            expected_current: expected_current.into(),
            session: session.into(),
            turn,
            signal: signal.into(),
            provenance: provenance.into(),
        }],
    )?;
    validate_candidate(&working)?;
    Ok(working)
}

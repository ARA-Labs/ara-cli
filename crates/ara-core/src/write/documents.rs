//! Bounded knowledge documents, root metadata, and explicit seed profiles.
use super::{
    EntrySelector, Fields, OperationResult, RevisionContext, WorkingArtifact, WriteError,
    WriteOperation, fields, logic,
    source::{self, PendingRevision, YamlDocument},
};
use crate::markdown;
use serde_json::{Value, json};
use std::{collections::BTreeMap, ops::Range};

pub fn allowed(path: &str) -> bool {
    if path.contains('\\')
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return false;
    }
    matches!(
        path,
        "logic/problem.md"
            | "logic/claims.md"
            | "logic/concepts.md"
            | "logic/experiments.md"
            | "logic/related_work.md"
    ) || path
        .strip_prefix("logic/solution/")
        .is_some_and(|name| !name.contains('/') && name.ends_with(".md") && name.len() > 3)
}

pub fn create(
    working: &mut WorkingArtifact,
    document: &str,
    content: &str,
) -> Result<OperationResult, WriteError> {
    if document == "PAPER.md" || !working.is_allowed_document(document)? {
        return Err(WriteError::semantic(
            "write.document",
            "Generic creation is restricted to allowlisted knowledge documents",
        ));
    }
    working.create(document, content)?;
    let mut result = OperationResult::new("document.create", None);
    result.target = Some(document.into());
    Ok(result)
}

pub fn append_entry(
    working: &mut WorkingArtifact,
    document: &str,
    title: &str,
    input: &Fields,
    content: Option<&str>,
) -> Result<String, WriteError> {
    if document == "PAPER.md"
        || !working.is_allowed_document(document)?
        || title.trim().is_empty()
        || title.contains(['\r', '\n'])
    {
        return Err(WriteError::semantic(
            "write.entry",
            "Entry needs an allowed document and a single-line title",
        ));
    }
    let fields = fields::validate(fields::kind(document)?, input, false, false)?;
    if working.exists(document)
        && working
            .headings(document)?
            .iter()
            .any(|h| h.heading == title)
    {
        return Err(WriteError::semantic(
            "write.entry_duplicate",
            "Entry heading already exists",
        ));
    }
    let mut block = format!("## {title}\n");
    for (label, value) in fields {
        block.push_str(&fields::render(&label, &value));
    }
    if let Some(content) = content {
        if markdown::headings(content).iter().any(|h| h.level <= 2) {
            return Err(WriteError::semantic(
                "write.entry_content",
                "Entry content cannot escape its selected heading",
            ));
        }
        block.push_str(content);
    }
    if working.exists(document) {
        logic::append(working, document, &block, "entry.append")?;
    } else {
        working.create(document, &block)?;
    }
    Ok(format!("{document}:{title}"))
}

pub fn body_range(text: &str, heading: &[String]) -> Result<Range<usize>, WriteError> {
    if heading.is_empty() {
        return Ok(0..text.len());
    }
    let requested: Vec<_> = heading.iter().map(String::as_str).collect();
    let headings: Vec<_> = markdown::headings(text)
        .into_iter()
        .filter(|h| h.path.ends_with(&requested))
        .collect();
    if headings.len() != 1 {
        return Err(WriteError::semantic(
            "write.selector",
            format!("Expected one heading body; found {}", headings.len()),
        ));
    }
    Ok(headings[0].body_range.clone())
}

fn replace(
    working: &mut WorkingArtifact,
    document: &str,
    heading: &[String],
    expected: &str,
    content: &str,
) -> Result<OperationResult, WriteError> {
    if document == "PAPER.md" || !working.is_allowed_document(document)? {
        return Err(WriteError::semantic(
            "write.document",
            "Replacement is restricted to mutable knowledge documents",
        ));
    }
    let text = working.text(document)?;
    let target = if heading.is_empty() {
        None
    } else {
        Some(logic::resolve(
            working,
            &EntrySelector::Document {
                document: document.into(),
                heading: heading.to_vec(),
                entry: None,
            },
        )?)
    };
    let range = target
        .as_ref()
        .map_or(0..text.len(), |entry| entry.body.clone());
    if source::digest(text[range.clone()].as_bytes()) != expected {
        return Err(WriteError::semantic(
            "write.digest_conflict",
            "Selected source digest does not match",
        ));
    }
    logic::preserve_canonical_ids(working, document, &range, content)?;
    if let Some(target) = target
        && markdown::headings(content)
            .iter()
            .any(|h| h.level <= target.level)
    {
        return Err(WriteError::semantic(
            "write.heading_escape",
            "Replacement cannot create a peer or ancestor heading",
        ));
    }
    let no_op = text[range.clone()] == *content;
    if !no_op {
        working.edit(document, range, content, "document.replace")?;
    }
    let mut result = OperationResult::new("document.replace", None);
    result.target = Some(document.into());
    result.no_op = no_op;
    Ok(result)
}

fn frontmatter_range(text: &str) -> Result<Option<Range<usize>>, WriteError> {
    let mut offset = 0;
    let mut open = None;
    for line in text.split_inclusive('\n') {
        let t = line.trim_end_matches(['\r', '\n']);
        if let Some(start) = open {
            if t == "---" {
                return Ok(Some(start..offset));
            }
        } else {
            if t.trim_start_matches('\u{feff}').trim().is_empty() {
                offset += line.len();
                continue;
            }
            if t.trim_start_matches('\u{feff}') != "---" {
                return Ok(None);
            }
            open = Some(offset + line.len());
        }
        offset += line.len();
    }
    if open.is_some() {
        Err(WriteError::semantic(
            "write.frontmatter",
            "Unclosed PAPER frontmatter",
        ))
    } else {
        Ok(None)
    }
}

fn validate_paper_fields(fields: &Fields) -> Result<(), WriteError> {
    for (key, value) in fields {
        let list = value
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string));
        let valid = match key.as_str() {
            "title" | "venue" | "doi" | "abstract" | "domain" | "ara_version" => value.is_string(),
            "authors" | "keywords" | "claims_summary" | "knowledge_paths" => list,
            "year" => value.is_i64() || value.is_u64(),
            _ => {
                return Err(WriteError::semantic(
                    "write.paper_field",
                    format!("Unknown PAPER field {key}"),
                )
                .at(key));
            }
        };
        if !valid {
            return Err(WriteError::semantic(
                "write.paper_type",
                format!("Invalid type for PAPER {key}"),
            )
            .at(key));
        }
    }
    Ok(())
}

fn paper(
    working: &mut WorkingArtifact,
    fields: &Fields,
    heading: &[String],
    expected: Option<&str>,
    content: Option<&str>,
    audit: Option<&RevisionContext>,
) -> Result<OperationResult, WriteError> {
    validate_paper_fields(fields)?;
    if let Some(audit) = audit {
        logic::revision_context(&audit.session, audit.turn, &audit.signal, &audit.provenance)?;
    }
    if content.is_none() && (!heading.is_empty() || expected.is_some()) {
        return Err(WriteError::semantic(
            "write.paper_body",
            "Body selector/digest requires content",
        ));
    }
    let before = working.text("PAPER.md")?.to_owned();
    if let Some(content) = content {
        let expected = expected.ok_or_else(|| {
            WriteError::semantic(
                "write.digest_required",
                "PAPER body replacement requires expected digest",
            )
        })?;
        let text = working.text("PAPER.md")?;
        let start = frontmatter_range(text)?.map_or(0, |r| {
            r.end
                + text[r.end..]
                    .split_inclusive('\n')
                    .next()
                    .map_or(0, str::len)
        });
        let headings = working.headings("PAPER.md")?;
        let selected = if heading.is_empty() {
            None
        } else {
            let mut found = headings
                .iter()
                .filter(|h| h.range.start >= start && h.path.ends_with(heading));
            let selected = found.next().ok_or_else(|| {
                WriteError::semantic("write.selector", "PAPER heading body not found")
            })?;
            if found.next().is_some() {
                return Err(WriteError::semantic(
                    "write.selector",
                    "PAPER heading body is ambiguous",
                ));
            }
            Some(selected)
        };
        let range = selected.map_or(start..text.len(), |h| h.body_range.clone());
        if source::digest(text[range.clone()].as_bytes()) != expected {
            return Err(WriteError::semantic(
                "write.digest_conflict",
                "PAPER body digest does not match",
            ));
        }
        if range.start == 0 && frontmatter_range(content)?.is_some() {
            return Err(WriteError::semantic(
                "write.paper_body",
                "Body cannot author metadata; use frontmatter fields",
            ));
        }
        if let Some(selected) = selected {
            let level = selected.level;
            if markdown::headings(content).iter().any(|h| h.level <= level) {
                return Err(WriteError::semantic(
                    "write.heading_escape",
                    "PAPER body replacement cannot escape its selected heading",
                ));
            }
        }
        if text[range.clone()] != *content {
            working.edit("PAPER.md", range, content, "paper.body")?;
        }
    }
    if !fields.is_empty() {
        let text = working.text("PAPER.md")?;
        if let Some(range) = frontmatter_range(text)? {
            let yaml = YamlDocument::parse(&text[range.clone()])?;
            let entries = yaml.root.mapping()?;
            let mut edits = Vec::new();
            let mut append = String::new();
            for (key, value) in fields {
                let matches: Vec<_> = entries
                    .iter()
                    .enumerate()
                    .filter(|(_, (k, _))| k.scalar() == Some(key.as_str()))
                    .collect();
                if matches.len() > 1 {
                    return Err(WriteError::semantic(
                        "write.paper_duplicate",
                        format!("Duplicate PAPER field {key}"),
                    ));
                }
                let rendered = format!(
                    "{key}: {}\n",
                    serde_json::to_string(value).map_err(|e| WriteError::io(e.to_string()))?
                );
                if let Some((index, (k, v))) = matches.first() {
                    if v.to_json()? != *value {
                        k.editable()?;
                        if yaml.root.flow {
                            return Err(WriteError::semantic(
                                "write.unsupported_source",
                                "PAPER flow frontmatter cannot be safely edited",
                            ));
                        }
                        let start = range.start + k.start;
                        let end = entries.get(index + 1).map_or(range.end, |(next, _)| {
                            range.start
                                + super::positions::line_start(&text[range.clone()], next.start)
                        });
                        let fragment = &text[start..end];
                        let mut span_end = end;
                        for line in fragment
                            .split_inclusive('\n')
                            .collect::<Vec<_>>()
                            .iter()
                            .rev()
                        {
                            if line.trim().is_empty() || line.starts_with('#') {
                                span_end -= line.len();
                            } else {
                                break;
                            }
                        }
                        edits.push((start..span_end, rendered));
                    }
                } else {
                    append.push_str(&rendered);
                }
            }
            if !append.is_empty() {
                let prefix = if range.end > range.start && !text[..range.end].ends_with('\n') {
                    "\n"
                } else {
                    ""
                };
                edits.push((range.end..range.end, format!("{prefix}{append}")));
            }
            edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
            for (range, content) in edits {
                working.edit("PAPER.md", range, &content, "paper.frontmatter")?;
            }
            let final_text = working.text("PAPER.md")?;
            let final_range = frontmatter_range(final_text)?.ok_or_else(|| {
                WriteError::semantic("write.frontmatter", "Frontmatter lost during edit")
            })?;
            let after = YamlDocument::parse(&final_text[final_range])?;
            let untouched_before: Vec<_> = entries
                .iter()
                .filter(|(k, _)| !k.scalar().is_some_and(|key| fields.contains_key(key)))
                .map(|(k, v)| (k.semantic(), v.semantic()))
                .collect();
            let untouched_after: Vec<_> = after
                .root
                .mapping()?
                .iter()
                .filter(|(k, _)| !k.scalar().is_some_and(|key| fields.contains_key(key)))
                .map(|(k, v)| (k.semantic(), v.semantic()))
                .collect();
            if untouched_before != untouched_after {
                return Err(WriteError::semantic(
                    "write.intent",
                    "PAPER edit changed untargeted metadata",
                ));
            }
            for (key, value) in fields {
                if after
                    .root
                    .get(key)?
                    .ok_or_else(|| WriteError::semantic("write.intent", "PAPER field missing"))?
                    .to_json()?
                    != *value
                {
                    return Err(WriteError::semantic(
                        "write.intent",
                        "PAPER field did not round-trip",
                    ));
                }
            }
        } else {
            let mut header = String::from("---\n");
            for (key, value) in fields {
                header.push_str(&format!(
                    "{key}: {}\n",
                    serde_json::to_string(value).map_err(|e| WriteError::io(e.to_string()))?
                ));
            }
            header.push_str("---\n");
            working.edit("PAPER.md", 0..0, &header, "paper.frontmatter")?;
        }
    }
    crate::knowledge_paths(working.text("PAPER.md")?)
        .map_err(|e| WriteError::semantic("write.knowledge_paths", e))?;
    let no_op = before == working.text("PAPER.md")?;
    let mut result = OperationResult::new("paper.edit", None);
    result.target = Some("PAPER.md".into());
    result.no_op = no_op;
    if !no_op && let Some(audit) = audit {
        let after = working.text("PAPER.md")?;
        let record = json!({"entry":"PAPER.md","field":"document","before":before,"after":after,"signal":audit.signal,"provenance":audit.provenance,"note":audit.note});
        working.revisions.push(PendingRevision {
            session: audit.session.clone(),
            turn: audit.turn,
            record,
        });
        result.turn = Some(audit.turn);
    }
    Ok(result)
}

fn init(
    working: &mut WorkingArtifact,
    profile: &str,
    paper: Option<&str>,
    documents: &BTreeMap<String, String>,
    missing_only: bool,
) -> Result<OperationResult, WriteError> {
    if !missing_only {
        if !working.paths().is_empty() {
            return Err(WriteError::semantic(
                "write.init_nonempty",
                "Initialization requires an empty artifact root",
            ));
        }
        match std::fs::read_dir(&working.base.root) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|e| WriteError::io(e.to_string()))?;
                    if entry.file_name() != ".ara" {
                        return Err(WriteError::semantic(
                            "write.init_nonempty",
                            "Initialization requires a new or empty root, not just absent knowledge files",
                        ));
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(WriteError::io(error.to_string())),
        }
    }
    let mut seeds = BTreeMap::new();
    match profile {
        "research-manager" => {
            if !documents.is_empty() {
                return Err(WriteError::semantic(
                    "write.init_documents",
                    "research-manager takes only caller PAPER",
                ));
            }
            let paper = paper.ok_or_else(|| {
                WriteError::semantic("write.init_paper", "Caller PAPER content is required")
            })?;
            crate::knowledge_paths(paper)
                .map_err(|e| WriteError::semantic("write.knowledge_paths", e))?;
            seeds.insert("PAPER.md".to_owned(), paper.to_owned());
            for (path, text) in [
                ("trace/exploration_tree.yaml", "tree: []\n"),
                (super::sessions::INDEX, "sessions: []\n"),
                ("trace/pm_reasoning_log.yaml", "entries: []\n"),
                ("staging/observations.yaml", "observations: []\n"),
                ("logic/claims.md", "# Claims\n"),
                ("logic/problem.md", "# Problem\n"),
                ("logic/solution/heuristics.md", "# Heuristics\n"),
                ("evidence/README.md", "# Evidence Index\n"),
            ] {
                seeds.insert(path.into(), text.into());
            }
        }
        "compiler" => {
            if paper.is_some() {
                return Err(WriteError::semantic(
                    "write.init_paper",
                    "compiler supplies PAPER in documents",
                ));
            }
            for mandatory in [
                "PAPER.md",
                "logic/problem.md",
                "logic/claims.md",
                "logic/concepts.md",
                "logic/experiments.md",
                "logic/related_work.md",
                "logic/solution/constraints.md",
            ] {
                if !documents.contains_key(mandatory) {
                    return Err(WriteError::semantic(
                        "write.init_required",
                        format!("Missing caller document {mandatory}"),
                    )
                    .at(mandatory));
                }
            }
            let registered = crate::knowledge_paths(documents.get("PAPER.md").unwrap())
                .map_err(|e| WriteError::semantic("write.knowledge_paths", e))?;
            for (path, text) in documents {
                if path != "PAPER.md" && !allowed(path) && !registered.contains(path) {
                    return Err(WriteError::semantic(
                        "write.init_document",
                        format!("Not an initialization document: {path}"),
                    ));
                }
                seeds.insert(path.clone(), text.clone());
            }
            seeds.insert("trace/exploration_tree.yaml".into(), "tree: []\n".into());
        }
        _ => {
            return Err(WriteError::semantic(
                "write.init_profile",
                "Unknown initialization profile",
            ));
        }
    }
    for path in seeds.keys() {
        if !working.base.files.contains_key(path) {
            working.base.capture(path)?;
        }
    }
    for (path, content) in &seeds {
        if working.exists(path) {
            let caller_supplied = path == "PAPER.md" || documents.contains_key(path);
            if !missing_only || (caller_supplied && working.text(path)? != content) {
                return Err(WriteError::semantic(
                    "write.init_conflict",
                    format!("Existing caller document {path} does not match exactly"),
                ));
            }
        }
    }
    for &directory in source::INIT_DIRECTORIES {
        match std::fs::symlink_metadata(working.base.root.join(directory)) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(WriteError::semantic(
                    "write.init_directory",
                    format!("Initialization directory {directory} must be a real directory"),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                working.created_dirs.insert(directory.into());
            }
            Err(error) => return Err(WriteError::io(error.to_string())),
        }
    }
    let mut no_op = true;
    for (path, content) in seeds {
        if !working.exists(&path) {
            working.create(&path, &content)?;
            no_op = false;
        }
    }
    let mut result = OperationResult::new("artifact.init", None);
    result.target = Some(profile.into());
    result.no_op = no_op && working.created_dirs.is_empty();
    Ok(result)
}

pub fn plan(
    working: &mut WorkingArtifact,
    operation: &WriteOperation,
) -> Result<OperationResult, WriteError> {
    match operation {
        WriteOperation::DocumentCreate { document, content } => create(working, document, content),
        WriteOperation::DocumentReplace {
            document,
            heading,
            expected,
            content,
        } => replace(working, document, heading, expected, content),
        WriteOperation::PaperEdit {
            frontmatter,
            heading,
            expected,
            content,
            audit,
        } => paper(
            working,
            frontmatter,
            heading,
            expected.as_deref(),
            content.as_deref(),
            audit.as_ref(),
        ),
        WriteOperation::ArtifactInit {
            profile,
            paper,
            documents,
            missing_only,
        } => init(working, profile, paper.as_deref(), documents, *missing_only),
        _ => Err(WriteError::semantic(
            "write.operation",
            "Not a document operation",
        )),
    }
}

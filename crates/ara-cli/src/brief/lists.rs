//! Address-led line output for `find`, `ls`, `status`, `path`, `refs` and
//! `open`.
use crate::agent::address;
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::{Result, Write};

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
/// The address a caller passes back to `show`: a native ID, a heading
/// address, or a document path.
fn row_address(row: &Value) -> String {
    if let Some(address) = row["address"].as_str() {
        return address.to_owned();
    }
    match (row["id"].as_str(), row["key"].as_str()) {
        (Some(id), _) => id.to_owned(),
        (None, Some(key)) if matches!(text(&row["kind"]), "source_document" | "solution") => {
            address::document(key)
        }
        (None, key) => key.unwrap_or("").to_owned(),
    }
}
fn title(row: &Value) -> String {
    [
        "title",
        "term",
        "cite",
        "name",
        "statement",
        "summary",
        "content",
    ]
    .iter()
    .find_map(|key| row[*key].as_str())
    .map_or_else(String::new, crate::output::excerpt)
}
fn rows<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value[key].as_array().into_iter().flatten()
}
/// Print `note` when `value[key]` is empty, so an empty result is visible.
/// Returns whether it was empty.
fn none(value: &Value, key: &str, note: &str, out: &mut impl Write) -> Result<bool> {
    let empty = rows(value, key).next().is_none();
    if empty {
        writeln!(out, "{note}")?;
    }
    Ok(empty)
}

pub fn find(value: &Value, out: &mut impl Write) -> Result<()> {
    if none(value, "results", "no results", out)? {
        return Ok(());
    }
    for result in rows(value, "results") {
        writeln!(
            out,
            "{} [{}] {}",
            row_address(result),
            text(&result["kind"]),
            text(&result["source"])
        )?;
        let matched: BTreeSet<u64> = rows(result, "matches")
            .filter_map(|hit| hit["line"].as_u64())
            .collect();
        if matched.is_empty() {
            writeln!(out, "  excerpt: {}", text(&result["excerpt"]))?;
            continue;
        }
        if let Some(blocks) = result["context"].as_array() {
            for (index, block) in blocks.iter().enumerate() {
                if index > 0 {
                    writeln!(out, "  --")?;
                }
                let start = block["start"].as_u64().unwrap_or(1);
                for (offset, line) in rows(block, "lines").enumerate() {
                    let number = start + offset as u64;
                    let mark = if matched.contains(&number) { ':' } else { '-' };
                    writeln!(out, "  {number}{mark} {}", text(line))?;
                }
            }
        } else {
            for hit in rows(result, "matches") {
                writeln!(out, "  {}: {}", hit["line"], text(&hit["text"]))?;
            }
        }
        let total = result["match_count"].as_u64().unwrap_or(0);
        if total > matched.len() as u64 {
            writeln!(
                out,
                "  … {} more matching lines",
                total - matched.len() as u64
            )?;
        }
    }
    Ok(())
}

pub fn ls(value: &Value, out: &mut impl Write) -> Result<()> {
    if let Some(documents) = value["documents"].as_array() {
        for document in documents {
            let counts = document["counts"]
                .as_object()
                .filter(|counts| !counts.is_empty())
                .map(|counts| {
                    counts
                        .iter()
                        .map(|(kind, count)| format!("{kind}={count}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                });
            let kind = match (counts, document["headings"].as_u64()) {
                (Some(counts), _) => counts,
                (None, Some(headings)) => format!("document headings={headings}"),
                (None, None) => "document".into(),
            };
            writeln!(
                out,
                "{}\t{kind}\tlines={}",
                text(&document["address"]),
                document["lines"]
            )?;
        }
        let roots: Vec<&str> = rows(value, "file_access").map(text).collect();
        writeln!(
            out,
            "direct files: {} are not ara documents; read and search them with your file tools",
            roots.join(" ")
        )?;
        return Ok(());
    }
    if none(value, "entries", "no entries", out)? {
        return Ok(());
    }
    for row in rows(value, "entries") {
        writeln!(
            out,
            "{}\t{}\t{}",
            row_address(row),
            text(&row["kind"]),
            title(row)
        )?;
    }
    Ok(())
}

pub fn open(value: &Value, out: &mut impl Write) -> Result<()> {
    if none(value, "items", "no open items", out)? {
        return Ok(());
    }
    for row in rows(value, "items") {
        let reasons: Vec<&str> = rows(row, "reasons").map(text).collect();
        write!(
            out,
            "{}\t{}\t{}\t{}",
            row_address(row),
            text(&row["kind"]),
            reasons.join(","),
            title(row)
        )?;
        // Observation rows end with their measured inactivity; an unknown
        // count prints `unknown`, never zero.
        if let Some(status) = row["history_status"].as_str() {
            let count = |key: &str| {
                row[key]
                    .as_u64()
                    .map_or_else(|| "unknown".to_owned(), |n| n.to_string())
            };
            write!(
                out,
                "\tturns={} days={} last_reference={} history={status}",
                count("turns_since_reference"),
                count("session_days_since_reference"),
                row["last_reference_turn"]
                    .as_str()
                    .or_else(|| row["last_reference_date"].as_str())
                    .unwrap_or("unknown"),
            )?;
        }
        writeln!(out)?;
    }
    Ok(())
}

pub fn path(value: &Value, out: &mut impl Write) -> Result<()> {
    for (depth, step) in rows(value, "steps").enumerate() {
        writeln!(
            out,
            "{}{}\t{}\t{}",
            "  ".repeat(depth),
            row_address(step),
            text(&step["kind"]),
            title(step)
        )?;
    }
    Ok(())
}

pub fn refs(value: &Value, out: &mut impl Write) -> Result<()> {
    let location = |row: &Value| match row["line"].as_u64() {
        Some(line) => format!("{}:{line}", text(&row["source"])),
        None => text(&row["source"]).to_owned(),
    };
    let target = value["display"]["target"]
        .as_str()
        .unwrap_or_else(|| text(&value["target"]));
    writeln!(out, "target: {target}")?;
    for row in rows(value, "structured") {
        writeln!(
            out,
            "{}\t{}\t{}\t{}",
            row["address"].as_str().unwrap_or_else(|| text(&row["id"])),
            text(&row["field"]),
            location(row),
            text(&row["literal"])
        )?;
    }
    for row in rows(value, "prose") {
        writeln!(
            out,
            "{}\tpossible mention\t{}\t{}",
            location(row),
            text(&row["literal"]),
            text(&row["context"])
        )?;
    }
    Ok(())
}

pub fn status(value: &Value, out: &mut impl Write) -> Result<()> {
    let complete = value["complete"] == true;
    writeln!(out, "complete\t{}", if complete { "yes" } else { "no" })?;
    let pairs = |object: &Value| {
        object.as_object().map(|object| {
            object
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{key}={}",
                        value
                            .as_str()
                            .map_or_else(|| value.to_string(), str::to_owned)
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
    };
    match pairs(&value["counts"]) {
        Some(counts) => writeln!(out, "counts\t{counts}")?,
        None => writeln!(out, "counts\tunavailable while errors exist")?,
    }
    match value["next_ids"].as_object() {
        Some(next) => {
            let mut reasons = std::collections::BTreeMap::<String, Vec<&str>>::new();
            let ids: Vec<String> = next
                .iter()
                .map(|(prefix, id)| match id.as_str() {
                    Some(id) => format!("{prefix}={id}"),
                    None => {
                        let error = &value["next_id_errors"][prefix];
                        let reason =
                            format!("{}: {}", text(&error["code"]), text(&error["message"]));
                        reasons.entry(reason).or_default().push(prefix);
                        format!("{prefix}=unavailable")
                    }
                })
                .collect();
            writeln!(out, "next_ids\t{}", ids.join(" "))?;
            for (reason, prefixes) in reasons {
                writeln!(
                    out,
                    "next_ids_unavailable\t{} ({reason})",
                    prefixes.join(" ")
                )?;
            }
        }
        None => writeln!(out, "next_ids\tunavailable while errors exist")?,
    }
    if let Some(session) = value["latest_session"].as_str() {
        writeln!(out, "latest_session\t{session}")?;
    }
    writeln!(
        out,
        "files\t{} ({} bytes)",
        value["file_count"], value["total_bytes"]
    )?;
    let codes = |key: &str| -> Vec<&str> { rows(&value["display"], key).map(text).collect() };
    writeln!(
        out,
        "diagnostics\t{}",
        super::summary(&codes("error_codes"), &codes("warning_codes"))
    )?;
    Ok(())
}

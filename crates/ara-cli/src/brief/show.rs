//! `show` blocks: a labeled header, the selected body and trailing metadata.
//!
//! A native source selection prints its exact bytes under a `source_digest`
//! with the write selector and scope that digest guards. An entry with no
//! native section prints a projection without any digest.
use serde_json::Value;
use std::io::Write;

/// One selection's display. A byte budget pages the row's `content` before
/// rendering ([`crate::agent`] `window`), and each candidate page is
/// measured by rendering its blocks again.
pub struct Block {
    pub header: Vec<String>,
    pub body: String,
    pub trailer: Vec<String>,
}
impl Block {
    pub fn write(&self, out: &mut impl Write) -> std::io::Result<()> {
        for line in &self.header {
            writeln!(out, "{line}")?;
        }
        out.write_all(self.body.as_bytes())?;
        if !self.body.is_empty() && !self.body.ends_with('\n') {
            writeln!(out)?;
        }
        for line in &self.trailer {
            writeln!(out, "{line}")?;
        }
        Ok(())
    }
}

pub fn blocks(value: &Value) -> Vec<Block> {
    value["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            if row["kind"] == "identity" {
                return Block {
                    header: vec![],
                    body: format!(
                        "{} -> {}\n",
                        row["requested_address"].as_str().unwrap_or(""),
                        row["resolved_target"].as_str().unwrap_or("")
                    ),
                    trailer: vec![],
                };
            }
            let mut block = if row["kind"] == "source_document" {
                source(row)
            } else {
                projection(row)
            };
            if let Some(relations) = row["relations"].as_object() {
                for (name, relation) in relations {
                    block.trailer.push(format!("{name}:"));
                    let mut rendered = Vec::new();
                    match name.as_str() {
                        "path" => {
                            super::lists::path(relation, &mut rendered).expect("memory writer")
                        }
                        "refs" => {
                            super::lists::refs(relation, &mut rendered).expect("memory writer")
                        }
                        _ => block.trailer.push(format!("  {relation}")),
                    }
                    block.trailer.extend(
                        String::from_utf8(rendered)
                            .expect("text")
                            .lines()
                            .map(str::to_owned),
                    );
                }
            }
            block
        })
        .collect()
}

/// `--name value`, or `--name=value` when the value starts with `-` so a
/// parser cannot read it as a flag.
fn flag(name: &str, value: &Value) -> String {
    let value = value.as_str().unwrap_or("");
    if value.starts_with('-') {
        format!("--{name}={}", quote(value))
    } else {
        format!("--{name} {}", quote(value))
    }
}

/// Shell-quote one argument when it needs quoting.
pub fn quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-./:@%+=,".contains(&b))
    {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

fn source(row: &Value) -> Block {
    let display = &row["display"];
    let address = display["cited"]
        .as_str()
        .or(row["address"].as_str())
        .unwrap_or("");
    let label = match (&row["entry"], row["heading_path"].as_array()) {
        (Value::Object(entry), _) => format!(
            "{} {}",
            entry["kind"].as_str().unwrap_or(""),
            entry["id"].as_str().unwrap_or("")
        ),
        (_, Some(path)) if !path.is_empty() => "heading".into(),
        _ => "document".into(),
    };
    let mut header = vec![format!("== {address} [{label}]")];
    let path: Vec<&str> = row["heading_path"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !path.is_empty() {
        header.push(format!("heading: {}", path.join(" > ")));
    }
    let selector = match &display["selector"] {
        Value::Object(selector) => {
            let mut text = flag("document", selector.get("document").unwrap_or(&Value::Null));
            for segment in selector
                .get("heading")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                text.push(' ');
                text.push_str(&flag("heading", segment));
            }
            text
        }
        _ => format!(
            "none ({})",
            display["no_selector"]
                .as_str()
                .unwrap_or("no write selector")
        ),
    };
    if let Some(digest) = row["digest"].as_str() {
        header.push(format!(
            "source_digest={digest} scope={} selector: {selector}",
            display["scope"].as_str().unwrap_or("selection")
        ));
    }
    let mut trailer = Vec::new();
    trailer.extend(range(display));
    Block {
        header,
        body: row["content"].as_str().unwrap_or("").to_owned(),
        trailer,
    }
}

/// `lines: S-E of N` for a windowed or paged selection, adding
/// `; truncated; next: --lines A:B` when lines remain.
fn range(display: &Value) -> Option<String> {
    let lines = display.get("lines")?;
    let number = |key: &str| lines[key].as_u64().unwrap_or(0);
    let (start, end, total) = (number("start"), number("end"), number("total"));
    let mut text = if end < start {
        format!("lines: none of {total}")
    } else {
        format!("lines: {start}-{end} of {total}")
    };
    if let Some(next) = display["next"].as_str() {
        text.push_str(&format!("; truncated; next: --lines {next}"));
    }
    Some(text)
}

fn projection(row: &Value) -> Block {
    let address = row["address"]
        .as_str()
        .or(row["id"].as_str())
        .or(row["key"].as_str())
        .unwrap_or("");
    let kind = row["kind"].as_str().unwrap_or("");
    let source = row["source"].as_str().unwrap_or("");
    let mut lines = Vec::new();
    if kind == "session" {
        // A session's `body` is its whole YAML file, which the structured
        // fields already show; the exact-source hint names the file.
        let mut row = row.clone();
        if let Some(object) = row.as_object_mut() {
            object.remove("body");
        }
        fields(&row, 0, &mut lines);
    } else {
        fields(row, 0, &mut lines);
    }
    let mut body = lines.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    Block {
        header: vec![format!(
            "== {address} [{kind}] projection of {source}; no source digest"
        )],
        body,
        trailer: vec![format!(
            "exact source: ara show --document {} --source",
            quote(source)
        )],
    }
}

/// `key: value` lines for an object; nested values indent by two spaces.
fn fields(value: &Value, depth: usize, lines: &mut Vec<String>) {
    let Some(object) = value.as_object() else {
        return;
    };
    let indent = "  ".repeat(depth);
    for (key, value) in object {
        if depth == 0
            && matches!(
                key.as_str(),
                "id" | "key" | "kind" | "source" | "address" | "relations"
            )
        {
            continue;
        }
        match value {
            Value::String(text) if text.contains('\n') => {
                lines.push(format!("{indent}{key}: |"));
                for line in text.lines() {
                    lines.push(format!("{indent}  {line}"));
                }
            }
            _ if empty(value) => {}
            Value::Array(items) if items.iter().all(inline) => {
                let items: Vec<String> = items.iter().map(plain).collect();
                lines.push(format!("{indent}{key}: [{}]", items.join(", ")));
            }
            Value::Array(items) => {
                lines.push(format!("{indent}{key}:"));
                list(items, depth, lines);
            }
            Value::Object(_) => {
                lines.push(format!("{indent}{key}:"));
                fields(value, depth + 1, lines);
            }
            _ => lines.push(format!("{indent}{key}: {}", plain(value))),
        }
    }
}
/// `- item` lines two spaces under `depth`; a nested list renders
/// recursively so none of its values are dropped.
fn list(items: &[Value], depth: usize, lines: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    for item in items.iter().filter(|item| !empty(item)) {
        match item {
            _ if scalar(item) => lines.push(format!("{indent}  - {}", plain(item))),
            Value::String(text) => {
                lines.push(format!("{indent}  - |"));
                for line in text.lines() {
                    lines.push(format!("{indent}    {line}"));
                }
            }
            Value::Array(inner) if inner.iter().all(inline) => {
                let inner: Vec<String> = inner.iter().map(plain).collect();
                lines.push(format!("{indent}  - [{}]", inner.join(", ")));
            }
            Value::Array(inner) => {
                lines.push(format!("{indent}  -"));
                list(inner, depth + 1, lines);
            }
            _ => {
                lines.push(format!("{indent}  -"));
                fields(item, depth + 2, lines);
            }
        }
    }
}
fn empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(items) => items.values().all(empty),
        _ => false,
    }
}
fn scalar(value: &Value) -> bool {
    !value.is_array() && !value.is_object() && !value.as_str().is_some_and(|s| s.contains('\n'))
}
/// A scalar that reads unambiguously inside `[a, b]`.
fn inline(value: &Value) -> bool {
    scalar(value) && !plain(value).contains([',', '[', ']'])
}
fn plain(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rendered(value: Value) -> Vec<String> {
        let mut lines = Vec::new();
        fields(&value, 0, &mut lines);
        lines
    }

    #[test]
    fn list_items_keep_multiline_text_and_ambiguous_scalars_become_lists() {
        assert_eq!(
            rendered(json!({"evidence": ["first line\nsecond line", "one"]})),
            [
                "evidence:",
                "  - |",
                "    first line",
                "    second line",
                "  - one"
            ]
        );
        assert_eq!(rendered(json!({"tags": ["a", "b"]})), ["tags: [a, b]"]);
        assert_eq!(
            rendered(json!({"tags": ["a, b", "c"]})),
            ["tags:", "  - a, b", "  - c"]
        );
        assert_eq!(rendered(json!({"tags": ["[x]"]})), ["tags:", "  - [x]"]);
    }

    #[test]
    fn nested_list_items_keep_their_values() {
        assert_eq!(
            rendered(json!({"events": [["a", "b"], ["c", ["d", "e, f"], {"k": "v"}]]})),
            [
                "events:",
                "  - [a, b]",
                "  -",
                "    - c",
                "    -",
                "      - d",
                "      - e, f",
                "    -",
                "      k: v",
            ]
        );
    }

    #[test]
    fn source_blocks_keep_exact_bodies_and_projections_have_no_digest() {
        let body = "line one\r\nline two";
        let source = blocks(&json!({"entries":[{
            "kind":"source_document","address":"d.md#h/A","heading_path":["A"],
            "document":"d.md","digest":"sha256:ab","content":body,
            "display":{"scope":"heading_body","selector":{"document":"d.md","heading":["A b"]}},
        }]}));
        assert_eq!(source[0].body, body);
        assert!(source[0].header.iter().any(
            |line| line.contains("source_digest=sha256:ab") && line.contains("--heading 'A b'")
        ));
        let projection = blocks(&json!({"entries":[{
            "id":"N01","kind":"question","source":"trace/exploration_tree.yaml","title":"T",
        }]}));
        let text: String = projection[0].header.join("\n") + &projection[0].body;
        assert!(!text.contains("source_digest") && !text.contains("sha256:"));
        assert!(projection[0].trailer[0].contains("--source"));
    }

    #[test]
    fn quoting_round_trips_through_a_posix_shell_reader() {
        assert_eq!(quote("C04"), "C04");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote(""), "''");
        assert_eq!(flag("heading", &json!("-foo")), "--heading=-foo");
        assert_eq!(flag("heading", &json!("- a")), "--heading='- a'");
        assert_eq!(flag("document", &json!("d.md")), "--document d.md");
    }
}

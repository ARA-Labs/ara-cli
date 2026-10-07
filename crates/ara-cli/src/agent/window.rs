//! Bounded and resumable `show` reads.
//!
//! `--lines A:B` keeps one-based inclusive lines of each native source
//! selection. A line ends after its `\n` (so `\r\n` stays whole), and a final
//! newline closes the last line instead of opening an empty one. A byte
//! budget covers the whole response exactly as it is written, headers and
//! metadata included. A single native selection that overflows pages at
//! whole lines and names the next window; any other overflow rejects with
//! the budget it needs. Every page keeps the full selection's `digest`.
use super::ShowArgs;
use crate::output::AgentError;
use serde_json::{Value, json};
use std::io::Write;

/// Brief `show` stdout budget when `--max-bytes` is absent.
pub const DEFAULT_MAX_BYTES: usize = 16 * 1024;

/// A parsed `--lines` value; at least one end is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineSpec {
    start: Option<usize>,
    end: Option<usize>,
}
impl LineSpec {
    pub fn parse(text: &str) -> Result<Self, AgentError> {
        let invalid = || {
            AgentError::setup(
                "invalid_lines",
                format!(
                    "--lines `{text}` must be A:B, A: or :B with one-based line numbers and A <= B"
                ),
            )
        };
        let (start, end) = text.split_once(':').ok_or_else(invalid)?;
        let bound = |part: &str| match part {
            "" => Ok(None),
            part => positive(part).map(Some).ok_or_else(invalid),
        };
        let spec = Self {
            start: bound(start)?,
            end: bound(end)?,
        };
        match (spec.start, spec.end) {
            (None, None) => Err(invalid()),
            (Some(start), Some(end)) if start > end => Err(invalid()),
            _ => Ok(spec),
        }
    }
}

pub fn parse_max_bytes(text: &str) -> Result<usize, AgentError> {
    positive(text).ok_or_else(|| {
        AgentError::setup(
            "invalid_max_bytes",
            format!("--max-bytes `{text}` must be a positive whole number of bytes"),
        )
    })
}

/// A nonzero decimal number without sign or spaces that fits `usize`.
fn positive(text: &str) -> Option<usize> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|value| *value > 0)
}

/// The bounds of one `show`, parsed before anything is read.
pub struct Bounds {
    lines: Option<LineSpec>,
    /// The response budget: explicit, or the brief default.
    budget: Option<usize>,
    /// Without `--json` the budget measures brief text, otherwise JSON.
    brief: bool,
}
impl Bounds {
    pub fn new(args: &ShowArgs) -> Result<Self, AgentError> {
        let lines = args.lines.as_deref().map(LineSpec::parse).transpose()?;
        let explicit = args.max_bytes.as_deref().map(parse_max_bytes).transpose()?;
        let brief = args.output.brief();
        // `--source --full` is the explicit unbounded read.
        let default = (brief && !(args.source && args.output.full)).then_some(DEFAULT_MAX_BYTES);
        Ok(Self {
            lines,
            budget: explicit.or(default),
            brief,
        })
    }

    /// Window each source row of a `show` value, then fit the response to
    /// the budget by paging a single native selection or rejecting.
    pub fn apply(&self, value: &mut Value) -> Result<(), AgentError> {
        if self.lines.is_none() && self.budget.is_none() {
            return Ok(());
        }
        // JSON bounds always report the range; brief text only when asked
        // for a window or when it pages.
        let report = self.lines.is_some() || !self.brief;
        let mut sources = Vec::new();
        for row in rows(value) {
            if row["kind"] != "source_document" {
                if self.lines.is_some() {
                    return Err(lines_unavailable(row));
                }
                sources.push(None);
                continue;
            }
            let source = Source::take(row);
            let window = source.window(self.lines, row)?;
            source.page(row, &window, window.len(), report);
            sources.push(Some((source, window)));
        }
        let Some(budget) = self.budget else {
            return Ok(());
        };
        let size = self.measure(value);
        if size <= budget {
            return Ok(());
        }
        let count = sources.len();
        let (source, window) = match sources.pop() {
            Some(Some(single)) if count == 1 => single,
            Some(None) if count == 1 => {
                let row = &rows(value)[0];
                let hint = projection_hint(row, size);
                return Err(too_small(Some(label(row)), budget, size, None, hint));
            }
            _ => {
                let hint = format!(
                    "The {count} selections need {size} bytes together; read each address separately to page through it, or rerun with --max-bytes {size}"
                );
                return Err(too_small(None, budget, size, None, hint));
            }
        };
        let total = window.len();
        let mut page_size = |lines: usize| {
            source.page(&mut rows(value)[0], &window, lines, true);
            self.measure(value)
        };
        // Bisect over truncated pages of 1..total-1 lines. Their size never
        // shrinks as lines are added: each line adds at least its own
        // terminator or one byte, and the range and `next` numbers only gain
        // digits. The complete page (`total` lines) is excluded because it
        // drops the `truncated; next` text and can be smaller than a
        // truncated page; it was measured above as `size` and did not fit.
        let first = if total > 1 { page_size(1) } else { size };
        if total > 1 && first <= budget {
            let (mut fitting, mut over) = (1, total);
            while over - fitting > 1 {
                let middle = fitting + (over - fitting) / 2;
                if page_size(middle) <= budget {
                    fitting = middle;
                } else {
                    over = middle;
                }
            }
            page_size(fitting);
            return Ok(());
        }
        // No page fits. The cheapest response holding the first line is the
        // one-line truncated page or, when that metadata costs more than the
        // rest of the window, the complete page; `required` is the smaller.
        let required = first.min(size);
        let (line, hint) = if total == 0 {
            (
                None,
                format!(
                    "The selection's metadata needs {required} bytes; rerun with --max-bytes {required} or more"
                ),
            )
        } else {
            (
                Some(window.start),
                format!(
                    "Line {} and its metadata need {required} bytes; rerun with --max-bytes {required} or more",
                    window.start
                ),
            )
        };
        Err(too_small(
            Some(label(&rows(value)[0])),
            budget,
            required,
            line,
            hint,
        ))
    }

    /// Bytes this value prints on stdout.
    fn measure(&self, value: &Value) -> usize {
        let mut count = Count(0);
        if self.brief {
            crate::brief::render(value, &mut count).expect("counting cannot fail");
        } else {
            serde_json::to_writer(&mut count, value).expect("counting cannot fail");
            count.0 += 1;
        }
        count.0
    }
}

fn rows(value: &mut Value) -> &mut Vec<Value> {
    value["entries"].as_array_mut().expect("show entries")
}

/// A writer that only counts bytes.
struct Count(usize);
impl Write for Count {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// One-based inclusive lines `start..=end` of `total`, empty when
/// `end < start`; `upper` is the requested end a continuation keeps.
#[derive(Debug, PartialEq, Eq)]
struct Window {
    start: usize,
    end: usize,
    total: usize,
    upper: Option<usize>,
}
impl Window {
    fn len(&self) -> usize {
        (self.end + 1).saturating_sub(self.start)
    }
}

/// The full content of a source selection and the byte offsets that end
/// its lines: line `n` is `text[breaks[n - 1]..breaks[n]]`.
struct Source {
    text: String,
    breaks: Vec<usize>,
}
impl Source {
    fn new(text: String) -> Self {
        let mut breaks = vec![0];
        breaks.extend(text.match_indices('\n').map(|(at, _)| at + 1));
        if breaks.last() != Some(&text.len()) {
            breaks.push(text.len());
        }
        Self { text, breaks }
    }
    fn take(row: &mut Value) -> Self {
        let text = match row["content"].take() {
            Value::String(text) => text,
            _ => String::new(),
        };
        Self::new(text)
    }
    fn window(&self, spec: Option<LineSpec>, row: &Value) -> Result<Window, AgentError> {
        let total = self.breaks.len() - 1;
        let spec = spec.unwrap_or(LineSpec {
            start: None,
            end: None,
        });
        let start = spec.start.unwrap_or(1);
        if spec.start.is_some() && start > total {
            return Err(out_of_range(row, start, total));
        }
        Ok(Window {
            start,
            end: spec.end.map_or(total, |end| end.min(total)),
            total,
            upper: spec.end,
        })
    }
    /// Show the first `lines` lines of `window` in `row`; `report` adds the
    /// range, which a truncated page always has.
    fn page(&self, row: &mut Value, window: &Window, lines: usize, report: bool) {
        let first = window.start;
        let content = &self.text[self.breaks[first - 1]..self.breaks[first - 1 + lines]];
        row["content"] = json!(content);
        let truncated = lines < window.len();
        if !(report || truncated) {
            return;
        }
        let next = truncated.then(|| {
            let upper = window.upper.map_or_else(String::new, |end| end.to_string());
            format!("{}:{upper}", first + lines)
        });
        let display = &mut row["display"];
        if !display.is_object() {
            *display = json!({});
        }
        display["lines"] = json!({"start": first, "end": first + lines - 1, "total": window.total});
        display["truncated"] = json!(truncated);
        display["next"] = json!(next);
    }
}

/// The address a reader would type: the cited `path#ID` of an entry
/// section, else the entry ID, canonical address or key.
fn label(row: &Value) -> &str {
    row["display"]["cited"]
        .as_str()
        .or_else(|| {
            ["id", "address", "key"]
                .iter()
                .find_map(|key| row[*key].as_str())
        })
        .unwrap_or("")
}

fn out_of_range(row: &Value, start: usize, total: usize) -> AgentError {
    let address = label(row);
    AgentError {
        id: Some(address.to_owned()),
        details: Some(Box::new(json!({
            "start": start,
            "total": total,
            "hint": format!("`{address}` has {total} lines; start at line {total} or earlier, or omit the start"),
        }))),
        ..AgentError::semantic(
            "line_out_of_range",
            format!("--lines starts at line {start}, beyond the {total} lines of `{address}`"),
        )
    }
}

fn exact_source(row: &Value) -> String {
    let source = row["source"].as_str().unwrap_or("");
    format!(
        "ara show --document {} --source",
        crate::brief::quote(source)
    )
}

fn lines_unavailable(row: &Value) -> AgentError {
    let address = label(row);
    AgentError {
        id: Some(address.to_owned()),
        details: Some(Box::new(json!({
            "hint": format!(
                "`{address}` is a projection without native source lines; read its source with `{} --lines A:B`",
                exact_source(row)
            ),
        }))),
        ..AgentError::semantic(
            "lines_unavailable",
            format!("--lines needs a native source selection; `{address}` is a projection"),
        )
    }
}

fn projection_hint(row: &Value, size: usize) -> String {
    if row.get("relations").is_some() || row["kind"] == "identity" {
        return format!(
            "The complete relations or identity mapping and required metadata need {size} bytes; rerun with --max-bytes {size} or more"
        );
    }
    format!(
        "`{}` is a projection with no native line range to page through; read the exact source with `{} --lines A:B`, or rerun with --max-bytes {size}",
        label(row),
        exact_source(row)
    )
}

/// `id` names the one selection that did not fit; a multi-address
/// overflow has none.
fn too_small(
    id: Option<&str>,
    budget: usize,
    required: usize,
    line: Option<usize>,
    hint: String,
) -> AgentError {
    let mut details = json!({"required": required, "max_bytes": budget, "hint": hint});
    if let Some(line) = line {
        details["line"] = json!(line);
    }
    AgentError {
        id: id.map(str::to_owned),
        details: Some(Box::new(details)),
        ..AgentError::semantic(
            "output_limit_too_small",
            format!("The response needs at least {required} bytes; the budget is {budget}"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str) -> Option<(Option<usize>, Option<usize>)> {
        LineSpec::parse(text)
            .ok()
            .map(|spec| (spec.start, spec.end))
    }

    #[test]
    fn line_specs_need_one_positive_ordered_bound() {
        assert_eq!(spec("2:5"), Some((Some(2), Some(5))));
        assert_eq!(spec("3:"), Some((Some(3), None)));
        assert_eq!(spec(":4"), Some((None, Some(4))));
        assert_eq!(spec("4:4"), Some((Some(4), Some(4))));
        for bad in [
            ":", "", "4", "0:1", "1:0", "5:4", "-1:2", "+1:2", "1:2:3", "a:", " 1:2",
        ] {
            assert_eq!(spec(bad), None, "{bad}");
        }
        let overflow = format!("{}0:", usize::MAX);
        assert_eq!(spec(&overflow), None);
        assert_eq!(parse_max_bytes("16").unwrap(), 16);
        for bad in ["0", "-1", "", "1e3", "+5", "99999999999999999999999"] {
            assert!(parse_max_bytes(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn lines_keep_their_terminators_and_a_final_newline_ends_the_last() {
        let lines = |text: &str| {
            let source = Source::new(text.to_owned());
            (1..source.breaks.len())
                .map(|n| source.text[source.breaks[n - 1]..source.breaks[n]].to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(lines("a\r\nb\nc"), ["a\r\n", "b\n", "c"]);
        assert_eq!(lines("a\n"), ["a\n"]);
        assert_eq!(lines("\n\n"), ["\n", "\n"]);
        assert!(lines("").is_empty());
        assert_eq!(lines("é😀\r"), ["é😀\r"]);
    }

    #[test]
    fn windows_clamp_the_end_and_reject_a_start_beyond_eof() {
        let source = Source::new("a\nb\nc\n".to_owned());
        let row = json!({"address": "d.md"});
        let window = |text: &str| source.window(Some(LineSpec::parse(text).unwrap()), &row);
        assert_eq!(
            window("2:9").unwrap(),
            Window {
                start: 2,
                end: 3,
                total: 3,
                upper: Some(9)
            }
        );
        assert_eq!(window(":2").unwrap().len(), 2);
        assert_eq!(window("4:").unwrap_err().code, "line_out_of_range");
        let empty = Source::new(String::new());
        assert_eq!(
            empty
                .window(LineSpec::parse(":3").ok(), &row)
                .unwrap()
                .len(),
            0
        );
        assert!(empty.window(LineSpec::parse("1:").ok(), &row).is_err());
    }

    #[test]
    fn a_page_names_the_next_window_with_the_original_upper_bound() {
        let source = Source::new("a\nb\nc\nd\n".to_owned());
        let row_of = |spec: &str, lines: usize| {
            let mut row = json!({"address": "d.md"});
            let window = source
                .window(Some(LineSpec::parse(spec).unwrap()), &row)
                .unwrap();
            source.page(&mut row, &window, lines, false);
            row
        };
        let row = row_of("2:9", 1);
        assert_eq!(row["content"], "b\n");
        assert_eq!(row["display"]["next"], "3:9");
        assert_eq!(
            row["display"]["lines"],
            json!({"start":2,"end":2,"total":4})
        );
        let row = row_of("2:", 2);
        assert_eq!(row["display"]["next"], "4:");
        // A complete window that was not asked to report adds nothing.
        let row = row_of("1:", 4);
        assert_eq!(row["content"], "a\nb\nc\nd\n");
        assert!(row.get("display").is_none());
    }
}

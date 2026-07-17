//! Lenient Markdown readers for the `logic/` section files.
//!
//! Three pure `str → struct` readers, mirroring [`crate::claims`]:
//! [`parse_problem`] (`problem.md`), [`parse_concepts`] (`concepts.md`), and
//! [`parse_related_work`] (`related_work.md`). All are tolerant: missing bullets
//! and drifting section shapes yield fewer items, never an error or a panic.
//! Output preserves source order; nothing is sorted by id. Values (including
//! LaTeX `$…$`) are stored verbatim — rendering is a later concern.

use crate::manifest::{ClaimId, Concept, Problem, RelatedWork, is_canonical_id};

// ── problem.md ───────────────────────────────────────────────────────────────

/// Parses `problem.md` into a [`Problem`].
///
/// Recognizes `O#`/`G#`/`I#` items whether written as `### O1: …` headings or
/// bold bullets `- **O1**: …`, plus a `- **Insight**:` bullet under a Key
/// Insight section. The demo `ls20` dialect (`## Goal`/`## Setting`/`## Success
/// criterion`) is folded in: `Goal` seeds the statement, `Setting`/`Success
/// criterion` become observations. Prose before the first `##` section is the
/// statement fallback. No section is required.
pub fn parse_problem(md: &str) -> Problem {
    let lines: Vec<&str> = md.lines().collect();

    // Leading prose: non-heading, non-blank lines before the first `##` header.
    let mut statement: Option<String> = leading_prose(&lines);

    let mut observations = Vec::new();
    let mut gaps = Vec::new();
    let mut insights = Vec::new();

    // Item pass: id-prefixed headings/bullets, classified by prefix letter.
    for line in &lines {
        if let Some((prefix, text)) = classify_item(line) {
            match prefix {
                'O' => observations.push(text),
                'G' => gaps.push(text),
                'I' => insights.push(text),
                _ => {}
            }
        } else if let Some(value) = insight_bullet(line) {
            insights.push(value);
        }
    }

    // `ls20` named sections: Goal → statement, Setting/Success → observations.
    for (name, body) in h2_sections(&lines) {
        match name.trim().to_ascii_lowercase().as_str() {
            "goal" => {
                if statement.is_none() {
                    statement = join_prose(&body);
                }
            }
            "setting" | "success criterion" => {
                if let Some(prose) = join_prose(&body) {
                    observations.push(format!("{name}: {prose}"));
                }
            }
            _ => {}
        }
    }

    Problem {
        statement,
        observations,
        gaps,
        insights,
    }
}

/// Non-heading, non-blank prose before the first `##` section (an `# H1` is
/// skipped). `None` when there is none.
fn leading_prose(lines: &[&str]) -> Option<String> {
    let mut collected = Vec::new();
    for line in lines {
        let t = line.trim_start();
        if t.starts_with("## ") {
            break;
        }
        if t.starts_with("# ") || t.trim().is_empty() {
            continue;
        }
        collected.push(line.trim());
    }
    join_lines(collected)
}

/// Classifies a heading or bold-bullet line carrying an `O#`/`G#`/`I#` id.
/// Returns `(prefix_letter, full_item_text)` with the id preserved.
fn classify_item(line: &str) -> Option<(char, String)> {
    let t = line.trim_start();
    let content = match strip_heading(t) {
        Some(s) => s.to_string(),
        None => strip_bold_label(t)?,
    };
    let prefix = leading_id_prefix(&content)?;
    Some((prefix, content))
}

/// Strips a leading run of `#` plus a space, returning the heading text.
fn strip_heading(t: &str) -> Option<&str> {
    if !t.starts_with('#') {
        return None;
    }
    let rest = t.trim_start_matches('#');
    if rest.starts_with(' ') {
        Some(rest.trim())
    } else {
        None
    }
}

/// For a bold-labeled bullet `- **Label**: value`, returns `Label: value`
/// (bold stripped). Handles `- **O1**: x` and `- **O1: x**` alike.
fn strip_bold_label(t: &str) -> Option<String> {
    let rest = t.strip_prefix("- ").or_else(|| t.strip_prefix("* "))?;
    let rest = rest.trim_start().strip_prefix("**")?;
    let (label, tail) = rest.split_once("**")?;
    let tail = tail.trim_start();
    let value = tail.strip_prefix(':').unwrap_or(tail).trim();
    if value.is_empty() {
        Some(label.trim().to_string())
    } else {
        Some(format!("{}: {}", label.trim(), value))
    }
}

/// The leading `O#`/`G#`/`I#` id prefix of an item's text, if any. The letter
/// must be followed by one or more digits, then `:`, whitespace, or the end.
fn leading_id_prefix(s: &str) -> Option<char> {
    let s = s.trim_start();
    let mut chars = s.chars();
    let first = chars.next()?;
    if !matches!(first, 'O' | 'G' | 'I') {
        return None;
    }
    let rest = chars.as_str();
    let digit_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_len == 0 {
        return None;
    }
    let after = &rest[digit_len..];
    if after.is_empty() || after.starts_with(':') || after.starts_with(char::is_whitespace) {
        Some(first)
    } else {
        None
    }
}

/// The value of a `- **Insight**: …` bullet, if the line is one.
fn insight_bullet(line: &str) -> Option<String> {
    let content = strip_bold_label(line.trim_start())?;
    let (label, value) = content.split_once(':')?;
    if label.trim().eq_ignore_ascii_case("insight") {
        let v = value.trim();
        (!v.is_empty()).then(|| v.to_string())
    } else {
        None
    }
}

/// Splits `lines` into `## <Name>` sections, returning `(name, body_lines)` for
/// each in source order. Body runs until the next `##` header.
fn h2_sections<'a>(lines: &[&'a str]) -> Vec<(String, Vec<&'a str>)> {
    let mut sections = Vec::new();
    let mut current: Option<(String, Vec<&'a str>)> = None;
    for line in lines {
        if let Some(name) = line.trim_start().strip_prefix("## ") {
            if let Some(sec) = current.take() {
                sections.push(sec);
            }
            current = Some((name.trim().to_string(), Vec::new()));
        } else if let Some((_, body)) = current.as_mut() {
            body.push(line);
        }
    }
    if let Some(sec) = current.take() {
        sections.push(sec);
    }
    sections
}

/// Joins non-blank body lines (dropping bullet/heading markers) into prose.
fn join_prose(body: &[&str]) -> Option<String> {
    let collected: Vec<&str> = body
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")).unwrap_or(l))
        .collect();
    join_lines(collected)
}

fn join_lines(collected: Vec<&str>) -> Option<String> {
    if collected.is_empty() {
        None
    } else {
        Some(collected.join("\n"))
    }
}

// ── concepts.md ──────────────────────────────────────────────────────────────

/// Parses `concepts.md` into glossary [`Concept`]s.
///
/// Splits on `## <Term>` headers; within each, reads `- **Label**: value`
/// bullets matched by case-insensitive label prefix: `Definition`, `Notation`,
/// `Boundary` (also `Boundary conditions`), `Related` (also `Related concepts`,
/// comma-split into names). LaTeX values are preserved verbatim.
pub fn parse_concepts(md: &str) -> Vec<Concept> {
    let lines: Vec<&str> = md.lines().collect();
    let mut concepts = Vec::new();

    for (term, body) in h2_sections(&lines) {
        let mut notation = None;
        let mut definition = None;
        let mut boundary = None;
        let mut related = Vec::new();

        for line in &body {
            let Some((label, value)) = parse_labeled_bullet(line) else {
                continue;
            };
            let label = label.to_ascii_lowercase();
            if label.starts_with("definition") {
                definition = non_empty(&value);
            } else if label.starts_with("notation") {
                notation = non_empty(&value);
            } else if label.starts_with("boundary") {
                boundary = non_empty(&value);
            } else if label.starts_with("related") {
                related = split_names(&value);
            }
        }

        concepts.push(Concept {
            term,
            notation,
            definition,
            boundary,
            related,
        });
    }

    concepts
}

/// Splits a comma-separated list of names, trimming and dropping empties.
fn split_names(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

// ── related_work.md ──────────────────────────────────────────────────────────

/// Parses `related_work.md` into typed [`RelatedWork`] dependencies.
///
/// Splits on `## RW\d+: <cite>` headers. Within each block, reads the bold
/// bullets `DOI`, `Type` (raw, tolerating `baseline, extends`), `Claims
/// affected` (`C##` tokens; prose `none` → empty), `Adopted elements`, and the
/// nested `Delta` sub-bullets `What changed` / `Why`.
pub fn parse_related_work(md: &str) -> Vec<RelatedWork> {
    let lines: Vec<&str> = md.lines().collect();
    let mut out = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let Some((id, cite)) = parse_rw_header(lines[i]) else {
            i += 1;
            continue;
        };
        // Body runs until the next `##` header.
        let mut j = i + 1;
        while j < lines.len() && !lines[j].trim_start().starts_with("## ") {
            j += 1;
        }
        let body = &lines[i + 1..j];

        let mut doi = None;
        let mut kind = None;
        let mut what_changed = None;
        let mut why = None;
        let mut adopted = None;
        let mut claims_affected = Vec::new();

        for line in body {
            let Some((label, value)) = parse_labeled_bullet(line) else {
                continue;
            };
            let label = label.to_ascii_lowercase();
            if label.starts_with("doi") {
                doi = non_empty(&value);
            } else if label.starts_with("type") {
                kind = non_empty(&value);
            } else if label.starts_with("what changed") {
                what_changed = non_empty(&value);
            } else if label == "why" || label.starts_with("why") {
                why = non_empty(&value);
            } else if label.starts_with("claims affected") {
                claims_affected = extract_claim_ids(&value);
            } else if label.starts_with("adopted") {
                adopted = non_empty(&value);
            }
        }

        out.push(RelatedWork {
            id,
            cite,
            doi,
            kind,
            what_changed,
            why,
            adopted,
            claims_affected,
        });
        i = j;
    }

    out
}

/// Matches `## RW\d+: <cite>`, returning `(id, cite)`. The cite is the header
/// text after the id (tolerating `—`, `(name)`, etc.).
fn parse_rw_header(line: &str) -> Option<(String, String)> {
    let rest = line.trim_start().strip_prefix("## ")?;
    let (raw_id, cite) = rest.split_once(':')?;
    let id = raw_id.trim();
    if !is_rw_id(id) {
        return None;
    }
    Some((id.to_string(), cite.trim().to_string()))
}

/// True for `RW\d+`.
fn is_rw_id(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("RW") else {
        return false;
    };
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
}

/// Extracts every `C\d+` token from a value, splitting on non-alphanumerics.
/// Prose `none` (no tokens) yields an empty list.
fn extract_claim_ids(value: &str) -> Vec<ClaimId> {
    value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|tok| is_canonical_id(tok, 'C'))
        .map(ClaimId::new)
        .collect()
}

// ── shared bullet parsing ────────────────────────────────────────────────────

/// Parses a bullet into `(label, value)`. Handles both the bold form
/// `- **Label**: value` and the plain form `- Label: value` (used by nested
/// `Delta` sub-bullets). Returns `None` for non-bullets and label-less bullets.
fn parse_labeled_bullet(line: &str) -> Option<(String, String)> {
    let t = line.trim_start();
    let rest = t.strip_prefix("- ").or_else(|| t.strip_prefix("* "))?;
    let rest = rest.trim_start();
    if let Some(after_stars) = rest.strip_prefix("**") {
        let (label, tail) = after_stars.split_once("**")?;
        let tail = tail.trim_start();
        let value = tail.strip_prefix(':').unwrap_or(tail).trim();
        Some((label.trim().to_string(), value.to_string()))
    } else {
        let (label, value) = rest.split_once(':')?;
        Some((label.trim().to_string(), value.trim().to_string()))
    }
}

/// Trims and returns `None` for empty values.
fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problem_heading_style() {
        let md = "\
# Problem Specification

## Observations

### O1: Networks forget
- **Statement**: overwriting weights.

### O2: Quadratic growth
- **Statement**: lateral connections.

## Gaps

### G1: No linear scaling
- **Statement**: existing methods fail.

## Key Insight

- **Insight**: give direct access to previous outputs.
- **Derived from**: O1, O2
";
        let p = parse_problem(md);
        assert_eq!(p.observations, vec!["O1: Networks forget", "O2: Quadratic growth"]);
        assert_eq!(p.gaps, vec!["G1: No linear scaling"]);
        assert_eq!(p.insights, vec!["give direct access to previous outputs."]);
        assert!(p.statement.is_none());
    }

    #[test]
    fn problem_bold_bullet_style() {
        let md = "\
## Observations
- **O1**: sample-specific masks exist
- **O2**: shared masks cause positive loss
## Gaps
- **G1**: no sample-level adaptation
";
        let p = parse_problem(md);
        assert_eq!(
            p.observations,
            vec!["O1: sample-specific masks exist", "O2: shared masks cause positive loss"]
        );
        assert_eq!(p.gaps, vec!["G1: no sample-level adaptation"]);
    }

    #[test]
    fn problem_ls20_goal_setting_style() {
        let md = "\
# Problem

## Goal

Play and solve the game by inferring mechanics.

## Setting

- Board: 64x64 grid.
- Budget bar depletes per action.

## Success criterion

A move drives levels_completed up by 1.
";
        let p = parse_problem(md);
        assert_eq!(
            p.statement.as_deref(),
            Some("Play and solve the game by inferring mechanics.")
        );
        assert_eq!(p.observations.len(), 2);
        assert!(p.observations[0].starts_with("Setting: "));
        assert!(p.observations[0].contains("Board: 64x64 grid."));
        assert!(p.observations[1].starts_with("Success criterion: "));
    }

    #[test]
    fn concepts_all_fields_with_latex() {
        let md = "\
# Concepts

## CompoNet
- **Notation**: CompoNet with modules $\\{\\pi^{(1)}, \\ldots, \\pi^{(n)}\\}$
- **Definition**: A growable modular network.
- **Boundary conditions**: Requires known task boundaries.
- **Related concepts**: Policy Module, ProgressiveNet, CRL
";
        let c = parse_concepts(md);
        assert_eq!(c.len(), 1);
        let cc = &c[0];
        assert_eq!(cc.term, "CompoNet");
        assert_eq!(
            cc.notation.as_deref(),
            Some("CompoNet with modules $\\{\\pi^{(1)}, \\ldots, \\pi^{(n)}\\}$")
        );
        assert_eq!(cc.definition.as_deref(), Some("A growable modular network."));
        assert_eq!(cc.boundary.as_deref(), Some("Requires known task boundaries."));
        assert_eq!(cc.related, vec!["Policy Module", "ProgressiveNet", "CRL"]);
    }

    #[test]
    fn concepts_short_labels_and_missing_definition() {
        // `Boundary` and `Related` (short forms) still match; no Definition line.
        let md = "\
## Phi Matrix
- **Notation**: $\\Phi^{k;s}$
- **Boundary**: only defined when k >= 2
- **Related**: Policy Module
";
        let c = parse_concepts(md);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].notation.as_deref(), Some("$\\Phi^{k;s}$"));
        assert_eq!(c[0].boundary.as_deref(), Some("only defined when k >= 2"));
        assert_eq!(c[0].related, vec!["Policy Module"]);
        assert!(c[0].definition.is_none());
    }

    #[test]
    fn related_work_all_fields() {
        let md = "\
# Related Work

## RW01: Rusu et al., 2016 — Progressive Neural Networks
- **DOI**: arXiv:1606.04671
- **Type**: baseline
- **Delta**:
  - What changed: CompoNet composes at the output level.
  - Why: linear O(n) growth vs quadratic.
- **Claims affected**: C01, C04
- **Adopted elements**: Freeze-and-grow paradigm.
";
        let rw = parse_related_work(md);
        assert_eq!(rw.len(), 1);
        let r = &rw[0];
        assert_eq!(r.id, "RW01");
        assert_eq!(r.cite, "Rusu et al., 2016 — Progressive Neural Networks");
        assert_eq!(r.doi.as_deref(), Some("arXiv:1606.04671"));
        assert_eq!(r.kind.as_deref(), Some("baseline"));
        assert_eq!(
            r.what_changed.as_deref(),
            Some("CompoNet composes at the output level.")
        );
        assert_eq!(r.why.as_deref(), Some("linear O(n) growth vs quadratic."));
        assert_eq!(r.adopted.as_deref(), Some("Freeze-and-grow paradigm."));
        assert_eq!(
            r.claims_affected,
            vec![ClaimId::new("C01"), ClaimId::new("C04")]
        );
    }

    #[test]
    fn related_work_combined_type_and_none_claims() {
        let md = "\
## RW02: Foo et al., 2020 (PackNet)
- **Type**: baseline, extends
- **Claims affected**: none
";
        let rw = parse_related_work(md);
        assert_eq!(rw.len(), 1);
        assert_eq!(rw[0].kind.as_deref(), Some("baseline, extends"));
        assert!(rw[0].claims_affected.is_empty());
        assert!(rw[0].doi.is_none());
    }

    #[test]
    fn empty_inputs_yield_empty_output() {
        assert_eq!(parse_concepts(""), vec![]);
        assert_eq!(parse_related_work(""), vec![]);
        let p = parse_problem("");
        assert!(p.statement.is_none());
        assert!(p.observations.is_empty());
    }
}

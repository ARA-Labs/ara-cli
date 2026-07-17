//! Detail pane — pure render model + Leptos `DetailPane` component.
//!
//! The file is split into two halves:
//! 1. **Pure model** (`DetailModel`, `detail_model`): no web-sys deps, fully
//!    native-testable.  All per-kind field ordering and claim-resolution logic
//!    lives here.
//! 2. **Leptos component** (`DetailPane`): depends on `leptos::prelude::*`. Like
//!    the rest of the viewer it compiles on native too (no browser-only APIs),
//!    so no `cfg` gating is needed.

use ara_core::{ExhibitKind, Manifest, Node, NodeFields, NodeId};
use leptos::prelude::*;

use crate::kind::kind_meta;
use crate::state::LoadState;

// ── Pure render model ─────────────────────────────────────────────────────────

/// A fully-resolved claim, ready to render.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimView {
    pub title: String,
    pub statement: Option<String>,
    pub status: Option<String>,
}

/// The display value for a typed field.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Text(String),
    List(Vec<String>),
}

/// A related-work reference this node builds on (BUILT ON block).
///
/// Resolved from a `manifest.built_on` edge against `manifest.related_work`.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltOnView {
    /// Related-work id (e.g. `RW01`).
    pub id: String,
    /// Citation text; empty when the id has no matching `related_work` entry.
    pub cite: String,
}

/// An exhibit (figure/table) linked to this node (RESULT block).
///
/// Resolved from a `manifest.node_exhibits` edge against `manifest.exhibits`.
/// Chips + linkage only — the exhibit `body` is intentionally not carried here
/// (table/markdown rendering is deferred; see the RESULT block comment).
#[derive(Debug, Clone, PartialEq)]
pub struct ExhibitView {
    /// Exhibit id.
    pub id: String,
    /// Source file, relative to the artifact root.
    pub file: String,
    /// Short kind label: `"figure"` | `"table"` | `"other"`.
    pub kind: String,
    /// Origin of the exhibit, when stated.
    pub source: Option<String>,
}

/// A single typed field entry in the per-kind section.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedField {
    pub label: &'static str,
    pub value: FieldValue,
    /// When true, this field gets the `.block.reason` accent-spine treatment.
    pub is_primary: bool,
}

/// The complete render model for one node, order-preserving.
///
/// Built by [`detail_model`] from a [`Node`] + [`Manifest`]; consumed by the
/// Leptos `DetailPane` component (and by unit tests, without a browser).
#[derive(Debug, Clone, PartialEq)]
pub struct DetailModel {
    /// `label ?? id` — always present.
    pub title: String,
    /// Kind badge string: canonical lowercase for named kinds, raw string for
    /// `Other(_)`.
    pub kind_badge: String,
    /// CSS class for the kind chip wrapper (e.g. `"dead_end"`, `"other"`).
    pub kind_css_class: String,
    /// Single-character glyph.
    pub kind_glyph: char,
    /// `explicit` | `inferred` when present.
    pub support_level: Option<String>,
    /// Prose description. `None` → omit the block.
    pub description: Option<String>,
    /// Per-kind fields in the kind-specific canonical order.
    pub typed_fields: Vec<TypedField>,
    /// Free-text evidence notes (the non-`C##` entries from `evidence:`).
    pub evidence_notes: Vec<String>,
    /// Claims resolved from `manifest.bindings` filtered to this node.
    pub claims: Vec<ClaimView>,
    /// Related work this node builds on (BUILT ON block), in source order.
    pub built_on: Vec<BuiltOnView>,
    /// Exhibits linked to this node (RESULT block), in source order.
    pub result_exhibits: Vec<ExhibitView>,
    /// Provenance refs.
    pub source_refs: Vec<String>,
}

impl DetailModel {
    /// True when there is nothing to show beyond the header.
    ///
    /// Criteria: description is `None`, no typed fields, no evidence notes, no
    /// claims, no built-on refs, no result exhibits, no source refs.
    pub fn is_empty(&self) -> bool {
        self.description.is_none()
            && self.typed_fields.is_empty()
            && self.evidence_notes.is_empty()
            && self.claims.is_empty()
            && self.built_on.is_empty()
            && self.result_exhibits.is_empty()
            && self.source_refs.is_empty()
    }
}

/// Build a [`DetailModel`] from `node` + the enclosing `manifest`.
///
/// This is the single authoritative place for:
/// - `label ?? id` title resolution,
/// - per-kind typed-field ordering,
/// - claim resolution via `manifest.bindings` → `manifest.claims`.
///
/// **Never panics.** A binding whose claim id is not found in `manifest.claims`
/// is silently skipped (graceful degradation).
pub fn detail_model(node: &Node, manifest: &Manifest) -> DetailModel {
    let meta = kind_meta(&node.kind);

    let title = node
        .label
        .clone()
        .unwrap_or_else(|| node.id.as_str().to_string());

    // ── Typed fields in kind-specific order ───────────────────────────────────
    let typed_fields = typed_fields_for(node);

    // ── Claim resolution: bindings → claims, preserve binding order ──────────
    let claims: Vec<ClaimView> = manifest
        .bindings
        .iter()
        .filter(|b| b.node == node.id)
        .filter_map(|b| {
            manifest
                .claims
                .iter()
                .find(|c| c.id == b.claim)
                .map(|c| ClaimView {
                    title: c.title.clone(),
                    statement: c.statement.clone(),
                    status: c.status.clone(),
                })
        })
        .collect();

    // ── BUILT ON: node→related-work edges, resolved to id + cite ─────────────
    // Preserve `manifest.built_on` order. A missing related_work entry still
    // yields a view with an empty cite (never panics, never drops the id).
    let built_on: Vec<BuiltOnView> = manifest
        .built_on
        .iter()
        .filter(|b| b.node == node.id)
        .map(|b| {
            let cite = manifest
                .related_work
                .iter()
                .find(|rw| rw.id == b.related_work)
                .map(|rw| rw.cite.clone())
                .unwrap_or_default();
            BuiltOnView {
                id: b.related_work.clone(),
                cite,
            }
        })
        .collect();

    // ── RESULT: node→exhibit edges, resolved to id/file/kind/source ──────────
    // Preserve `manifest.node_exhibits` order. An exhibit id with no matching
    // exhibit is skipped (graceful degradation), never panics.
    let result_exhibits: Vec<ExhibitView> = manifest
        .node_exhibits
        .iter()
        .filter(|ne| ne.node == node.id)
        .filter_map(|ne| {
            manifest
                .exhibits
                .iter()
                .find(|ex| ex.id == ne.exhibit)
                .map(|ex| ExhibitView {
                    id: ex.id.clone(),
                    file: ex.file.clone(),
                    kind: exhibit_kind_label(&ex.kind).to_string(),
                    source: ex.source.clone(),
                })
        })
        .collect();

    DetailModel {
        title,
        kind_badge: meta.badge,
        kind_css_class: meta.css_class.to_string(),
        kind_glyph: meta.glyph,
        support_level: node.support_level.clone(),
        description: node.description.clone(),
        typed_fields,
        evidence_notes: node.evidence_notes.clone(),
        claims,
        built_on,
        result_exhibits,
        source_refs: node.source_refs.clone(),
    }
}

/// Short lowercase label for an [`ExhibitKind`], matching our lowercase
/// block-label convention.
fn exhibit_kind_label(kind: &ExhibitKind) -> &'static str {
    match kind {
        ExhibitKind::Figure => "figure",
        ExhibitKind::Table => "table",
        ExhibitKind::Other => "other",
    }
}

/// Build the per-kind typed fields in the canonical plan-specified order.
///
/// Order requirements (from plan):
/// - `Question`  → none
/// - `Experiment { result }` → `[("what it did", result)]` if Some; mark
///   primary (the experiment result is the node's WHAT IT DID block)
/// - `Decision { choice, rationale, alternatives }` →
///   `("choice", choice?)`, `("rationale", rationale?)` [primary],
///   `("alternatives", alternatives)` if non-empty; omit None/empty
/// - `DeadEnd { hypothesis, failure_mode, lesson, why_failed }` →
///   `("hypothesis", hypothesis?)`, `("failure mode", failure_mode?)` [primary],
///   `("lesson", lesson?)`, `("why failed", why_failed?)` [primary only when
///   `failure_mode` is absent — the legacy field is promoted so there is still
///   an accented block]; omit None
/// - `Insight`   → none
/// - `Pivot { from, to, trigger }` → `("from", from?)`, `("to", to?)`,
///   `("trigger", trigger?)` [primary]; omit None
/// - `Other`     → none
fn typed_fields_for(node: &Node) -> Vec<TypedField> {
    match &node.fields {
        NodeFields::Question | NodeFields::Insight | NodeFields::Other => vec![],

        NodeFields::Experiment { result } => {
            let mut fields = Vec::new();
            if let Some(r) = result {
                // Relabelled to "what it did" per the corrected hub order (the
                // experiment's `result` field is the node's WHAT IT DID block).
                fields.push(TypedField {
                    label: "what it did",
                    value: FieldValue::Text(r.clone()),
                    is_primary: true,
                });
            }
            fields
        }

        NodeFields::Decision {
            choice,
            rationale,
            alternatives,
        } => {
            let mut fields = Vec::new();
            if let Some(c) = choice {
                fields.push(TypedField {
                    label: "choice",
                    value: FieldValue::Text(c.clone()),
                    is_primary: false,
                });
            }
            if let Some(r) = rationale {
                fields.push(TypedField {
                    label: "rationale",
                    value: FieldValue::Text(r.clone()),
                    is_primary: true,
                });
            }
            if !alternatives.is_empty() {
                fields.push(TypedField {
                    label: "alternatives",
                    value: FieldValue::List(alternatives.clone()),
                    is_primary: false,
                });
            }
            fields
        }

        NodeFields::DeadEnd {
            hypothesis,
            failure_mode,
            lesson,
            why_failed,
        } => {
            let mut fields = Vec::new();
            if let Some(h) = hypothesis {
                fields.push(TypedField {
                    label: "hypothesis",
                    value: FieldValue::Text(h.clone()),
                    is_primary: false,
                });
            }
            if let Some(fm) = failure_mode {
                fields.push(TypedField {
                    label: "failure mode",
                    value: FieldValue::Text(fm.clone()),
                    is_primary: true,
                });
            }
            if let Some(l) = lesson {
                fields.push(TypedField {
                    label: "lesson",
                    value: FieldValue::Text(l.clone()),
                    is_primary: false,
                });
            }
            if let Some(w) = why_failed {
                // `failure_mode` is the modern primary field; when a node carries
                // only the legacy `why_failed`, promote it so the pane still has a
                // primary (accented) block instead of none.
                fields.push(TypedField {
                    label: "why failed",
                    value: FieldValue::Text(w.clone()),
                    is_primary: failure_mode.is_none(),
                });
            }
            fields
        }

        NodeFields::Pivot { from, to, trigger } => {
            let mut fields = Vec::new();
            if let Some(f) = from {
                fields.push(TypedField {
                    label: "from",
                    value: FieldValue::Text(f.clone()),
                    is_primary: false,
                });
            }
            if let Some(t) = to {
                fields.push(TypedField {
                    label: "to",
                    value: FieldValue::Text(t.clone()),
                    is_primary: false,
                });
            }
            if let Some(t) = trigger {
                fields.push(TypedField {
                    label: "trigger",
                    value: FieldValue::Text(t.clone()),
                    is_primary: true,
                });
            }
            fields
        }
    }
}

// ── Leptos component ──────────────────────────────────────────────────────────
// Like the rest of the viewer (scene.rs, main.rs), this component is compiled
// on both native and wasm32 targets.  No browser-only APIs are used here;
// the Leptos proc-macros and signal types work on native too.

/// Renders the detail pane for the currently selected node.
///
/// Reacts to both `load_state` and `selected`:
/// - `selected` is `None` → placeholder "Select a step to see its details."
/// - `selected` is `Some(id)` but manifest not loaded → same placeholder
/// - `selected` is `Some(id)` and manifest loaded → find node and render
#[component]
pub fn DetailPane(
    load_state: ReadSignal<LoadState>,
    selected: RwSignal<Option<NodeId>>,
) -> impl IntoView {
    move || {
        let sel = selected.get();
        let state = load_state.get();

        match (sel, state) {
            (None, _) | (_, LoadState::Loading) => view! {
                <p class="placeholder-text">"Select a step to see its details."</p>
            }
            .into_any(),

            (_, LoadState::Failed(_)) => view! {
                <p class="placeholder-text">"Select a step to see its details."</p>
            }
            .into_any(),

            (Some(id), LoadState::Loaded(manifest)) => {
                match manifest.nodes.iter().find(|n| n.id == id) {
                    None => view! {
                        <p class="placeholder-text">"Node not found."</p>
                    }
                    .into_any(),
                    Some(node) => {
                        let model = detail_model(node, &manifest);
                        render_detail(model).into_any()
                    }
                }
            }
        }
    }
}

/// Render a fully-populated `DetailModel` into DOM.
fn render_detail(m: DetailModel) -> impl IntoView {
    let is_empty = m.is_empty();
    let dead_end_class = if m.kind_css_class == "dead_end" {
        "dead_end"
    } else {
        ""
    };

    view! {
        <div class="detail-root">
            // ── 1. Header ─────────────────────────────────────────────────
            <div class="detail-header">
                <h2 class="detail-title">{m.title.clone()}</h2>
                <div class="detail-meta">
                    // Kind chip: glyph + badge text.  dead_end gets warn colour.
                    <span class=format!("kind-chip-wrap {}", dead_end_class)>
                        <span class=format!("kind-chip {}", dead_end_class)>
                            {m.kind_glyph.to_string()}
                        </span>
                        <span class="kind-badge">{m.kind_badge.clone()}</span>
                    </span>
                    // Support-level pill (explicit/inferred) when present.
                    {m.support_level.clone().map(|sl| view! {
                        <span class="pill support">{sl}</span>
                    })}
                </div>
            </div>

            // ── 2. Description ────────────────────────────────────────────
            {m.description.clone().map(|desc| view! {
                <div class="block description-block">
                    <p class="description-text">{desc}</p>
                </div>
            })}

            // ── Inert REASONING slot ──────────────────────────────────────
            // REASONING is intentionally dropped for v1 (D1): the schema does
            // not yet carry a stored `reasoning:` field. When it does, the
            // REASONING block plugs in HERE — above WHAT IT DID — matching the
            // hub order. No visible UI until then.

            // ── 3. WHAT IT DID + other typed fields (per-kind order) ───────
            // For an Experiment the sole typed field is `result`, relabelled
            // "what it did" in `typed_fields_for` (the WHAT IT DID block).
            {m.typed_fields.iter().map(|tf| {
                let block_class = if tf.is_primary {
                    "block reason"
                } else {
                    "block"
                };
                let label = tf.label;
                let value_view = match &tf.value {
                    FieldValue::Text(t) => view! {
                        <p class="field-value">{t.clone()}</p>
                    }.into_any(),
                    FieldValue::List(items) => view! {
                        <ul class="field-list">
                            {items.iter().map(|item| view! {
                                <li>{item.clone()}</li>
                            }).collect::<Vec<_>>()}
                        </ul>
                    }.into_any(),
                };
                view! {
                    <div class=block_class>
                        <span class="block-label">{label}</span>
                        {value_view}
                    </div>
                }
            }).collect::<Vec<_>>()}

            // ── 4. Evidence (notes + claims) ──────────────────────────────
            // Omit the whole block if both are empty.
            {if !m.evidence_notes.is_empty() || !m.claims.is_empty() {
                Some(view! {
                    <div class="block evidence-block">
                        <span class="block-label">"evidence"</span>
                        // Evidence notes list
                        {if !m.evidence_notes.is_empty() {
                            Some(view! {
                                <ul class="evidence-notes">
                                    {m.evidence_notes.iter().map(|note| view! {
                                        <li>{note.clone()}</li>
                                    }).collect::<Vec<_>>()}
                                </ul>
                            })
                        } else {
                            None
                        }}
                        // Bound claims
                        {m.claims.iter().map(|cv| {
                            let status_class = status_css_class(cv.status.as_deref());
                            view! {
                                <div class="claim">
                                    <span class="claim-title">{cv.title.clone()}</span>
                                    {cv.statement.clone().map(|stmt| view! {
                                        <p class="claim-statement">{stmt}</p>
                                    })}
                                    {cv.status.clone().map(|st| view! {
                                        <span class=format!("status-pill status-{}", status_class)>
                                            {st}
                                        </span>
                                    })}
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                })
            } else {
                None
            }}

            // ── 5. Built on (related work this node builds on) ─────────────
            // Omit entirely when empty — a bare node shows nothing, matching
            // the hub. Chips carry the RW id + cite.
            {if !m.built_on.is_empty() {
                Some(view! {
                    <div class="block built-on-block">
                        <span class="block-label">"built on"</span>
                        <div class="chip-row">
                            {m.built_on.iter().map(|bo| {
                                let text = if bo.cite.is_empty() {
                                    bo.id.clone()
                                } else {
                                    format!("{} · {}", bo.id, bo.cite)
                                };
                                view! { <span class="chip">{text}</span> }
                            }).collect::<Vec<_>>()}
                        </div>
                    </div>
                })
            } else {
                None
            }}

            // ── 6. Result (exhibits linked to this node) ───────────────────
            // Chips + linkage ONLY — no exhibit `body` / table rendering (that
            // is deferred to a follow-up issue). Each chip shows the exhibit id
            // + kind label; the file/source is a hover tooltip on the chip.
            {if !m.result_exhibits.is_empty() {
                Some(view! {
                    <div class="block result-block">
                        <span class="block-label">"result"</span>
                        <div class="chip-row">
                            {m.result_exhibits.iter().map(|ex| {
                                let text = format!("{} · {}", ex.id, ex.kind);
                                let note = ex.source.clone().unwrap_or_else(|| ex.file.clone());
                                view! {
                                    <span class="chip" title=note>{text}</span>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    </div>
                })
            } else {
                None
            }}

            // ── 7. Provenance ─────────────────────────────────────────────
            {if !m.source_refs.is_empty() {
                Some(view! {
                    <div class="block provenance-block">
                        <span class="block-label">"sources"</span>
                        <div class="chip-row">
                            {m.source_refs.iter().map(|r| view! {
                                <span class="chip">{r.clone()}</span>
                            }).collect::<Vec<_>>()}
                        </div>
                    </div>
                })
            } else {
                None
            }}

            // ── Empty-node fallback ────────────────────────────────────────
            {if is_empty {
                Some(view! {
                    <p class="empty-note">"Nothing recorded for this node."</p>
                })
            } else {
                None
            }}

            // ── Inert richer blocks (T-REAL-CORPUS deferred) ──────────────
            // These CSS classes are styled in styles.css and will plug in
            // here when T-REAL-CORPUS schema widening lands.  Do NOT render
            // any of these blocks until their schema fields exist.
            //
            // Slots reserved (in published viewer order):
            //   .quote        — blockquote / pull-quote
            //   figure > img  — embedded figure image
            //   table.md      — markdown-sourced data table
            //   pre.diff      — inline diff
            //   .glossary     — term definitions
            //   .deps-list    — claim/node dependencies
            //   .recipe       — step-by-step recipes
        </div>
    }
}

/// Map a raw status string to a stable CSS suffix.
///
/// Known values: "supported", "refuted", "hypothesis".  Any unknown value
/// falls back to "neutral" so the pill still renders without a broken class.
fn status_css_class(status: Option<&str>) -> &'static str {
    match status {
        Some("supported") => "supported",
        Some("refuted") => "refuted",
        Some("hypothesis") => "hypothesis",
        _ => "neutral",
    }
}

// ── Unit tests (native — no browser required) ─────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ara_core::{
        Binding, BindingRole, BuiltOn, Claim, ClaimId, Exhibit, ExhibitKind, Manifest, Node,
        NodeExhibit, NodeFields, NodeId, NodeKind, RelatedWork,
    };

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn bare_manifest() -> Manifest {
        Manifest {
            nodes: vec![],
            links: vec![],
            bindings: vec![],
            claims: vec![],
            bounds: None,
            paper: None,
            related_work: vec![],
            concepts: vec![],
            problem: None,
            recipes: vec![],
            exhibits: vec![],
            built_on: vec![],
            node_exhibits: vec![],
        }
    }

    fn make_node(id: &str, kind: NodeKind, fields: NodeFields) -> Node {
        Node {
            id: NodeId::new(id),
            kind,
            label: None,
            support_level: None,
            source_refs: vec![],
            description: None,
            fields,
            evidence_notes: vec![],
            isolated: false,
            pos: None,
        }
    }

    // ── Decision order ────────────────────────────────────────────────────────

    /// A `Decision` node with choice + rationale + alternatives → typed_fields
    /// labels are exactly ["choice", "rationale", "alternatives"] in that order;
    /// rationale is_primary.
    #[test]
    fn decision_order_choice_rationale_alternatives() {
        let node = Node {
            fields: NodeFields::Decision {
                choice: Some("sinusoidal encoding".to_string()),
                rationale: Some("Better on long sequences.".to_string()),
                alternatives: vec!["learned".to_string(), "relative".to_string()],
            },
            ..make_node("N01", NodeKind::Decision, NodeFields::Other)
        };
        let m = detail_model(&node, &bare_manifest());
        let labels: Vec<&str> = m.typed_fields.iter().map(|f| f.label).collect();
        assert_eq!(labels, ["choice", "rationale", "alternatives"]);
        assert!(!m.typed_fields[0].is_primary, "choice is NOT primary");
        assert!(m.typed_fields[1].is_primary, "rationale IS primary");
        assert!(!m.typed_fields[2].is_primary, "alternatives is NOT primary");
    }

    /// Decision omits None choice and empty alternatives.
    #[test]
    fn decision_omits_none_choice_and_empty_alternatives() {
        let node = Node {
            fields: NodeFields::Decision {
                choice: None,
                rationale: Some("Because.".to_string()),
                alternatives: vec![],
            },
            ..make_node("N01", NodeKind::Decision, NodeFields::Other)
        };
        let m = detail_model(&node, &bare_manifest());
        let labels: Vec<&str> = m.typed_fields.iter().map(|f| f.label).collect();
        assert_eq!(labels, ["rationale"]);
    }

    // ── DeadEnd primary ──────────────────────────────────────────────────────

    /// `DeadEnd` with only the legacy `why_failed` (no `failure_mode`) → single
    /// field labeled "why failed", promoted to `is_primary == true` so the pane
    /// still has an accented block.
    #[test]
    fn dead_end_primary_why_failed() {
        let node = Node {
            fields: NodeFields::DeadEnd {
                hypothesis: None,
                failure_mode: None,
                lesson: None,
                why_failed: Some("Gradient vanished.".to_string()),
            },
            ..make_node("N02", NodeKind::DeadEnd, NodeFields::Other)
        };
        let m = detail_model(&node, &bare_manifest());
        assert_eq!(m.typed_fields.len(), 1);
        assert_eq!(m.typed_fields[0].label, "why failed");
        assert!(
            m.typed_fields[0].is_primary,
            "why failed is primary when failure_mode is absent"
        );
        assert_eq!(
            m.typed_fields[0].value,
            FieldValue::Text("Gradient vanished.".to_string())
        );
    }

    /// `DeadEnd` with all body fields `None` → no typed fields.
    #[test]
    fn dead_end_none_why_failed_no_typed_fields() {
        let node = Node {
            fields: NodeFields::DeadEnd {
                hypothesis: None,
                failure_mode: None,
                lesson: None,
                why_failed: None,
            },
            ..make_node("N02", NodeKind::DeadEnd, NodeFields::Other)
        };
        let m = detail_model(&node, &bare_manifest());
        assert!(m.typed_fields.is_empty());
    }

    /// A widened `DeadEnd` carrying hypothesis/failure_mode/lesson → typed_fields
    /// labels are exactly ["hypothesis", "failure mode", "lesson"] in that order;
    /// failure mode is_primary.
    #[test]
    fn dead_end_hypothesis_failure_mode_lesson_order() {
        let node = Node {
            fields: NodeFields::DeadEnd {
                hypothesis: Some("GPT-3.5 passes single-sample.".to_string()),
                failure_mode: Some("Low pass rate at scale.".to_string()),
                lesson: Some("Need execution validation.".to_string()),
                why_failed: None,
            },
            ..make_node("N02", NodeKind::DeadEnd, NodeFields::Other)
        };
        let m = detail_model(&node, &bare_manifest());
        let labels: Vec<&str> = m.typed_fields.iter().map(|f| f.label).collect();
        assert_eq!(labels, ["hypothesis", "failure mode", "lesson"]);
        assert!(!m.typed_fields[0].is_primary, "hypothesis is NOT primary");
        assert!(m.typed_fields[1].is_primary, "failure mode IS primary");
        assert!(!m.typed_fields[2].is_primary, "lesson is NOT primary");
    }

    /// A `Pivot` node with from/to/trigger → typed_fields labels are exactly
    /// ["from", "to", "trigger"] in that order; trigger is_primary.
    #[test]
    fn pivot_from_to_trigger_order() {
        let node = Node {
            fields: NodeFields::Pivot {
                from: Some("Full manual curation".to_string()),
                to: Some("Semi-automated pipeline".to_string()),
                trigger: Some("Manual curation infeasible at scale.".to_string()),
            },
            ..make_node("N01", NodeKind::Pivot, NodeFields::Other)
        };
        let m = detail_model(&node, &bare_manifest());
        let labels: Vec<&str> = m.typed_fields.iter().map(|f| f.label).collect();
        assert_eq!(labels, ["from", "to", "trigger"]);
        assert!(!m.typed_fields[0].is_primary, "from is NOT primary");
        assert!(!m.typed_fields[1].is_primary, "to is NOT primary");
        assert!(m.typed_fields[2].is_primary, "trigger IS primary");
        assert_eq!(
            m.typed_fields[2].value,
            FieldValue::Text("Manual curation infeasible at scale.".to_string())
        );
    }

    /// A `Pivot` node with all fields `None` → no typed fields.
    #[test]
    fn pivot_all_none_no_typed_fields() {
        let node = Node {
            fields: NodeFields::Pivot {
                from: None,
                to: None,
                trigger: None,
            },
            ..make_node("N01", NodeKind::Pivot, NodeFields::Other)
        };
        let m = detail_model(&node, &bare_manifest());
        assert!(m.typed_fields.is_empty());
    }

    // ── Experiment ────────────────────────────────────────────────────────────

    /// `Experiment { result: Some(_) }` → one typed field labelled
    /// "what it did" (the WHAT IT DID block), is_primary.
    #[test]
    fn experiment_result_present() {
        let node = Node {
            fields: NodeFields::Experiment {
                result: Some("28.4 BLEU".to_string()),
            },
            ..make_node("N03", NodeKind::Experiment, NodeFields::Other)
        };
        let m = detail_model(&node, &bare_manifest());
        assert_eq!(m.typed_fields.len(), 1);
        assert_eq!(m.typed_fields[0].label, "what it did");
        assert!(m.typed_fields[0].is_primary);
        assert_eq!(
            m.typed_fields[0].value,
            FieldValue::Text("28.4 BLEU".to_string())
        );
    }

    /// `Experiment { result: None }` → no typed fields.
    #[test]
    fn experiment_result_none_no_typed_fields() {
        let node = Node {
            fields: NodeFields::Experiment { result: None },
            ..make_node("N03", NodeKind::Experiment, NodeFields::Other)
        };
        let m = detail_model(&node, &bare_manifest());
        assert!(m.typed_fields.is_empty());
    }

    // ── Claim resolution ──────────────────────────────────────────────────────

    /// A binding to an existing claim → ClaimView with title, statement, status.
    #[test]
    fn claim_resolution_found() {
        let node_id = NodeId::new("N01");
        let claim_id = ClaimId::new("C01");

        let node = make_node(
            "N01",
            NodeKind::Experiment,
            NodeFields::Experiment { result: None },
        );

        let mut manifest = bare_manifest();
        manifest.bindings.push(Binding {
            node: node_id.clone(),
            claim: claim_id.clone(),
            role: BindingRole::Evidence,
        });
        manifest.claims.push(Claim {
            id: claim_id,
            title: "ResNet convergence".to_string(),
            statement: Some("The model converges.".to_string()),
            status: Some("refuted".to_string()),
            proof: vec![],
            deps: vec![],
        });

        let m = detail_model(&node, &manifest);
        assert_eq!(m.claims.len(), 1);
        assert_eq!(m.claims[0].title, "ResNet convergence");
        assert_eq!(
            m.claims[0].statement,
            Some("The model converges.".to_string())
        );
        assert_eq!(m.claims[0].status, Some("refuted".to_string()));
    }

    /// A binding to a missing claim id → silently skipped (no panic).
    #[test]
    fn claim_resolution_missing_claim_skipped() {
        let node = make_node("N01", NodeKind::Question, NodeFields::Question);
        let mut manifest = bare_manifest();
        manifest.bindings.push(Binding {
            node: NodeId::new("N01"),
            claim: ClaimId::new("C99"), // not in claims
            role: BindingRole::Evidence,
        });
        // No claims added.

        let m = detail_model(&node, &manifest);
        assert!(
            m.claims.is_empty(),
            "missing claim must be silently skipped"
        );
    }

    /// Bindings for a different node are ignored.
    #[test]
    fn claim_resolution_filters_to_selected_node() {
        let node = make_node("N01", NodeKind::Question, NodeFields::Question);
        let mut manifest = bare_manifest();
        // Binding for a different node
        manifest.bindings.push(Binding {
            node: NodeId::new("N02"),
            claim: ClaimId::new("C01"),
            role: BindingRole::Evidence,
        });
        manifest.claims.push(Claim {
            id: ClaimId::new("C01"),
            title: "Other node claim".to_string(),
            statement: None,
            status: None,
            proof: vec![],
            deps: vec![],
        });

        let m = detail_model(&node, &manifest);
        assert!(m.claims.is_empty());
    }

    // ── Degradation ───────────────────────────────────────────────────────────

    /// A Question with no description, no evidence, no bindings, no source_refs
    /// → `is_empty() == true`.
    #[test]
    fn question_with_nothing_is_empty() {
        let node = make_node("N01", NodeKind::Question, NodeFields::Question);
        let m = detail_model(&node, &bare_manifest());
        assert!(m.is_empty());
    }

    /// A node with a description but no typed fields → `is_empty() == false`,
    /// `typed_fields` empty.
    #[test]
    fn node_with_description_not_empty_but_typed_fields_absent() {
        let node = Node {
            description: Some("Overarching question.".to_string()),
            ..make_node("N01", NodeKind::Question, NodeFields::Question)
        };
        let m = detail_model(&node, &bare_manifest());
        assert!(!m.is_empty());
        assert!(m.typed_fields.is_empty());
    }

    // ── Title resolution ──────────────────────────────────────────────────────

    #[test]
    fn title_prefers_label_over_id() {
        let node = Node {
            label: Some("My Question".to_string()),
            ..make_node("N01", NodeKind::Question, NodeFields::Question)
        };
        let m = detail_model(&node, &bare_manifest());
        assert_eq!(m.title, "My Question");
    }

    #[test]
    fn title_falls_back_to_id() {
        let node = make_node("N01", NodeKind::Question, NodeFields::Question);
        let m = detail_model(&node, &bare_manifest());
        assert_eq!(m.title, "N01");
    }

    // ── Other kind ────────────────────────────────────────────────────────────

    /// `Other("weird")` → `kind_badge == "weird"`, no typed fields.
    #[test]
    fn other_kind_badge_is_raw_string_no_typed_fields() {
        let node = make_node(
            "N01",
            NodeKind::Other("weird".to_string()),
            NodeFields::Other,
        );
        let m = detail_model(&node, &bare_manifest());
        assert_eq!(m.kind_badge, "weird");
        assert!(m.typed_fields.is_empty());
    }

    // ── Support level ─────────────────────────────────────────────────────────

    #[test]
    fn support_level_propagated() {
        let node = Node {
            support_level: Some("inferred".to_string()),
            ..make_node("N01", NodeKind::Question, NodeFields::Question)
        };
        let m = detail_model(&node, &bare_manifest());
        assert_eq!(m.support_level, Some("inferred".to_string()));
    }

    // ── Source refs ───────────────────────────────────────────────────────────

    #[test]
    fn source_refs_propagated() {
        let node = Node {
            source_refs: vec!["§1".to_string(), "Fig. 3".to_string()],
            ..make_node("N01", NodeKind::Question, NodeFields::Question)
        };
        let m = detail_model(&node, &bare_manifest());
        assert_eq!(m.source_refs, vec!["§1", "Fig. 3"]);
        assert!(!m.is_empty(), "source_refs alone makes it non-empty");
    }

    // ── Evidence notes ────────────────────────────────────────────────────────

    #[test]
    fn evidence_notes_propagated() {
        let node = Node {
            evidence_notes: vec!["Table 2".to_string()],
            ..make_node(
                "N01",
                NodeKind::Experiment,
                NodeFields::Experiment { result: None },
            )
        };
        let m = detail_model(&node, &bare_manifest());
        assert_eq!(m.evidence_notes, vec!["Table 2"]);
        assert!(!m.is_empty());
    }

    // ── Alternatives rendered as List ─────────────────────────────────────────

    #[test]
    fn decision_alternatives_render_as_list() {
        let node = Node {
            fields: NodeFields::Decision {
                choice: None,
                rationale: None,
                alternatives: vec!["A".to_string(), "B".to_string()],
            },
            ..make_node("N01", NodeKind::Decision, NodeFields::Other)
        };
        let m = detail_model(&node, &bare_manifest());
        assert_eq!(m.typed_fields.len(), 1);
        assert_eq!(m.typed_fields[0].label, "alternatives");
        assert_eq!(
            m.typed_fields[0].value,
            FieldValue::List(vec!["A".to_string(), "B".to_string()])
        );
    }

    // ── Built on + Result linkage ─────────────────────────────────────────────

    /// A node with `built_on` + `node_exhibits` resolves both in source order:
    /// built_on carries id + cite, result_exhibits carries id/file/kind.
    #[test]
    fn built_on_and_result_exhibits_resolve_in_source_order() {
        let node = make_node(
            "N01",
            NodeKind::Experiment,
            NodeFields::Experiment { result: None },
        );

        let mut manifest = bare_manifest();
        manifest.related_work = vec![
            RelatedWork {
                id: "RW01".to_string(),
                cite: "Vaswani et al., 2017".to_string(),
                doi: None,
                kind: None,
                what_changed: None,
                why: None,
                adopted: None,
                claims_affected: vec![],
            },
            RelatedWork {
                id: "RW02".to_string(),
                cite: "He et al., 2016".to_string(),
                doi: None,
                kind: None,
                what_changed: None,
                why: None,
                adopted: None,
                claims_affected: vec![],
            },
        ];
        // Source order RW02 then RW01 — must be preserved.
        manifest.built_on = vec![
            BuiltOn {
                node: NodeId::new("N01"),
                related_work: "RW02".to_string(),
            },
            BuiltOn {
                node: NodeId::new("N01"),
                related_work: "RW01".to_string(),
            },
        ];
        manifest.exhibits = vec![
            Exhibit {
                id: "E01".to_string(),
                file: "evidence/fig1.md".to_string(),
                kind: ExhibitKind::Figure,
                source: Some("Fig. 1".to_string()),
                description: None,
                claims: vec![],
                body: String::new(),
            },
            Exhibit {
                id: "T01".to_string(),
                file: "evidence/tab1.md".to_string(),
                kind: ExhibitKind::Table,
                source: None,
                description: None,
                claims: vec![],
                body: String::new(),
            },
        ];
        manifest.node_exhibits = vec![
            NodeExhibit {
                node: NodeId::new("N01"),
                exhibit: "T01".to_string(),
            },
            NodeExhibit {
                node: NodeId::new("N01"),
                exhibit: "E01".to_string(),
            },
        ];

        let m = detail_model(&node, &manifest);

        // built_on: source order RW02, RW01 with resolved cites.
        assert_eq!(m.built_on.len(), 2);
        assert_eq!(m.built_on[0].id, "RW02");
        assert_eq!(m.built_on[0].cite, "He et al., 2016");
        assert_eq!(m.built_on[1].id, "RW01");
        assert_eq!(m.built_on[1].cite, "Vaswani et al., 2017");

        // result_exhibits: source order T01 (table), E01 (figure).
        assert_eq!(m.result_exhibits.len(), 2);
        assert_eq!(m.result_exhibits[0].id, "T01");
        assert_eq!(m.result_exhibits[0].file, "evidence/tab1.md");
        assert_eq!(m.result_exhibits[0].kind, "table");
        assert_eq!(m.result_exhibits[0].source, None);
        assert_eq!(m.result_exhibits[1].id, "E01");
        assert_eq!(m.result_exhibits[1].kind, "figure");
        assert_eq!(m.result_exhibits[1].source, Some("Fig. 1".to_string()));

        assert!(!m.is_empty(), "built_on + exhibits make it non-empty");
    }

    /// Only edges for the selected node are resolved; other nodes' edges ignored.
    #[test]
    fn built_on_and_exhibits_filter_to_selected_node() {
        let node = make_node("N01", NodeKind::Question, NodeFields::Question);
        let mut manifest = bare_manifest();
        manifest.related_work = vec![RelatedWork {
            id: "RW01".to_string(),
            cite: "Other, 2020".to_string(),
            doi: None,
            kind: None,
            what_changed: None,
            why: None,
            adopted: None,
            claims_affected: vec![],
        }];
        manifest.built_on = vec![BuiltOn {
            node: NodeId::new("N02"), // different node
            related_work: "RW01".to_string(),
        }];
        manifest.exhibits = vec![Exhibit {
            id: "E01".to_string(),
            file: "evidence/fig1.md".to_string(),
            kind: ExhibitKind::Figure,
            source: None,
            description: None,
            claims: vec![],
            body: String::new(),
        }];
        manifest.node_exhibits = vec![NodeExhibit {
            node: NodeId::new("N02"), // different node
            exhibit: "E01".to_string(),
        }];

        let m = detail_model(&node, &manifest);
        assert!(m.built_on.is_empty());
        assert!(m.result_exhibits.is_empty());
    }

    /// An unresolvable RW id keeps the id with an empty cite (no panic); an
    /// unresolvable exhibit id is skipped.
    #[test]
    fn unresolvable_ids_degrade_gracefully() {
        let node = make_node("N01", NodeKind::Question, NodeFields::Question);
        let mut manifest = bare_manifest();
        // built_on points at RW99, which is not in related_work.
        manifest.built_on = vec![BuiltOn {
            node: NodeId::new("N01"),
            related_work: "RW99".to_string(),
        }];
        // node_exhibits points at E99, which is not in exhibits.
        manifest.node_exhibits = vec![NodeExhibit {
            node: NodeId::new("N01"),
            exhibit: "E99".to_string(),
        }];

        let m = detail_model(&node, &manifest);
        // RW99 still shown, with empty cite.
        assert_eq!(m.built_on.len(), 1);
        assert_eq!(m.built_on[0].id, "RW99");
        assert_eq!(m.built_on[0].cite, "");
        // E99 skipped silently.
        assert!(m.result_exhibits.is_empty());
    }

    /// A node with no linkage yields empty vecs.
    #[test]
    fn no_linkage_yields_empty_vecs() {
        let node = make_node("N01", NodeKind::Question, NodeFields::Question);
        let m = detail_model(&node, &bare_manifest());
        assert!(m.built_on.is_empty());
        assert!(m.result_exhibits.is_empty());
    }

    /// `is_empty` accounts for built_on alone and result_exhibits alone.
    #[test]
    fn is_empty_accounts_for_new_fields() {
        // built_on alone → not empty.
        let node = make_node("N01", NodeKind::Question, NodeFields::Question);
        let mut m1 = bare_manifest();
        m1.built_on = vec![BuiltOn {
            node: NodeId::new("N01"),
            related_work: "RW01".to_string(),
        }];
        assert!(!detail_model(&node, &m1).is_empty());

        // result_exhibits alone → not empty.
        let mut m2 = bare_manifest();
        m2.exhibits = vec![Exhibit {
            id: "E01".to_string(),
            file: "evidence/fig1.md".to_string(),
            kind: ExhibitKind::Figure,
            source: None,
            description: None,
            claims: vec![],
            body: String::new(),
        }];
        m2.node_exhibits = vec![NodeExhibit {
            node: NodeId::new("N01"),
            exhibit: "E01".to_string(),
        }];
        assert!(!detail_model(&node, &m2).is_empty());
    }
}

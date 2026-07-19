//! Context, Glossary, and Recipes panels — three more consumers of the shared
//! [`Modal`], built on the same shape as [`crate::deps::DependenciesPanel`]: a
//! header launcher with a live count (hidden when empty) that opens a filtered
//! list in a [`Modal`].
//!
//! LaTeX in concept/recipe text is rendered as **inert monospace** (`$…$` kept
//! verbatim inside a `<code>`), never interpreted — a real math renderer is
//! deferred (D3). The pure helpers ([`latex_segments`], [`concept_matches`],
//! [`recipe_matches`]) are native-testable.

use ara_core::{Concept, Problem, Recipe};
use leptos::prelude::*;

use crate::modal::Modal;
use crate::state::LoadState;

// ── Inert-LaTeX splitting (D3) ────────────────────────────────────────────────

/// A slice of text: plain prose, or an inert `$…$` LaTeX span (kept verbatim).
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Plain(String),
    /// The full `$…$` span, delimiters included, rendered as monospace.
    Latex(String),
}

/// Split `s` into alternating plain / inert-`$…$` segments. An unbalanced `$`
/// (no closing delimiter) leaves the remainder as plain text. The math is never
/// interpreted — this only marks spans so the view can render them monospace.
pub fn latex_segments(s: &str) -> Vec<Segment> {
    let mut segs = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find('$') {
        if open > 0 {
            segs.push(Segment::Plain(rest[..open].to_string()));
        }
        let after = &rest[open + 1..];
        match after.find('$') {
            Some(close) => {
                segs.push(Segment::Latex(format!("${}$", &after[..close])));
                rest = &after[close + 1..];
            }
            None => {
                // Unbalanced: the rest, from the lone `$`, is plain text.
                segs.push(Segment::Plain(rest[open..].to_string()));
                rest = "";
                break;
            }
        }
    }
    if !rest.is_empty() {
        segs.push(Segment::Plain(rest.to_string()));
    }
    segs
}

/// Render a string with inert-monospace spans for any `$…$` LaTeX. The returned
/// view owns its data (`use<>` — captures no borrow of `s`).
fn latex_view(s: &str) -> impl IntoView + use<> {
    latex_segments(s)
        .into_iter()
        .map(|seg| match seg {
            Segment::Plain(t) => t.into_any(),
            Segment::Latex(t) => view! { <code class="latex-inert">{t}</code> }.into_any(),
        })
        .collect::<Vec<_>>()
}

// ── Context panel (logic/problem.md) ──────────────────────────────────────────

/// The Context launcher + modal. Present only when the manifest carries a
/// `problem`. Unlike the counted panels, Context shows no numeric badge (there
/// is one problem framing, not a countable set) — it matches the hub's "—".
#[component]
pub fn ContextPanel(load_state: ReadSignal<LoadState>) -> impl IntoView {
    let open = RwSignal::new(false);
    let query = RwSignal::new(String::new());
    let problem = Memo::new(move |_| match load_state.get() {
        LoadState::Loaded(m) => m.problem.clone(),
        _ => None,
    });

    view! {
        {move || problem.get().is_some().then(|| view! {
            <button
                type="button"
                class="btn panel-launch-btn"
                on:click=move |_| open.update(|o| *o = !*o)
            >
                "Context"
            </button>
        })}

        <Modal open=open title="Context">
            <input
                class="panel-filter"
                type="text"
                placeholder="filter\u{2026}"
                aria-label="Filter context"
                prop:value=move || query.get()
                on:input=move |ev| query.set(event_target_value(&ev))
            />
            {move || {
                let Some(p) = problem.get() else {
                    return view! { <p class="rw-empty">"No context."</p> }.into_any();
                };
                let q = query.get().trim().to_lowercase();
                render_context(&p, &q).into_any()
            }}
        </Modal>
    }
}

/// Filter a list of items to those containing `q` (already lowercased); an empty
/// query keeps all.
fn filter_items(items: &[String], q: &str) -> Vec<String> {
    items
        .iter()
        .filter(|i| q.is_empty() || i.to_lowercase().contains(q))
        .cloned()
        .collect()
}

fn render_context(p: &Problem, q: &str) -> impl IntoView {
    let statement = p
        .statement
        .clone()
        .filter(|s| q.is_empty() || s.to_lowercase().contains(q));
    let observations = filter_items(&p.observations, q);
    let gaps = filter_items(&p.gaps, q);
    let insights = filter_items(&p.insights, q);

    view! {
        {statement.map(|s| view! { <p class="context-statement">{s}</p> })}
        {section("Observations", observations)}
        {section("Gaps", gaps)}
        {section("Key Insight", insights)}
    }
}

/// One labelled list section, omitted entirely when empty.
fn section(label: &'static str, items: Vec<String>) -> Option<impl IntoView> {
    (!items.is_empty()).then(|| {
        view! {
            <div class="block context-section">
                <span class="block-label">{label}</span>
                <ul class="context-list">
                    {items.into_iter().map(|i| view! { <li>{i}</li> }).collect::<Vec<_>>()}
                </ul>
            </div>
        }
    })
}

// ── Glossary panel (logic/concepts.md) ────────────────────────────────────────

/// Case-insensitive match of `query` against a concept's term/notation/
/// definition/boundary and its related-term list. Empty query matches all.
pub fn concept_matches(c: &Concept, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    let mut hay = c.term.clone();
    for s in [&c.notation, &c.definition, &c.boundary]
        .into_iter()
        .flatten()
    {
        hay.push(' ');
        hay.push_str(s);
    }
    for r in &c.related {
        hay.push(' ');
        hay.push_str(r);
    }
    hay.to_lowercase().contains(&q)
}

/// The Glossary launcher + modal. Count = number of concept (`## Term`) blocks.
#[component]
pub fn GlossaryPanel(load_state: ReadSignal<LoadState>) -> impl IntoView {
    let open = RwSignal::new(false);
    let query = RwSignal::new(String::new());
    let concepts = Memo::new(move |_| match load_state.get() {
        LoadState::Loaded(m) => m.concepts.clone(),
        _ => Vec::new(),
    });

    view! {
        {move || {
            let count = concepts.get().len();
            (count > 0).then(|| view! {
                <button
                    type="button"
                    class="btn panel-launch-btn"
                    on:click=move |_| open.update(|o| *o = !*o)
                >
                    "Glossary"
                    <span class="launch-count">{count}</span>
                </button>
            })
        }}

        <Modal open=open title="Glossary">
            <input
                class="panel-filter"
                type="text"
                placeholder="filter\u{2026}"
                aria-label="Filter glossary"
                prop:value=move || query.get()
                on:input=move |ev| query.set(event_target_value(&ev))
            />
            {move || {
                let q = query.get();
                let items: Vec<Concept> = concepts
                    .get()
                    .into_iter()
                    .filter(|c| concept_matches(c, &q))
                    .collect();
                if items.is_empty() {
                    view! { <p class="rw-empty">"No terms match the filter."</p> }.into_any()
                } else {
                    view! {
                        <div class="rw-list">
                            {items.into_iter().map(concept_entry).collect::<Vec<_>>()}
                        </div>
                    }
                    .into_any()
                }
            }}
        </Modal>
    }
}

/// Render one concept as a `.block` card with inert-LaTeX text and dotted
/// cross-reference chips for its related terms.
///
/// The hub also shows a `mentions N07 N08…` node-chip row, but our data model
/// carries no concept→node linkage, so that row is intentionally omitted rather
/// than fabricated.
fn concept_entry(c: Concept) -> impl IntoView {
    view! {
        <div class="block concept-entry">
            <div class="concept-term">{c.term.clone()}</div>
            {c.notation.clone().map(|n| view! {
                <div class="rw-line"><span class="rw-key">"Notation"</span>
                    <span>{latex_view(&n)}</span></div>
            })}
            {c.definition.clone().map(|d| view! {
                <div class="rw-line"><span class="rw-key">"Definition"</span>
                    <span>{latex_view(&d)}</span></div>
            })}
            {c.boundary.clone().map(|b| view! {
                <div class="rw-line"><span class="rw-key">"Boundary"</span>
                    <span>{latex_view(&b)}</span></div>
            })}
            {(!c.related.is_empty()).then(|| view! {
                <div class="rw-line"><span class="rw-key">"Related"</span>
                    <div class="chip-row">
                        {c.related.iter().map(|r| view! {
                            <span class="chip concept-xref">{r.clone()}</span>
                        }).collect::<Vec<_>>()}
                    </div>
                </div>
            })}
        </div>
    }
}

// ── Recipes panel (logic/solution/*.md) ───────────────────────────────────────

/// Case-insensitive match of `query` against a recipe's name/title/body.
pub fn recipe_matches(r: &Recipe, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    let mut hay = r.name.clone();
    if let Some(t) = &r.title {
        hay.push(' ');
        hay.push_str(t);
    }
    hay.push(' ');
    hay.push_str(&r.body);
    hay.to_lowercase().contains(&q)
}

/// The "Solution files" launcher + modal. Count = number of `logic/solution/*.md`
/// files, one entry per file. The label is deliberately a file count, not a
/// "recipe" count: per ARA-Labs/ara-cli#35 the maintainer deferred defining a
/// canonical "recipe" unit, so the panel names what it actually counts. The
/// internal `RecipesPanel` / `recipes` names stay until that unit is defined.
#[component]
pub fn RecipesPanel(load_state: ReadSignal<LoadState>) -> impl IntoView {
    let open = RwSignal::new(false);
    let query = RwSignal::new(String::new());
    let recipes = Memo::new(move |_| match load_state.get() {
        LoadState::Loaded(m) => m.recipes.clone(),
        _ => Vec::new(),
    });

    view! {
        {move || {
            let count = recipes.get().len();
            (count > 0).then(|| view! {
                <button
                    type="button"
                    class="btn panel-launch-btn"
                    on:click=move |_| open.update(|o| *o = !*o)
                >
                    "Solution files"
                    <span class="launch-count">{count}</span>
                </button>
            })
        }}

        <Modal open=open title="Solution files">
            <input
                class="panel-filter"
                type="text"
                placeholder="filter\u{2026}"
                aria-label="Filter solution files"
                prop:value=move || query.get()
                on:input=move |ev| query.set(event_target_value(&ev))
            />
            {move || {
                let q = query.get();
                let items: Vec<Recipe> = recipes
                    .get()
                    .into_iter()
                    .filter(|r| recipe_matches(r, &q))
                    .collect();
                if items.is_empty() {
                    view! { <p class="rw-empty">"No solution files match the filter."</p> }.into_any()
                } else {
                    view! {
                        <div class="rw-list">
                            {items.into_iter().map(recipe_entry).collect::<Vec<_>>()}
                        </div>
                    }
                    .into_any()
                }
            }}
        </Modal>
    }
}

/// Render one recipe: heading + the raw body as preformatted text with inert
/// LaTeX. Body markdown/tables are NOT rendered (D4); the `<pre>` scrolls
/// horizontally so wide content can't overflow the modal.
fn recipe_entry(r: Recipe) -> impl IntoView {
    let heading = r.title.clone().unwrap_or_else(|| r.name.clone());
    view! {
        <div class="block recipe-entry">
            <div class="concept-term">{heading}</div>
            <pre class="recipe-body">{latex_view(&r.body)}</pre>
        </div>
    }
}

// ── Unit tests (native — no browser required) ─────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latex_split_plain_only() {
        assert_eq!(
            latex_segments("just prose"),
            vec![Segment::Plain("just prose".into())]
        );
    }

    #[test]
    fn latex_split_marks_spans_verbatim() {
        let segs = latex_segments("policy $\\pi^{(k)}$ over states");
        assert_eq!(
            segs,
            vec![
                Segment::Plain("policy ".into()),
                Segment::Latex("$\\pi^{(k)}$".into()),
                Segment::Plain(" over states".into()),
            ]
        );
    }

    #[test]
    fn latex_split_unbalanced_dollar_is_plain() {
        // A lone `$` with no closing delimiter stays plain — never swallowed.
        assert_eq!(
            latex_segments("costs $5 total"),
            vec![
                Segment::Plain("costs ".into()),
                Segment::Plain("$5 total".into()),
            ]
        );
    }

    #[test]
    fn latex_split_two_spans() {
        let segs = latex_segments("$a$ and $b$");
        assert_eq!(
            segs,
            vec![
                Segment::Latex("$a$".into()),
                Segment::Plain(" and ".into()),
                Segment::Latex("$b$".into()),
            ]
        );
    }

    fn concept(term: &str) -> Concept {
        Concept {
            term: term.to_string(),
            notation: None,
            definition: None,
            boundary: None,
            related: vec![],
        }
    }

    #[test]
    fn concept_match_empty_and_fields() {
        let mut c = concept("Self-Composing Policy");
        c.definition = Some("a policy $\\pi$ composed of modules".into());
        c.related = vec!["ProgressiveNet".into()];
        assert!(concept_matches(&c, ""));
        assert!(concept_matches(&c, "self-composing"));
        assert!(concept_matches(&c, "modules"));
        assert!(concept_matches(&c, "progressivenet"));
        assert!(!concept_matches(&c, "transformer"));
    }

    fn recipe(name: &str) -> Recipe {
        Recipe {
            name: name.to_string(),
            title: None,
            body: String::new(),
        }
    }

    #[test]
    fn recipe_match_name_title_body() {
        let r = Recipe {
            title: Some("Composition Algorithm".into()),
            body: "for each module compute attention".into(),
            ..recipe("algorithm")
        };
        assert!(recipe_matches(&r, ""));
        assert!(recipe_matches(&r, "algorithm"));
        assert!(recipe_matches(&r, "composition"));
        assert!(recipe_matches(&r, "attention"));
        assert!(!recipe_matches(&r, "pruning"));
    }
}

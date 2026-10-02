#![cfg(target_arch = "wasm32")]

use ara_viewer::{
    math::{MathText, Segment, segments},
    panels::{GlossaryPanel, RecipesPanel},
    state::{LoadState, parse_manifest},
};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::HtmlElement;

mod math_support;
use math_support::*;

wasm_bindgen_test_configure!(run_in_browser);

fn manifest(notation: &str, body: &str) -> ara_core::Manifest {
    let mut manifest = parse_manifest(r#"{"nodes":[],"links":[],"bindings":[],"claims":[],"bounds":null,"paper":null,"related_work":[],"concepts":[],"problem":null,"recipes":[],"exhibits":[],"built_on":[],"node_exhibits":[]}"#).unwrap();
    manifest.concepts = vec![ara_core::Concept {
        term: "Current concept".into(),
        notation: Some(notation.into()),
        definition: Some("unchanged definition".into()),
        boundary: None,
        related: vec![],
    }];
    manifest.recipes = vec![ara_core::Recipe {
        name: "algorithm".into(),
        title: Some("Algorithm source".into()),
        body: body.into(),
    }];
    manifest
}

fn element(root: &HtmlElement, selector: &str) -> web_sys::Element {
    root.query_selector(selector).unwrap().unwrap_or_else(|| panic!("missing {selector}: {}", root.inner_text()))
}

fn source_visible(root: &HtmlElement, source: &str) {
    let fallback = element(root, ".panel-math-source");
    assert_eq!(fallback.text_content().as_deref(), Some(source));
    assert!(!fallback.has_attribute("hidden"), "original source must remain visible");
}

#[wasm_bindgen_test]
async fn math_solution_real_aligned_cases_matrix_and_raw_source() {
    let runtime = setup("normal").await;
    let doc = web_sys::window().unwrap().document().unwrap();
    let root = container(&doc);
    let source = "# Raw **Markdown**\nπ before $\\pi^{(k)}$ and $\\Phi^{k;s}$ after.\n$$\\begin{aligned}a&=b\\\\c&=d\\end{aligned}$$\n$$f(x)=\\begin{cases}x&x>0\\\\0&x\\le0\\end{cases}$$\n$$\\begin{bmatrix}1&2\\\\3&4\\end{bmatrix}$$\n```python\nprice = '$5'\nprint('$$literal$$')\n```\n    $indented$\n`$inline code$`\n<script>not executed</script>\n";
    let (load_state, _) = signal(LoadState::Loaded(manifest("$x$", source)));
    let handle = leptos::mount::mount_to(root.clone(), move || view! { <RecipesPanel load_state=load_state /> });
    leptos::task::tick().await;
    assert_eq!(runtime.requests(), 0, "closed panels must not load math assets");
    click(&root, ".panel-launch-btn");
    settle(&root).await;
    let body = element(&root, ".recipe-body").unchecked_into::<HtmlElement>();
    let plain = segments(source).into_iter().filter_map(|segment| match segment { Segment::Plain(text) => Some(text), _ => None }).collect::<String>();
    assert_eq!(math_plain_source(&body), plain, "all non-math source remains byte-for-byte raw text");
    assert_eq!(body.query_selector_all(".panel-math-inline math msup").unwrap().length(), 2);
    let displays = body.query_selector_all(".panel-math-display math[display='block']").unwrap();
    assert_eq!(displays.length(), 3);
    for i in 0..3 {
        let display = displays.item(i).unwrap().unchecked_into::<web_sys::Element>();
        assert_eq!(display.query_selector_all("mtable mtr").unwrap().length(), 2, "aligned/cases/matrix has two semantic rows");
    }
    assert!(math_aligned_rows_are_distinct(&body), "real HTML aligned rows occupy distinct lines");
    assert!(math_fonts_loaded(), "the rendered equations use a genuinely loaded font");
    assert!(body.query_selector("script, pre, code").unwrap().is_none(), "raw Markdown and HTML are not interpreted");
    assert_eq!(runtime.requests(), 2, "one script and one stylesheet load");
    drop(handle);
    root.remove();
}

#[wasm_bindgen_test]
async fn math_malformed_expression_is_local_and_diagnostics_are_escaped() {
    let _runtime = setup("normal").await;
    let doc = web_sys::window().unwrap().document().unwrap();
    let root = container(&doc);
    let source = "$x^2$ then $\\frac{1}{$ then $y^3$ then $\\unknown{<img src=x onerror=alert(1)>}$";
    let handle = leptos::mount::mount_to(root.clone(), move || view! { <MathText text=source.to_owned() /> });
    settle(&root).await;
    assert_eq!(root.query_selector_all("math msup").unwrap().length(), 2, "valid neighbors still typeset");
    let fragments = root.query_selector_all(".panel-math").unwrap();
    for i in [1, 3] {
        let fragment = fragments.item(i).unwrap().unchecked_into::<HtmlElement>();
        assert!(fragment.inner_text().contains("Could not render equation."));
        assert!(fragment.query_selector(".katex").unwrap().is_none());
    }
    let malformed = fragments.item(1).unwrap().unchecked_into::<HtmlElement>();
    source_visible(&malformed, "$\\frac{1}{$");
    let malicious = fragments.item(3).unwrap().unchecked_into::<HtmlElement>();
    source_visible(&malicious, "$\\unknown{<img src=x onerror=alert(1)>}$");
    assert!(root.query_selector("img, script").unwrap().is_none(), "source/diagnostic HTML stays escaped");
    drop(handle);
    root.remove();
}

#[wasm_bindgen_test]
async fn math_security_blocks_trust_commands_aliases_and_macro_pollution() {
    let _runtime = setup("normal").await;
    let doc = web_sys::window().unwrap().document().unwrap();
    let root = container(&doc);
    let expressions = [
        r"$\href{javascript:alert(1)}{click}$",
        r"$\url{https://external.invalid/link}$",
        r"$\includegraphics{https://external.invalid/image.png}$",
        r"$\htmlStyle{background-image:url(https://external.invalid/style.png)}{x}$",
        r"$\htmlClass{artifact-controlled}{x}$",
        r"$\htmlId{artifact-controlled}{x}$",
        r"$\htmlData{artifact=controlled}{x}$",
        r"$\def\alias{\href}\alias{javascript:alert(1)}{click}$",
        r"$\def\recur{\recur}\recur$",
        r"$\gdef\poison{P}\poison$",
        r"$\poison$",
        r"$z^2$",
        r"$\html@mathml{\href{javascript:alert(1)}{click}}{x}$",
        r"$\html@mathml{\htmlStyle{color:red}{x}}{x}$",
        r"$\textcolor{#cc0000}{\text{\textbackslash href}}$",
        r"$\href{1:bad}{click}$",
        r"$\html@mathml{\href{1:bad}{click}}{x}$",
        r"$\html@mathml{x}{\html@mathml{\href{javascript:alert(1)}{click}}{y}}$",
        r"$\html@mathml{x}{\html@mathml{\href{1:bad}{click}}{y}}$",
    ];
    let source = expressions.join(" between ");
    let handle = leptos::mount::mount_to(root.clone(), move || view! { <MathText text=source.clone() /> });
    settle(&root).await;
    let fragments = root.query_selector_all(".panel-math").unwrap();
    assert_eq!(fragments.length(), expressions.len() as u32);
    for i in (0..9).chain(std::iter::once(10)).chain(12..14).chain(15..19) {
        let fragment = fragments.item(i).unwrap().unchecked_into::<HtmlElement>();
        source_visible(&fragment, expressions[i as usize]);
        assert!(fragment.inner_text().contains("Could not render equation."), "rejected {}", expressions[i as usize]);
        assert!(fragment.query_selector(".katex").unwrap().is_none(), "blocked commands cannot be counted as successful");
    }
    assert!(fragments.item(9).unwrap().unchecked_into::<web_sys::Element>().query_selector("math").unwrap().is_some(), "a local macro works within its own expression");
    assert_eq!(root.query_selector_all("math msup").unwrap().length(), 1, "fresh macros protect the final valid neighbor");
    assert_eq!(
        fragments.item(14).unwrap().unchecked_into::<web_sys::Element>()
            .query_selector("math mtext").unwrap().unwrap().text_content().as_deref(),
        Some("\\href"),
        "a colored literal is not a trust-sensitive command",
    );
    assert!(root.query_selector("a, img, script, #artifact-controlled, .artifact-controlled, [data-artifact]").unwrap().is_none());
    assert_eq!(math_external_requests(), 0, "artifact input causes no external asset requests");
    drop(handle);
    root.remove();
}

#[wasm_bindgen_test]
async fn math_finite_size_and_local_display_scroll_at_narrow_width() {
    let _runtime = setup("normal").await;
    let doc = web_sys::window().unwrap().document().unwrap();
    let root = container(&doc);
    let body = format!("$$ {} $$\n$\\rule{{999em}}{{999em}}$", std::iter::repeat_n("a+b", 100).collect::<Vec<_>>().join("+"));
    let (load_state, _) = signal(LoadState::Loaded(manifest("$x$", &body)));
    let handle = leptos::mount::mount_to(root.clone(), move || view! { <RecipesPanel load_state=load_state /> });
    click(&root, ".panel-launch-btn");
    leptos::task::tick().await;
    element(&root, ".modal").set_attribute("style", "width:375px;max-width:375px").unwrap();
    settle(&root).await;
    assert!(math_has_local_overflow(&root), "long display scrolls locally without widening the modal");
    assert!(math_rule_is_bounded(&root), "maxSize caps rules at 20em");
    drop(handle);
    root.remove();
}

#[wasm_bindgen_test]
async fn math_filter_and_reopen_cancel_pending_jobs_and_reuse_renderer() {
    let runtime = setup("delay").await;
    let doc = web_sys::window().unwrap().document().unwrap();
    let root = container(&doc);
    let (load_state, _) = signal(LoadState::Loaded(manifest("$\\pi^2$", "raw source")));
    let handle = leptos::mount::mount_to(root.clone(), move || view! { <GlossaryPanel load_state=load_state /> });
    click(&root, ".panel-launch-btn");
    leptos::task::tick().await;
    let removed = element(&root, ".concept-entry");
    source_visible(&removed.clone().unchecked_into::<HtmlElement>(), "$\\pi^2$");
    filter(&root, "no-match").await;
    assert!(root.query_selector(".concept-entry").unwrap().is_none());
    runtime.release();
    sleep(100).await;
    assert!(removed.query_selector(".katex, .panel-math-diagnostic").unwrap().is_none(), "disposed entry never receives output or errors");
    filter(&root, "").await;
    settle(&root).await;
    assert_eq!(element(&root, "math msup mi").text_content().as_deref(), Some("π"));
    click(&root, ".modal-close");
    leptos::task::tick().await;
    assert!(root.query_selector(".modal").unwrap().is_none());
    click(&root, ".panel-launch-btn");
    settle(&root).await;
    assert_eq!(element(&root, "math msup mi").text_content().as_deref(), Some("π"));
    assert_eq!(runtime.requests(), 2, "reopened fragments reuse shared loaded assets");
    drop(handle);
    root.remove();
}

#[wasm_bindgen_test]
async fn math_manifest_replacement_during_loading_renders_only_current_text() {
    let runtime = setup("delay").await;
    let doc = web_sys::window().unwrap().document().unwrap();
    let root = container(&doc);
    let (load_state, set_load_state) = signal(LoadState::Loaded(manifest("$\\pi^2$", "raw source")));
    let handle = leptos::mount::mount_to(root.clone(), move || view! { <GlossaryPanel load_state=load_state /> });
    click(&root, ".panel-launch-btn");
    leptos::task::tick().await;
    set_load_state.set(LoadState::Loaded(manifest("$\\Phi^3$", "new source")));
    leptos::task::tick().await;
    source_visible(&root, "$\\Phi^3$");
    runtime.release();
    settle(&root).await;
    assert_eq!(element(&root, "math msup mi").text_content().as_deref(), Some("Φ"));
    let annotations = root.query_selector_all("math annotation").unwrap();
    let rendered: Vec<_> = (0..annotations.length())
        .map(|i| annotations.item(i).unwrap().text_content().unwrap())
        .collect();
    assert_eq!(rendered, ["\\Phi^3"], "no stale equation survives manifest replacement");
    assert_eq!(runtime.requests(), 2);
    drop(handle);
    root.remove();
}

#[wasm_bindgen_test]
async fn math_close_during_loading_and_closed_manifest_replacement_are_safe() {
    let runtime = setup("delay").await;
    let doc = web_sys::window().unwrap().document().unwrap();
    let root = container(&doc);
    let (load_state, set_load_state) = signal(LoadState::Loaded(manifest("$\\pi^2$", "raw source")));
    let handle = leptos::mount::mount_to(root.clone(), move || view! { <GlossaryPanel load_state=load_state /> });
    click(&root, ".panel-launch-btn");
    leptos::task::tick().await;
    let removed = element(&root, ".modal");
    click(&root, ".modal-close");
    leptos::task::tick().await;
    set_load_state.set(LoadState::Loaded(manifest("$z^4$", "replacement")));
    runtime.release();
    sleep(100).await;
    assert!(root.query_selector(".modal").unwrap().is_none());
    assert!(removed.query_selector(".katex, .panel-math-diagnostic").unwrap().is_none());
    click(&root, ".panel-launch-btn");
    settle(&root).await;
    assert_eq!(element(&root, "math annotation").text_content().as_deref(), Some("z^4"));
    assert_eq!(runtime.requests(), 2);
    drop(handle);
    root.remove();
}

#[wasm_bindgen_test]
async fn math_lazy_loading_is_shared_by_both_panels_and_skips_plain_source() {
    let runtime = setup("normal").await;
    let doc = web_sys::window().unwrap().document().unwrap();
    let root = container(&doc);
    let (load_state, set_load_state) = signal(LoadState::Loaded(manifest("$\\pi^2$", "only raw Markdown **here**")));
    let handle = leptos::mount::mount_to(root.clone(), move || view! {
        <GlossaryPanel load_state=load_state /> <RecipesPanel load_state=load_state />
    });
    leptos::task::tick().await;
    assert_eq!(runtime.requests(), 0);
    let launchers = root.query_selector_all(".panel-launch-btn").unwrap();
    launchers.item(1).unwrap().unchecked_ref::<HtmlElement>().click();
    leptos::task::tick().await;
    assert_eq!(element(&root, ".recipe-body").text_content().as_deref(), Some("only raw Markdown **here**"));
    assert_eq!(runtime.requests(), 0, "opening a math-free solution must not fetch math assets");
    click(&root, ".modal-close");
    leptos::task::tick().await;
    click(&root, ".panel-launch-btn");
    settle(&root).await;
    assert_eq!(element(&root, "math msup mi").text_content().as_deref(), Some("π"));
    click(&root, ".modal-close");
    leptos::task::tick().await;
    set_load_state.set(LoadState::Loaded(manifest("$\\pi^2$", "$$x^2$$")));
    leptos::task::tick().await;
    root.query_selector_all(".panel-launch-btn").unwrap().item(1).unwrap().unchecked_ref::<HtmlElement>().click();
    settle(&root).await;
    assert_eq!(element(&root, "math annotation").text_content().as_deref(), Some("x^2"));
    assert_eq!(runtime.requests(), 2, "Glossary and Solution math share one real script/CSS load");
    drop(handle);
    root.remove();
}

async fn unavailable_delivery(mode: &str) {
    let runtime = setup(mode).await;
    let doc = web_sys::window().unwrap().document().unwrap();
    let root = container(&doc);
    let source = "$\\pi^2$";
    let handle = leptos::mount::mount_to(root.clone(), move || view! { <MathText text=source.to_owned() /> });
    settle(&root).await;
    source_visible(&root, source);
    assert!(root.inner_text().contains("Math rendering unavailable; showing LaTeX source."));
    assert!(element(&root, ".panel-math-diagnostic").text_content().unwrap().contains("reload to retry"));
    assert!(root.query_selector(".katex").unwrap().is_none(), "asset/font failures cannot be counted as successful");
    let another = container(&doc);
    let another_handle = leptos::mount::mount_to(another.clone(), move || view! { <MathText text="$x$".to_owned() /> });
    settle(&another).await;
    source_visible(&another, "$x$");
    assert!(another.inner_text().contains("Math rendering unavailable; showing LaTeX source."));
    assert_eq!(runtime.requests(), 2, "availability failure is cached; a new mount does not retry");
    drop(another_handle);
    drop(handle);
    another.remove();
    root.remove();
}

#[wasm_bindgen_test]
async fn math_missing_script_is_visible_and_cached() { unavailable_delivery("script").await; }

#[wasm_bindgen_test]
async fn math_missing_stylesheet_is_visible_and_cached() { unavailable_delivery("css").await; }

#[wasm_bindgen_test]
async fn math_missing_required_font_is_visible_and_cached() { unavailable_delivery("font").await; }

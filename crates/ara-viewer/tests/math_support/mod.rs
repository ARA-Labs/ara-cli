//! Real local asset setup for wasm-pack's filesystem-serving browser runner.
//! Run from crates/ara-viewer so /public/... serves the pinned release bytes.
//! Failure/delay injection changes delivery, never KaTeX typesetting output.
#![allow(dead_code)]

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Document, HtmlElement};

#[wasm_bindgen(inline_js = r#"
export async function install_math_test(viewerCss, mode) {
    const previous = { katex: globalThis.katex, bridge: globalThis.AraMath, append: document.head.append };
    delete globalThis.katex;
    const test = { previous, resources: [], nodes: [] };
    test.gate = mode === "delay" ? new Promise(resolve => { test.release = resolve; }) : Promise.resolve();
    document.head.append = function(...nodes) {
        for (const node of nodes) {
            const url = node.src || node.href;
            if (!url || !new URL(url).pathname.includes("/vendor/katex-0.19.0/")) continue;
            test.resources.push(node);
            if (mode === "script" && node.tagName === "SCRIPT") node.src += ".missing";
            if (mode === "css" && node.tagName === "LINK") node.href += ".missing";
            const loaded = node.onload;
            node.onload = event => {
                if (mode === "font" && node.tagName === "LINK") {
                    for (const rule of node.sheet.cssRules) {
                        if (rule.type === CSSRule.FONT_FACE_RULE) {
                            rule.style.setProperty("src", "url(/public/vendor/katex-0.19.0/fonts/missing.woff2) format('woff2')");
                        }
                    }
                }
                test.gate.then(() => loaded.call(node, event));
            };
        }
        return previous.append.apply(this, nodes);
    };
    const style = document.createElement("style");
    style.textContent = viewerCss;
    document.head.append(style);
    const script = document.createElement("script");
    script.src = new URL("/public/math-loader.js", location.href).href;
    test.nodes.push(style, script);
    await new Promise((resolve, reject) => {
        script.onload = resolve;
        script.onerror = () => reject(new Error("Run browser tests from crates/ara-viewer to serve /public/math-loader.js"));
        document.head.append(script);
    });
    return test;
}
export function cleanup_math_test(test) {
    test.release?.();
    for (const node of [...test.resources, ...test.nodes]) node.remove();
    document.head.append = test.previous.append;
    globalThis.katex = test.previous.katex;
    globalThis.AraMath = test.previous.bridge;
}
export function release_math_test(test) { test.release?.(); }
export function math_request_count(test) { return test.resources.length; }
export function math_fonts_loaded() {
    return Array.from(document.fonts).some(face => face.family.replaceAll('"', '') === "KaTeX_Math" && face.status === "loaded");
}
export function math_external_requests() {
    return performance.getEntriesByType("resource").filter(entry => entry.name.includes("external.invalid")).length;
}
export function math_rule_is_bounded(root) {
    const space = root.querySelector("math mspace[mathbackground]");
    if (!space) return false;
    const width = parseFloat(space.getAttribute("width"));
    const height = parseFloat(space.getAttribute("height"));
    return width > 0 && width <= 20 && height > 0 && height <= 20;
}
export function math_has_local_overflow(root) {
    const display = root.querySelector(".panel-math-display");
    const modal = root.querySelector(".modal");
    return display.scrollWidth > display.clientWidth && modal.scrollWidth <= modal.clientWidth + 1;
}
export function math_aligned_rows_are_distinct(root) {
    const letters = Array.from(root.querySelectorAll(".katex-html .mord.mathnormal"));
    const a = letters.find(node => node.textContent === "a");
    const c = letters.find(node => node.textContent === "c");
    return a && c && c.getBoundingClientRect().top - a.getBoundingClientRect().top > 0.8 * parseFloat(getComputedStyle(a).fontSize);
}
export function math_plain_source(root) {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    let result = "";
    while (walker.nextNode()) {
        if (!walker.currentNode.parentElement.closest(".panel-math")) result += walker.currentNode.textContent;
    }
    return result;
}
"#)]
extern "C" {
    fn install_math_test(viewer_css: &str, mode: &str) -> js_sys::Promise;
    fn cleanup_math_test(runtime: &JsValue);
    fn release_math_test(runtime: &JsValue);
    fn math_request_count(runtime: &JsValue) -> u32;
    pub fn math_fonts_loaded() -> bool;
    pub fn math_external_requests() -> u32;
    pub fn math_rule_is_bounded(root: &HtmlElement) -> bool;
    pub fn math_has_local_overflow(root: &HtmlElement) -> bool;
    pub fn math_aligned_rows_are_distinct(root: &HtmlElement) -> bool;
    pub fn math_plain_source(root: &HtmlElement) -> String;
}

pub struct Runtime(JsValue);

impl Runtime {
    pub fn release(&self) {
        release_math_test(&self.0);
    }
    pub fn requests(&self) -> u32 {
        math_request_count(&self.0)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        cleanup_math_test(&self.0);
    }
}

pub async fn setup(mode: &str) -> Runtime {
    Runtime(
        wasm_bindgen_futures::JsFuture::from(install_math_test(
            include_str!("../../public/styles.css"),
            mode,
        ))
        .await
        .expect("real local math test assets must load"),
    )
}

pub fn container(doc: &Document) -> HtmlElement {
    let div = doc
        .create_element("div")
        .unwrap()
        .unchecked_into::<HtmlElement>();
    doc.body().unwrap().append_child(&div).unwrap();
    div
}

pub fn click(root: &HtmlElement, selector: &str) {
    root.query_selector(selector)
        .unwrap()
        .unwrap()
        .unchecked_ref::<HtmlElement>()
        .click();
}

pub async fn filter(root: &HtmlElement, value: &str) {
    let input = root
        .query_selector(".panel-filter")
        .unwrap()
        .unwrap()
        .unchecked_into::<web_sys::HtmlInputElement>();
    input.set_value(value);
    input
        .dispatch_event(&web_sys::Event::new("input").unwrap())
        .unwrap();
    leptos::task::tick().await;
}

pub async fn settle(root: &HtmlElement) {
    for _ in 0..500 {
        leptos::task::tick().await;
        let statuses = root.query_selector_all(".panel-math-status").unwrap();
        let loading = (0..statuses.length())
            .any(|i| statuses.item(i).unwrap().text_content().as_deref() == Some("Loading math…"));
        if !loading {
            return;
        }
        sleep(20).await;
    }
    panic!("math did not settle: {}", root.inner_text());
}

pub async fn sleep(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
}

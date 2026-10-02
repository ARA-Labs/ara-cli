//! Source-preserving dollar math for Glossary and raw Solution-file text.
//!
//! Markdown is parsed only to protect code ranges, never to reserialize prose.
//! Leptos owns each host and its escaped fallback; the local renderer owns only
//! host descendants, and a disposed fragment cannot commit asynchronous output.

use std::ops::Range;

use leptos::prelude::*;
use pulldown_cmark::{Event, Parser, Tag};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathFragment<'a> {
    pub range: Range<usize>,
    pub source: &'a str,
    pub tex: &'a str,
    pub display: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment<'a> {
    Plain(&'a str),
    Math(MathFragment<'a>),
}

impl<'a> Segment<'a> {
    pub fn source(&self) -> &'a str {
        match self {
            Self::Plain(source) => source,
            Self::Math(fragment) => fragment.source,
        }
    }
}

fn escaped(bytes: &[u8], position: usize) -> bool {
    let backslashes = bytes[..position]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    backslashes % 2 == 1
}

// A failed inline search also returns its scan boundary, so currency/unmatched
// dollar runs are not repeatedly scanned from every candidate opener.
fn closing_delimiter(
    source: &str,
    start: usize,
    end: usize,
    display: bool,
) -> (Option<usize>, usize) {
    let bytes = source.as_bytes();
    let width = if display { 2 } else { 1 };
    let mut position = start + width;
    while position < end {
        let byte = bytes[position];
        if !display && matches!(byte, b'\n' | b'\r') {
            return (None, position);
        }
        if byte == b'$' && !escaped(bytes, position) {
            let double = position + 1 < end && bytes[position + 1] == b'$';
            if double {
                return (display.then_some(position), position);
            }
            if !display {
                let content = &source[start + 1..position];
                let content_edge = content
                    .chars()
                    .next_back()
                    .is_some_and(|c| !c.is_whitespace());
                let followed_by_digit = bytes.get(position + 1).is_some_and(u8::is_ascii_digit);
                if content_edge && !followed_by_digit {
                    return (Some(position), position);
                }
            }
        }
        position += 1;
    }
    (None, if display { start + width } else { end })
}

/// Recognize only `$...$` and `$$...$$` outside Markdown code ranges.
/// Each output borrows the original source, including unmatched delimiters,
/// Markdown markers, whitespace, and the complete spelling of every equation.
pub fn segments(source: &str) -> Vec<Segment<'_>> {
    let protected = Parser::new(source)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            matches!(event, Event::Code(_) | Event::Start(Tag::CodeBlock(_))).then_some(range)
        });
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut position = 0;
    let mut plain_start = 0;
    // The terminal empty range lets the last unprotected slice use the same scan.
    for range in protected.chain(std::iter::once(source.len()..source.len())) {
        let end = range.start;
        while position < end {
            if bytes[position] != b'$' || escaped(bytes, position) {
                position += 1;
                continue;
            }
            let display = position + 1 < end && bytes[position + 1] == b'$';
            let width = if display { 2 } else { 1 };
            if !display
                && source[position + 1..end]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace)
            {
                position += 1;
                continue;
            }
            let (close, resume) = closing_delimiter(source, position, end, display);
            if let Some(close) = close {
                if plain_start < position {
                    result.push(Segment::Plain(&source[plain_start..position]));
                }
                let finish = close + width;
                result.push(Segment::Math(MathFragment {
                    range: position..finish,
                    source: &source[position..finish],
                    tex: &source[position + width..close],
                    display,
                }));
                position = finish;
                plain_start = finish;
            } else {
                position = resume;
            }
        }
        position = position.max(range.end);
    }
    if plain_start < source.len() {
        result.push(Segment::Plain(&source[plain_start..]));
    }
    result
}

#[derive(Clone, PartialEq, Eq)]
enum RenderState {
    Loading,
    Ready,
    Failed { unavailable: bool, detail: String },
}

#[cfg(target_arch = "wasm32")]
mod bridge {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        pub type MathJob;
        pub type MathResult;

        #[wasm_bindgen(catch, js_namespace = AraMath, js_name = createJob)]
        pub fn create_job(tex: &str, display: bool) -> Result<MathJob, JsValue>;
        #[wasm_bindgen(js_namespace = AraMath, js_name = jobPromise)]
        pub fn job_promise(job: &MathJob) -> js_sys::Promise;
        #[wasm_bindgen(js_namespace = AraMath, js_name = cancelJob)]
        pub fn cancel_job(job: &MathJob);
        #[wasm_bindgen(method, getter)]
        pub fn status(this: &MathResult) -> String;
        #[wasm_bindgen(method, getter)]
        pub fn detail(this: &MathResult) -> String;
        #[wasm_bindgen(method, getter)]
        pub fn host(this: &MathResult) -> Option<web_sys::Element>;
    }
}

#[component]
fn Equation(source: String, tex: String, display: bool) -> impl IntoView {
    let host_ref = NodeRef::<leptos::html::Span>::new();
    let state = RwSignal::new(if cfg!(target_arch = "wasm32") {
        RenderState::Loading
    } else {
        RenderState::Failed {
            unavailable: true,
            detail: "Math rendering requires a browser.".into(),
        }
    });

    #[cfg(target_arch = "wasm32")]
    {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use wasm_bindgen::JsCast;

        let live = Arc::new(AtomicBool::new(true));
        let cleanup_live = live.clone();
        let job = StoredValue::new_local(None::<bridge::MathJob>);
        on_cleanup(move || {
            cleanup_live.store(false, Ordering::Relaxed);
            job.with_value(|job| {
                if let Some(job) = job {
                    bridge::cancel_job(job);
                }
            });
        });
        Effect::new(move |_| {
            let Some(host) = host_ref.get() else { return };
            if job.with_value(Option::is_some) {
                return;
            }
            let Ok(pending) = bridge::create_job(&tex, display) else {
                state.set(RenderState::Failed {
                    unavailable: true,
                    detail: "The local math loader is unavailable; reload to retry.".into(),
                });
                return;
            };
            let promise = bridge::job_promise(&pending);
            job.set_value(Some(pending));
            let live = live.clone();
            leptos::task::spawn_local(async move {
                let result = wasm_bindgen_futures::JsFuture::from(promise).await;
                // Check ownership before touching either the DOM or reactive signals.
                if !live.load(Ordering::Relaxed) || !host.is_connected() {
                    return;
                }
                match result {
                    Ok(value) => {
                        let result: bridge::MathResult = value.unchecked_into();
                        match result.status().as_str() {
                            "ready" => {
                                if let Some(rendered) = result.host()
                                    && host.append_child(&rendered).is_ok()
                                {
                                    state.set(RenderState::Ready);
                                    return;
                                }
                                state.set(RenderState::Failed {
                                    unavailable: true,
                                    detail: "The equation could not be mounted.".into(),
                                });
                            }
                            "cancelled" => {}
                            status => state.set(RenderState::Failed {
                                unavailable: status == "unavailable",
                                detail: result.detail(),
                            }),
                        }
                    }
                    Err(_) => state.set(RenderState::Failed {
                        unavailable: true,
                        detail: "The local math renderer failed; reload to retry.".into(),
                    }),
                }
            });
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = tex;

    let content = move || {
        view! {
            <span class="panel-math-host" node_ref=host_ref hidden=move || state.with(|s| *s != RenderState::Ready)></span>
            <span class="panel-math-source" hidden=move || state.with(|s| *s == RenderState::Ready)>{source.clone()}</span>
            <span class="panel-math-status" role="status" aria-live="polite">
                {move || match state.get() {
                    RenderState::Loading => "Loading math…".into(),
                    RenderState::Ready => String::new(),
                    RenderState::Failed { unavailable: true, .. } => "Math rendering unavailable; showing LaTeX source.".into(),
                    RenderState::Failed { unavailable: false, .. } => "Could not render equation.".into(),
                }}
            </span>
            {move || match state.get() {
                RenderState::Failed { detail, .. } => Some(view! { <span class="panel-math-diagnostic">{detail}</span> }),
                _ => None,
            }}
        }
    };
    if display {
        view! { <div class="panel-math panel-math-display">{content()}</div> }.into_any()
    } else {
        view! { <span class="panel-math panel-math-inline">{content()}</span> }.into_any()
    }
}

/// Raw panel text with individually owned math fragments; not a Markdown view.
#[component]
pub fn MathText(text: String) -> impl IntoView {
    segments(&text)
        .into_iter()
        .map(|segment| match segment {
            Segment::Plain(source) => source.to_owned().into_any(),
            Segment::Math(fragment) => view! {
                <Equation source=fragment.source.to_owned() tex=fragment.tex.to_owned() display=fragment.display />
            }.into_any(),
        })
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expressions(source: &str) -> Vec<MathFragment<'_>> {
        let segments = segments(source);
        assert_eq!(
            segments.iter().map(Segment::source).collect::<String>(),
            source
        );
        segments
            .into_iter()
            .filter_map(|segment| match segment {
                Segment::Math(fragment) => {
                    assert_eq!(&source[fragment.range.clone()], fragment.source);
                    Some(fragment)
                }
                Segment::Plain(_) => None,
            })
            .collect()
    }

    #[test]
    fn math_display_preserves_expression_and_protected_source() {
        let source = "π before $$\\begin{aligned}a&=b\\\\\nc&=d\\end{aligned}$$ after `$code$`\n```text\n$$literal$$\n```\n";
        let fragments = expressions(source);
        assert_eq!(
            fragments.iter().map(|m| m.source).collect::<Vec<_>>(),
            vec!["$$\\begin{aligned}a&=b\\\\\nc&=d\\end{aligned}$$"]
        );
        assert!(fragments[0].display);
        assert_eq!(
            fragments[0].tex,
            "\\begin{aligned}a&=b\\\\\nc&=d\\end{aligned}"
        );
    }

    #[test]
    fn mixed_inline_display_adjacent_and_utf8() {
        let fragments = expressions("π policy $\\pi^{(k)}$ and $\\Phi^{k;s}$\n$$a^2$$$$b^2$$ 끝");
        assert_eq!(
            fragments
                .iter()
                .map(|m| (m.tex, m.display))
                .collect::<Vec<_>>(),
            vec![
                ("\\pi^{(k)}", false),
                ("\\Phi^{k;s}", false),
                ("a^2", true),
                ("b^2", true)
            ]
        );
    }

    #[test]
    fn unmatched_inline_cannot_swallow_a_display_expression() {
        let fragments = expressions("$unfinished before $$x^2$$ then $y$");
        assert_eq!(
            fragments
                .iter()
                .map(|m| (m.tex, m.display))
                .collect::<Vec<_>>(),
            vec![("x^2", true), ("y", false)]
        );
    }

    #[test]
    fn dollars_preserve_currency_escaping_and_unmatched_source() {
        let source =
            "cost $5 and $10; \\$escaped\\$; $ spaced $; $a\nb$; $$unfinished; \\(x\\) \\[y\\]";
        assert!(expressions(source).is_empty());
        assert_eq!(
            expressions(r"\\$x$ \\$y$")
                .iter()
                .map(|m| m.tex)
                .collect::<Vec<_>>(),
            vec!["x", "y"]
        );
        assert_eq!(expressions("$x$2").len(), 0);
        assert_eq!(expressions("$x$ done")[0].tex, "x");
    }

    #[test]
    fn protected_code_ranges_are_never_crossed_or_reserialized() {
        let source = "**raw** ``backtick ` $x$`` and `$y$`\n\n~~~rust\n$$fenced$$\n~~~\n\n    $indented$\n    $$still code$$\n\n$real$\n$before `code` after$";
        assert_eq!(
            expressions(source)
                .iter()
                .map(|m| m.source)
                .collect::<Vec<_>>(),
            vec!["$real$"]
        );
    }

    #[test]
    fn escaped_tex_dollars_and_malformed_braces_preserve_fragment_boundaries() {
        let fragments = expressions(
            r"$\text{price \$5} + x$ $\frac{1}{$ $$\begin{cases}x&x>0\\0&x\le0\end{cases}$$",
        );
        assert_eq!(
            fragments.iter().map(|m| m.tex).collect::<Vec<_>>(),
            vec![
                r"\text{price \$5} + x",
                r"\frac{1}{",
                r"\begin{cases}x&x>0\\0&x\le0\end{cases}"
            ]
        );
    }

    #[test]
    fn empty_plain_and_unmatched_display_keep_all_bytes() {
        assert!(segments("").is_empty());
        assert!(expressions("just prose — no math").is_empty());
        assert!(expressions("$$never closes\n").is_empty());
        assert_eq!(expressions("$$unmatched then $x$ end")[0].source, "$x$");
    }
}

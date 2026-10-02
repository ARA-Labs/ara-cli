//! Client-side markdown → HTML for exhibit bodies (issue #32).
//!
//! Exhibit bodies (`evidence/figures/*.md`, `evidence/tables/*.md`) are full GFM
//! markdown — captions + prose + tables. We render them to an HTML string with
//! `pulldown-cmark` and mount it via Leptos's `inner_html` on a wrapper element
//! (approach A). This is the first `inner_html` in the viewer, which otherwise
//! emits only escaped Leptos nodes; the exception is deliberate and bounded to
//! trusted-local exhibit content.
//!
//! Only the extensions the corpus needs are enabled (tables, strikethrough).
//! Exhibit math remains literal: this renderer deliberately does not use the
//! separate Glossary/Solution-file math pipeline.
//!
//! Two injection vectors are closed before mounting, because the corpus is not
//! always self-authored — `ara serve` may render a *downloaded* artifact:
//!   1. Raw HTML embedded in a body (`Event::Html`/`InlineHtml`) is re-emitted
//!      as escaped text, so no author markup reaches the DOM.
//!   2. Link/image destinations are scheme-checked against an allowlist, so a
//!      `[x](javascript:…)` or `![x](data:…)` cannot become a live sink.

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};

/// URL schemes safe to emit into `href`/`src`. A destination with no scheme
/// (a relative link) is also safe; anything else — `javascript:`, `data:`,
/// `vbscript:`, `file:`, … — is neutralised.
const SAFE_SCHEMES: [&str; 3] = ["http", "https", "mailto"];

/// True if `url` is safe to place in an `href`/`src` attribute: a relative URL,
/// or one whose scheme is in [`SAFE_SCHEMES`].
///
/// Whitespace and ASCII control characters are stripped first, because browsers
/// ignore them when parsing the scheme (so `java\tscript:` would otherwise
/// slip through).
fn is_safe_url(url: &str) -> bool {
    let cleaned: String = url
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && !c.is_ascii_control())
        .collect();

    match cleaned.find(':') {
        // No colon → relative URL, safe.
        None => true,
        Some(i) => {
            let scheme = &cleaned[..i];
            // A colon after a path separator is part of the path, not a scheme
            // (e.g. `./a:b`, `#frag`, `?q=a:b`) — those are relative, safe.
            if scheme.is_empty()
                || scheme.contains('/')
                || scheme.contains('?')
                || scheme.contains('#')
            {
                return true;
            }
            let scheme = scheme.to_ascii_lowercase();
            SAFE_SCHEMES.contains(&scheme.as_str())
        }
    }
}

/// Render an exhibit body's GFM markdown to an HTML string.
///
/// Tables and strikethrough are enabled. The result is safe to mount via
/// `inner_html`: raw HTML in the source is escaped, and link/image destinations
/// with an unsafe scheme are rewritten to `#`.
pub fn render_exhibit_body(md: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);

    let parser = Parser::new_ext(md, opts).map(|event| match event {
        // Neutralise raw-HTML events: re-emit their content as escaped text so
        // no author-supplied markup reaches the DOM through `inner_html`.
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),

        // Neutralise link/image destinations with an unsafe scheme.
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) if !is_safe_url(&dest_url) => Event::Start(Tag::Link {
            link_type,
            dest_url: CowStr::Borrowed("#"),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) if !is_safe_url(&dest_url) => Event::Start(Tag::Image {
            link_type,
            dest_url: CowStr::Borrowed("#"),
            title,
            id,
        }),

        other => other,
    });

    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_gfm_table() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |\n";
        let html = render_exhibit_body(md);
        assert!(html.contains("<table>"), "got: {html}");
        assert!(html.contains("<th>"), "got: {html}");
        assert!(html.contains("<td>"), "got: {html}");
    }

    #[test]
    fn escapes_raw_html() {
        let html = render_exhibit_body("<script>alert(1)</script>");
        assert!(!html.contains("<script>"), "raw HTML leaked: {html}");
        assert!(html.contains("&lt;script&gt;"), "not escaped: {html}");
    }

    #[test]
    fn leaves_math_literal() {
        let html = render_exhibit_body("energy is $E = mc^2$ here");
        assert!(html.contains("$E = mc^2$"), "math not literal: {html}");
    }

    #[test]
    fn neutralises_javascript_link() {
        let html = render_exhibit_body("[click](javascript:alert(1))");
        assert!(!html.contains("javascript:"), "js scheme leaked: {html}");
        assert!(html.contains("href=\"#\""), "not rewritten to #: {html}");
    }

    #[test]
    fn neutralises_control_char_scheme_evasion() {
        // A tab inside the scheme, delivered via the `<…>` destination form so
        // it reaches the parsed link dest (browsers strip it before parsing, so
        // `java\tscript:` would run without the whitespace-stripping in
        // `is_safe_url`).
        let html = render_exhibit_body("[click](<java\tscript:alert(1)>)");
        assert!(
            html.contains("href=\"#\""),
            "control-char scheme evasion not neutralised: {html}"
        );
        assert!(
            !html.contains("script:alert"),
            "control-char scheme evasion leaked a live href: {html}"
        );
    }

    #[test]
    fn neutralises_entity_encoded_scheme() {
        // pulldown-cmark decodes `&colon;` / `&#58;` in the destination before
        // our map sees it, so the guard still catches the reconstructed scheme.
        for md in [
            "[c](javascript&colon;alert(1))",
            "[c](javascript&#58;alert(1))",
        ] {
            let html = render_exhibit_body(md);
            assert!(
                !html.contains("javascript:"),
                "entity-encoded scheme leaked for {md:?}: {html}"
            );
            assert!(
                html.contains("href=\"#\""),
                "not rewritten for {md:?}: {html}"
            );
        }
    }

    #[test]
    fn neutralises_data_image() {
        let html = render_exhibit_body("![x](data:text/html,<script>alert(1)</script>)");
        assert!(!html.contains("data:"), "data scheme leaked: {html}");
        assert!(html.contains("src=\"#\""), "img src not rewritten: {html}");
    }

    #[test]
    fn keeps_safe_and_relative_links() {
        let html = render_exhibit_body("[a](https://example.com) and [b](./local.md)");
        assert!(
            html.contains("https://example.com"),
            "https dropped: {html}"
        );
        assert!(html.contains("./local.md"), "relative link dropped: {html}");
    }

    #[test]
    fn is_safe_url_classifies() {
        assert!(is_safe_url("https://example.com"));
        assert!(is_safe_url("http://example.com"));
        assert!(is_safe_url("mailto:a@b.com"));
        assert!(is_safe_url("./relative/path.md"));
        assert!(is_safe_url("#anchor"));
        assert!(is_safe_url("?q=a:b"));
        assert!(!is_safe_url("javascript:alert(1)"));
        assert!(!is_safe_url("JavaScript:alert(1)"));
        assert!(!is_safe_url("data:text/html,x"));
        assert!(!is_safe_url("vbscript:msgbox(1)"));
    }
}

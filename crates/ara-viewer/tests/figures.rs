#![cfg(target_arch = "wasm32")]

use ara_viewer::{
    detail::DetailPane,
    source::{ImageSource, ManifestSource, fetch_manifest},
    state::LoadState,
};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;
use web_sys::{HtmlElement, HtmlImageElement};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn image_urls_follow_successful_manifest_location_and_encode_filename_segments() {
    let reference = "evidence/figures/雪 % # ?.PNG";
    for (manifest_url, api, expected) in [
        (
            "https://example.test/api/manifest",
            true,
            "https://example.test/api/figure/figures/%E9%9B%AA%20%25%20%23%20%3F.PNG",
        ),
        (
            "https://example.test/a/demo/api/manifest",
            true,
            "https://example.test/a/demo/api/figure/figures/%E9%9B%AA%20%25%20%23%20%3F.PNG",
        ),
        (
            "https://example.test/site/artifacts/demo/manifest.json?version=2#old",
            false,
            "https://example.test/site/artifacts/demo/evidence/figures/%E9%9B%AA%20%25%20%23%20%3F.PNG",
        ),
    ] {
        let source = ImageSource {
            manifest_url: manifest_url.into(),
            api,
        };
        assert_eq!(source.image_url(reference).as_deref(), Some(expected));
        for unsafe_path in [
            "https://unsafe.test/x.png",
            "//unsafe.test/x.png",
            "/evidence/x.png",
            "evidence/../x.png",
            "evidence\\x.png",
            "evidence/x\n.png",
            "evidence/x.svg",
            "data:image/png;base64,x",
        ] {
            assert!(source.image_url(unsafe_path).is_none(), "{unsafe_path:?}");
        }
        assert_eq!(
            source.image_url("evidence/figures/%2e%2e.png").unwrap(),
            expected.rsplit_once('/').unwrap().0.to_string() + "/%252e%252e.png"
        );
    }
    let doc = web_sys::window().unwrap().document().unwrap();
    let source = ImageSource {
        manifest_url: "tests/fixtures/figure-artifact/manifest.json".into(),
        api: false,
    };
    let manifest =
        web_sys::Url::new_with_base(&source.manifest_url, &doc.base_uri().unwrap().unwrap())
            .unwrap();
    assert_eq!(
        source.image_url("evidence/figures/pixel.png").unwrap(),
        web_sys::Url::new_with_base("evidence/figures/pixel.png", &manifest.href())
            .unwrap()
            .href()
    );
}

// The wasm-bindgen browser runner serves real files from its current directory.
// Run wasm-pack from crates/ara-viewer so this fixture is a same-origin HTTP asset.
#[wasm_bindgen_test]
async fn static_and_api_fallback_figures_decode_real_images_and_keep_markdown() {
    let fixture = "tests/fixtures/figure-artifact/manifest.json";
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = doc
        .create_element("div")
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap();
    container
        .set_attribute(
            "style",
            "width:720px; max-width:100%; box-sizing:border-box",
        )
        .unwrap();
    doc.body().unwrap().append_child(&container).unwrap();
    let style = doc.create_element("style").unwrap();
    style.set_text_content(Some(include_str!("../public/styles.css")));
    doc.head().unwrap().append_child(&style).unwrap();
    let image_source: RwSignal<Option<ImageSource>> = RwSignal::new(None);
    let (load_state, set_load_state) = signal(LoadState::Loading);
    let selected = RwSignal::new(Some(ara_core::NodeId::new("N01")));
    let handle = leptos::mount::mount_to(container.clone(), move || {
        provide_context(image_source);
        view! { <DetailPane load_state=load_state selected=selected /> }
    });
    for (source, api_expected) in [
        (ManifestSource::Static(fixture.into()), false),
        // Treat this real HTTP JSON endpoint as the primary API. Its filename
        // deliberately is not "api/manifest": mapping must follow the fetch
        // mode, not a guess from the page or response pathname.
        (
            ManifestSource::Api {
                manifest_url: fixture.into(),
                fallback_url: "tests/fixtures/missing.json".into(),
                live_url: "api/live".into(),
            },
            true,
        ),
        (
            ManifestSource::Api {
                manifest_url: "tests/fixtures/missing/api/manifest".into(),
                fallback_url: fixture.into(),
                live_url: "api/live".into(),
            },
            false,
        ),
    ] {
        // Refetch through the same mounted pane so stale image context from the
        // previous successful API/static response cannot pass this test.
        set_load_state.set(LoadState::Loading);
        fetch_manifest(source, move |state, image| {
            image_source.set(image);
            set_load_state.set(state);
        });
        for _ in 0..200 {
            if !matches!(load_state.get_untracked(), LoadState::Loading) {
                break;
            }
            sleep_ms(10).await;
        }
        match load_state.get_untracked() {
            LoadState::Loaded(_) => {}
            other => panic!("real static fixture must load: {other:?}"),
        }
        leptos::task::tick().await;
        let successful_source = image_source.get_untracked().unwrap();
        assert_eq!(
            successful_source.api, api_expected,
            "fallback must switch to static mapping"
        );
        assert!(successful_source.manifest_url.ends_with(fixture));
        assert_ne!(
            successful_source.manifest_url,
            doc.base_uri().unwrap().unwrap()
        );
        assert_eq!(
            container
                .query_selector_all("figure.detail-figure")
                .unwrap()
                .length(),
            3
        );
        let images = container
            .query_selector_all("figure.detail-figure > img")
            .unwrap();
        for i in 0..images.length() {
            let image = images
                .item(i)
                .unwrap()
                .dyn_into::<HtmlImageElement>()
                .unwrap();
            JsFuture::from(image.decode())
                .await
                .expect("fixture image must actually decode");
            assert!(image.natural_width() > 0);
        }
        let image = images
            .item(0)
            .unwrap()
            .dyn_into::<HtmlImageElement>()
            .unwrap();
        assert_eq!(image.natural_width(), 1200);
        assert_eq!(image.alt(), "<em>Loss & accuracy</em>");
        let image_directory = if api_expected {
            "/figure/"
        } else {
            "/evidence/"
        };
        assert!(image.src().ends_with(&format!(
            "{image_directory}figures/%E9%9B%AA%20%25%20%23%20%3F.PNG"
        )));
        let caption = container
            .query_selector("figure.detail-figure > figcaption")
            .unwrap()
            .unwrap();
        assert_eq!(
            caption.text_content().as_deref(),
            Some("<em>Loss & accuracy</em>")
        );
        assert!(
            caption.query_selector("em").unwrap().is_none(),
            "caption must remain escaped text"
        );
        let image_only = images
            .item(1)
            .unwrap()
            .dyn_into::<HtmlImageElement>()
            .unwrap();
        assert_eq!(image_only.alt(), "Image only");
        assert_eq!(image_only.natural_width(), 1);
        let uncaptioned = images
            .item(2)
            .unwrap()
            .dyn_into::<HtmlImageElement>()
            .unwrap();
        assert_eq!(uncaptioned.alt(), "uncaptioned");
        assert!(
            uncaptioned
                .parent_element()
                .unwrap()
                .query_selector("figcaption")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            container
                .query_selector_all(".exhibit-body table")
                .unwrap()
                .length(),
            2
        );
        assert_eq!(
            container
                .query_selector_all(".exhibit-caption")
                .unwrap()
                .length(),
            3
        );
        let text = container.inner_text();
        for retained in [
            "Supporting measurements.",
            "Markdown caption",
            "Markdown fallback.",
            "Rejected caption",
            "Safe rejected body.",
            "Table caption",
            "Non-figure body.",
        ] {
            assert!(text.contains(retained), "{retained}");
        }
        assert_eq!(text.matches("<em>Loss & accuracy</em>").count(), 1);
        assert!(container.query_selector("script").unwrap().is_none());
        assert!(
            container
                .query_selector("img[src*='unsafe']")
                .unwrap()
                .is_none()
        );
        for width in [720, 280] {
            container
                .set_attribute(
                    "style",
                    &format!("width:{width}px; max-width:100%; box-sizing:border-box"),
                )
                .unwrap();
            leptos::task::tick().await;
            let figure = image.parent_element().unwrap();
            assert!(
                image.get_bounding_client_rect().width()
                    <= container.get_bounding_client_rect().width() + 1.0
            );
            assert!(
                figure.get_bounding_client_rect().width()
                    <= container.get_bounding_client_rect().width() + 1.0
            );
            assert!(container.scroll_width() <= container.client_width() + 1);
            let body = container.query_selector(".exhibit-body").unwrap().unwrap();
            assert_eq!(
                web_sys::window()
                    .unwrap()
                    .get_computed_style(&body)
                    .unwrap()
                    .unwrap()
                    .get_property_value("overflow-x")
                    .unwrap(),
                "auto"
            );
        }
    }
    drop(handle);
    container.remove();
    style.remove();
}

async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .unwrap();
    });
    JsFuture::from(promise).await.unwrap();
}

mod bundle;
mod css;
mod fetch;
mod refs;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use base64::prelude::{BASE64_STANDARD, Engine as _};
use bundle::Kind;
use fullbleed::{AssetBundle, FullBleed};
use futures::future::join_all;
use lol_html::html_content::{ContentType, Element};
use lol_html::{RewriteStrSettings, element, rewrite_str, text};
use url::Url;

use css::CssLocation;
use fetch::{Fetched, Fetcher, Transport, WstdTransport, redact};
use html_escape::decode_html_entities;

const BROWSER_DEFAULT_STYLES: &str = "html{font-family:serif}\
b,strong,th{font-weight:bold}i,em{font-style:italic}p{margin:1em 0}\
h1{font-size:2em;margin:.67em 0;font-weight:bold}h2{font-size:1.5em;margin:.83em 0;font-weight:bold}\
h3{font-size:1.17em;margin:1em 0;font-weight:bold}a{color:#00e;text-decoration:underline}";

pub fn render_html(html: &str) -> Result<Vec<u8>, String> {
    let fetcher = Fetcher::new(WstdTransport::new());
    wstd::runtime::block_on(render(html, &fetcher))
}

async fn render<T: Transport>(html: &str, fetcher: &Fetcher<T>) -> Result<Vec<u8>, String> {
    let prepared = prepare(html, fetcher).await?;
    FullBleed::builder()
        .register_bundle(prepared.bundle)
        .build()
        .and_then(|engine| engine.render_to_buffer(&prepared.html, ""))
        .map_err(|e| format!("Failed to generate PDF: {e}"))
}

struct Prepared {
    html: String,
    bundle: AssetBundle,
}

#[derive(Default)]
struct Scanned {
    base: Option<Url>,
    has_head: bool,
    css: Vec<(CssLocation, String)>,
    images: Vec<String>,
}

async fn prepare<T: Transport>(html: &str, fetcher: &Fetcher<T>) -> Result<Prepared, String> {
    let scanned = scan(html)?;
    let has_head = scanned.has_head;
    let (replacements, bundle) = process(scanned, fetcher).await;
    let html = rewrite(html, &replacements)?;
    Ok(Prepared {
        html: if has_head {
            html
        } else {
            format!("{}{html}", default_styles())
        },
        bundle,
    })
}

fn scan(html: &str) -> Result<Scanned, String> {
    let scanned = RefCell::new(Scanned::default());
    let style_text = RefCell::new(String::new());
    rewrite_str(
        html,
        RewriteStrSettings {
            element_content_handlers: vec![
                element!("head", |_| {
                    scanned.borrow_mut().has_head = true;
                    Ok(())
                }),
                element!("base[href]", |el| {
                    let mut scanned = scanned.borrow_mut();
                    if scanned.base.is_none() {
                        scanned.base =
                            attribute(el, "href").and_then(|href| Url::parse(&href).ok());
                    }
                    Ok(())
                }),
                element!("link[href]", |el| {
                    if links_a_stylesheet(el) {
                        let media = attribute(el, "media");
                        let href = el.get_attribute("href").unwrap_or_default();
                        scanned
                            .borrow_mut()
                            .css
                            .push((CssLocation::LinkedStylesheet { media }, href));
                    }
                    Ok(())
                }),
                element!("[style]", |el| {
                    if has_inline_style(el) {
                        let style = el.get_attribute("style").unwrap_or_default();
                        scanned
                            .borrow_mut()
                            .css
                            .push((CssLocation::StyleAttribute, style));
                    }
                    Ok(())
                }),
                text!("style", |chunk| {
                    style_text.borrow_mut().push_str(chunk.as_str());
                    if chunk.last_in_text_node() {
                        let css = std::mem::take(&mut *style_text.borrow_mut());
                        scanned
                            .borrow_mut()
                            .css
                            .push((CssLocation::StyleElement, css));
                    }
                    Ok(())
                }),
                element!("img, image", |el| {
                    if let Some((_, src)) = image_source(el) {
                        scanned.borrow_mut().images.push(src);
                    }
                    Ok(())
                }),
            ],
            ..RewriteStrSettings::new()
        },
    )
    .map_err(|e| format!("Invalid HTML: {e}"))?;
    Ok(scanned.into_inner())
}

async fn process<T: Transport>(
    scanned: Scanned,
    fetcher: &Fetcher<T>,
) -> (HashMap<String, String>, AssetBundle) {
    let base = scanned.base.as_ref();
    let css = join_all(
        scanned
            .css
            .into_iter()
            .map(|(location, original)| async move {
                let css = match location {
                    CssLocation::StyleElement => {
                        css::prepare_stylesheet(fetcher, &original, base, 0).await
                    }
                    CssLocation::StyleAttribute => {
                        css::absolutize_urls(fetcher, &decode_html_entities(&original), true, base)
                    }
                    CssLocation::LinkedStylesheet { .. } => {
                        let import =
                            format!("@import url(\"{}\");", decode_html_entities(&original));
                        css::prepare_stylesheet(fetcher, &import, base, 0).await
                    }
                };
                (location, original, css)
            }),
    )
    .await;
    let images: Vec<(String, Option<Url>)> = scanned
        .images
        .into_iter()
        .map(|original| {
            let url = fetcher.resolve(base, &decode_html_entities(&original).into_owned());
            (original, url)
        })
        .collect();

    let fetched = fetch_all(fetcher, resource_urls(&css, &images)).await;
    let (bundle, replacement_urls) = bundle_resources(fetched);

    let mut replacements = HashMap::new();
    for (location, original, css) in css {
        let css = css::replace_urls(&css, location.is_style_attribute(), &replacement_urls);
        replacements.insert(original, css::write_css(&css, location));
    }
    for (original, url) in images {
        if let Some(replacement) = url.and_then(|url| replacement_urls.get(url.as_str())) {
            replacements.insert(original, replacement.clone());
        }
    }
    (replacements, bundle)
}

fn rewrite(html: &str, replacements: &HashMap<String, String>) -> Result<String, String> {
    let style_text = RefCell::new(String::new());
    rewrite_str(
        html,
        RewriteStrSettings {
            element_content_handlers: vec![
                element!("head", |el| {
                    el.prepend(&default_styles(), ContentType::Html);
                    Ok(())
                }),
                element!("link[href]", |el| {
                    if links_a_stylesheet(el)
                        && let Some(style) = el
                            .get_attribute("href")
                            .and_then(|href| replacements.get(&href))
                    {
                        el.replace(style, ContentType::Html);
                    }
                    Ok(())
                }),
                element!("[style]", |el| {
                    if let Some(css) = el
                        .get_attribute("style")
                        .and_then(|style| replacements.get(&style))
                    {
                        el.set_attribute("style", css)?;
                    }
                    Ok(())
                }),
                text!("style", |chunk| {
                    style_text.borrow_mut().push_str(chunk.as_str());
                    chunk.remove();
                    if chunk.last_in_text_node() {
                        let original = std::mem::take(&mut *style_text.borrow_mut());
                        let css = replacements.get(&original).unwrap_or(&original);
                        chunk.replace(css, ContentType::Html);
                    }
                    Ok(())
                }),
                element!("img, image", |el| {
                    if let Some((name, src)) = image_source(el)
                        && let Some(replacement) = replacements.get(&src)
                    {
                        el.set_attribute(name, replacement)?;
                    }
                    Ok(())
                }),
            ],
            ..RewriteStrSettings::new()
        },
    )
    .map_err(|e| format!("Invalid HTML: {e}"))
}

fn attribute(el: &Element, name: &str) -> Option<String> {
    el.get_attribute(name)
        .map(|value| decode_html_entities(&value).into_owned())
}

fn links_a_stylesheet(el: &Element) -> bool {
    el.tag_name() == "link"
        && attribute(el, "rel").is_some_and(|rel| {
            rel.split_ascii_whitespace()
                .any(|token| token.eq_ignore_ascii_case("stylesheet"))
        })
}

fn has_inline_style(el: &Element) -> bool {
    !links_a_stylesheet(el)
        && el
            .get_attribute("style")
            .is_some_and(|style| !style.is_empty())
}

fn image_source(el: &Element) -> Option<(&'static str, String)> {
    let names: &[&'static str] = if el.tag_name() == "img" {
        &["src"]
    } else {
        &["href", "xlink:href"]
    };
    names
        .iter()
        .find_map(|name| el.get_attribute(name).map(|value| (*name, value)))
}

fn resource_urls(
    css: &[(CssLocation, String, String)],
    images: &[(String, Option<Url>)],
) -> Vec<Url> {
    let mut urls: Vec<Url> = Vec::new();
    for (location, _, css) in css {
        urls.extend(
            refs::urls(css, location.is_style_attribute())
                .into_iter()
                .filter_map(|url| Url::parse(&url).ok().filter(fetch::is_http)),
        );
    }
    urls.extend(images.iter().filter_map(|(_, url)| url.clone()));
    let mut seen = HashSet::new();
    urls.retain(|url| seen.insert(url.clone()));
    urls
}

async fn fetch_all<T: Transport>(
    fetcher: &Fetcher<T>,
    urls: Vec<Url>,
) -> Vec<(Url, Result<Fetched, String>)> {
    join_all(urls.into_iter().map(|url| async move {
        let result = fetcher.fetch(&url).await;
        (url, result)
    }))
    .await
}

fn bundle_resources(
    fetched: Vec<(Url, Result<Fetched, String>)>,
) -> (AssetBundle, HashMap<String, String>) {
    let mut bundle = AssetBundle::default();
    bundle::add_liberation_fonts(&mut bundle);
    let mut replacement_urls: HashMap<String, String> = HashMap::new();
    for (n, (url, result)) in fetched.into_iter().enumerate() {
        let kind = result.and_then(|f| {
            bundle::classify_by_content(f.bytes).map_err(|e| format!("{}: {e}", redact(&url)))
        });
        let kind = match kind {
            Ok(kind) => kind,
            Err(e) => {
                eprintln!("generate-pdf: {e}");
                continue;
            }
        };
        let replacement_url = match kind {
            Kind::Font(bytes) => {
                let key = format!("fb-asset-{n}.ttf");
                bundle::add_font(&mut bundle, &key, bytes, url.to_string());
                key
            }
            Kind::Image { mime, bytes } => data_uri(mime, &bytes),
        };
        replacement_urls.insert(url.to_string(), replacement_url);
    }
    (bundle, replacement_urls)
}

fn data_uri(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", BASE64_STANDARD.encode(bytes))
}

fn default_styles() -> String {
    format!(
        "<style>{}{BROWSER_DEFAULT_STYLES}</style>",
        bundle::liberation_font_faces_for_common_families()
    )
}

#[cfg(test)]
mod tests {
    use super::fetch::mock::MockTransport;
    use super::*;

    const TTF: &[u8] = include_bytes!("../../fonts/LiberationSerif-Bold.ttf");
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    fn prepare_with(html: &str, transport: MockTransport) -> (Prepared, Vec<String>) {
        let fetcher = Fetcher::new(transport);
        let prepared = futures::executor::block_on(prepare(html, &fetcher)).unwrap();
        let requested = fetcher.transport().requested.borrow().clone();
        (prepared, requested)
    }

    fn asset<'a>(prepared: &'a Prepared, name: &str) -> &'a fullbleed::Asset {
        prepared
            .bundle
            .assets
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("no asset {name}"))
    }

    fn base_fonts(pdf: &[u8]) -> Vec<String> {
        let text = String::from_utf8_lossy(pdf);
        text.match_indices("/BaseFont")
            .filter_map(|(i, _)| text[i..].split_whitespace().nth(1))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn inlines_linked_stylesheets_and_resolves_their_fonts_against_the_sheet_url() {
        let html = r#"<html><head><link rel="stylesheet" href="https://cdn.test/css/site.css"></head><body><p>x</p></body></html>"#;
        let transport = MockTransport::default()
            .ok(
                "https://cdn.test/css/site.css",
                "@import 'parts/type.css' print;\nbody{background:url(../img/bg.png)}",
            )
            .ok(
                "https://cdn.test/css/parts/type.css",
                "@font-face{font-family:Brand;src:url(../../fonts/brand.ttf)}",
            )
            .ok("https://cdn.test/fonts/brand.ttf", TTF)
            .ok("https://cdn.test/img/bg.png", PNG);
        let (prepared, _) = prepare_with(html, transport);

        assert!(!prepared.html.contains("<link"), "{}", prepared.html);
        let expected = format!(
            "<style>@media print{{@font-face{{font-family:Brand;src:url(fb-asset-0.ttf)}}}}body{{background:url(data:image/png;base64,{})}}</style>",
            BASE64_STANDARD.encode(PNG)
        );
        assert!(prepared.html.contains(&expected), "{}", prepared.html);
        assert_eq!(
            asset(&prepared, "fb-asset-0.ttf").kind,
            fullbleed::AssetKind::Font
        );
    }

    #[test]
    fn fetches_only_the_first_readable_candidate_of_each_font_src_list() {
        let html = r#"<style>
            @font-face{font-family:A;src:url(https://x.test/a.eot) format("embedded-opentype"),
                url(https://x.test/a.woff2) format("woff2"),url(https://x.test/a.ttf) format("truetype")}
            @font-face{font-family:B;src:local(B),url(https://x.test/b.ttf);font-weight:bold}
            p{background:url(https://x.test/1.png),url(https://x.test/2.png)}</style>"#;
        let transport = MockTransport::default()
            .ok("https://x.test/a.woff2", TTF)
            .ok("https://x.test/b.ttf", TTF)
            .ok("https://x.test/1.png", PNG)
            .ok("https://x.test/2.png", PNG);
        let (prepared, requested) = prepare_with(html, transport);
        assert_eq!(
            requested,
            vec![
                "https://x.test/a.woff2",
                "https://x.test/b.ttf",
                "https://x.test/1.png",
                "https://x.test/2.png"
            ]
        );
        assert!(
            prepared
                .html
                .contains("src:url(fb-asset-0.ttf)format(\"woff2\")}"),
            "{}",
            prepared.html
        );
    }

    #[test]
    fn inlines_img_sources_and_skips_what_cannot_be_used() {
        let photo_2 = [PNG, b"2"].concat();
        let html = r#"<head><base href="https://app.test/files/"></head>
            <img src="1/photo.png?v=1&amp;s=2"><img src=2/photo.png><img src="anim.gif">
            <img src="data:image/png;base64,AA=="><img src="missing.png"><div style='background:url("bg.png")'></div>"#;
        let transport = MockTransport::default()
            .ok("https://app.test/files/1/photo.png?v=1&s=2", PNG)
            .ok("https://app.test/files/2/photo.png", photo_2.clone())
            .ok("https://app.test/files/anim.gif", b"GIF89a".to_vec())
            .ok("https://app.test/files/bg.png", PNG);
        let (prepared, requested) = prepare_with(html, transport);

        let png = |bytes: &[u8]| format!("data:image/png;base64,{}", BASE64_STANDARD.encode(bytes));
        for expected in [
            format!(r#"<img src="{}">"#, png(PNG)),
            format!(r#"<img src="{}">"#, png(&photo_2)),
            format!(r#"style="background:url({})""#, png(PNG)),
            r#"<img src="anim.gif">"#.to_string(),
            r#"<img src="data:image/png;base64,AA==">"#.to_string(),
            r#"<img src="missing.png">"#.to_string(),
        ] {
            assert!(
                prepared.html.contains(&expected),
                "{expected}\n{}",
                prepared.html
            );
        }
        assert_eq!(
            requested.len(),
            5,
            "data: URIs are not fetched: {requested:?}"
        );
    }

    #[test]
    fn leaves_unresolvable_references_alone() {
        let html = r#"<style>@import "theme.css"; p{background:url(p.png)}</style><img src="/logo.png"><img src="ftp://x.test/a.png">"#;
        let (prepared, requested) = prepare_with(html, MockTransport::default());
        assert!(requested.is_empty());
        assert!(
            prepared
                .html
                .contains("<style>p{background:url(p.png)}</style>"),
            "{}",
            prepared.html
        );
        for untouched in [
            r#"<img src="/logo.png">"#,
            r#"<img src="ftp://x.test/a.png">"#,
        ] {
            assert!(prepared.html.contains(untouched), "{}", prepared.html);
        }
    }

    #[test]
    fn inlines_a_linked_stylesheet_that_has_a_style_attribute() {
        let html = r#"<link rel=stylesheet style="color:red" href="https://x.test/a.css">"#;
        let transport = MockTransport::default().ok("https://x.test/a.css", "p{color:blue}");
        let (prepared, _) = prepare_with(html, transport);
        assert!(!prepared.html.contains("<link"), "{}", prepared.html);
        assert!(
            prepared.html.ends_with("<style>p{color:#00f}</style>"),
            "{}",
            prepared.html
        );
    }

    #[test]
    fn injects_default_styles_into_the_head_element_only() {
        let (prepared, _) = prepare_with(
            r#"<html><!-- <head> --><head data-x></head><body><header>x</header></body></html>"#,
            MockTransport::default(),
        );
        assert!(
            prepared.html.contains("<head data-x><style>@font-face"),
            "{}",
            prepared.html
        );
        let (prepared, _) = prepare_with("<header>x</header>", MockTransport::default());
        assert!(
            prepared.html.starts_with("<style>@font-face")
                && prepared.html.ends_with("</style><header>x</header>"),
            "{}",
            prepared.html
        );
    }

    #[test]
    fn leaves_a_valueless_style_attribute_alone() {
        let (prepared, _) = prepare_with(
            r#"<div style class="x"><p style="">y</p></div>"#,
            MockTransport::default(),
        );
        assert!(
            prepared
                .html
                .ends_with(r#"<div style class="x"><p style="">y</p></div>"#),
            "{}",
            prepared.html
        );
    }

    #[test]
    fn fetched_css_cannot_close_the_style_element() {
        let html = r#"<link rel=stylesheet href="https://x.test/a.css">"#;
        let transport = MockTransport::default().ok(
            "https://x.test/a.css",
            r#"p::after{content:"</STYLE><h1>injected</h1>"}"#,
        );
        let (prepared, _) = prepare_with(html, transport);
        assert!(
            !prepared.html.to_ascii_lowercase().contains("</style><h1>"),
            "{}",
            prepared.html
        );
    }

    #[test]
    fn renders_every_family_with_an_embedded_liberation_face() {
        let fetcher = Fetcher::new(MockTransport::default());
        let html = r#"<html><head><style>@page{@bottom-center{content:counter(page);font-family:Arial}}</style></head>
            <body><h1>Title</h1><p>Hello <b>PDF</b> <i>italic</i></p>
            <p style="font-family: Arial, Helvetica, sans-serif">Arial <i>italic</i></p>
            <p style="font-family: NoSuchFont">unknown family</p></body></html>"#;
        let pdf = futures::executor::block_on(render(html, &fetcher)).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let fonts = base_fonts(&pdf);
        for face in [
            "LiberationSerif",
            "LiberationSerif-Bold",
            "LiberationSerif-Italic",
            "LiberationSans",
            "LiberationSans-Italic",
        ] {
            assert!(
                fonts.iter().any(|f| f.ends_with(&format!("+{face}"))),
                "{face} should be embedded: {fonts:?}"
            );
        }
        assert!(
            fonts.iter().all(|f| f.contains('+')),
            "a built-in PDF font was used instead of an embedded one: {fonts:?}"
        );
    }

    #[test]
    fn renders_a_family_name_that_lightningcss_writes_without_quotes() {
        let fetcher = Fetcher::new(MockTransport::default());
        for html in [
            r#"<style>p{font-family:"Helvetica Neue"}</style><p>x</p>"#,
            r#"<p style='font-family:"Helvetica Neue"'>x</p>"#,
        ] {
            let pdf = futures::executor::block_on(render(html, &fetcher)).unwrap();
            let fonts = base_fonts(&pdf);
            assert!(
                fonts.iter().all(|f| f.ends_with("+LiberationSans")),
                "{html}: {fonts:?}"
            );
        }
    }
}

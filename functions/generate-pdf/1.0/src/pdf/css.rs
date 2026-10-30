use std::collections::HashMap;

use url::Url;

use super::fetch::{Fetcher, Transport};
use super::refs::{self, Import};

const MAX_IMPORT_DEPTH: usize = 4;

pub enum CssLocation {
    StyleElement,
    StyleAttribute,
    LinkedStylesheet { media: Option<String> },
}

impl CssLocation {
    pub fn is_style_attribute(&self) -> bool {
        matches!(self, Self::StyleAttribute)
    }
}

pub async fn prepare_stylesheet<T: Transport>(
    fetcher: &Fetcher<T>,
    css: &str,
    base: Option<&Url>,
    depth: usize,
) -> String {
    let extracted = refs::extract_imports(css);
    let imported_css = load_imports(fetcher, extracted.imports, base, depth).await;
    let own_css = prepare_own_css(fetcher, &extracted.own_css, base);
    imported_css + &own_css
}

fn prepare_own_css<T: Transport>(
    fetcher: &Fetcher<T>,
    own_css: &str,
    base: Option<&Url>,
) -> String {
    let own_css = refs::keep_the_first_readable_font_source(own_css);
    absolutize_urls(fetcher, &own_css, false, base)
}

async fn load_imports<T: Transport>(
    fetcher: &Fetcher<T>,
    imports: Vec<Import>,
    base: Option<&Url>,
    depth: usize,
) -> String {
    if depth >= MAX_IMPORT_DEPTH {
        for import in &imports {
            eprintln!(
                "generate-pdf: {}: @import nested too deep, left out",
                import.url
            );
        }
        return String::new();
    }
    let loads = imports.iter().filter_map(|import| {
        let url = fetcher.resolve(base, &import.url)?;
        Some(load_import(fetcher, url, &import.media, depth))
    });
    futures::future::join_all(loads).await.concat()
}

async fn load_import<T: Transport>(
    fetcher: &Fetcher<T>,
    url: Url,
    media: &str,
    depth: usize,
) -> String {
    let fetched = match fetcher.fetch(&url).await {
        Ok(fetched) => fetched,
        Err(e) => {
            eprintln!("generate-pdf: {e}");
            return String::new();
        }
    };
    let css = String::from_utf8_lossy(&fetched.bytes);
    let sheet = Box::pin(prepare_stylesheet(
        fetcher,
        &css,
        Some(&fetched.final_url),
        depth + 1,
    ))
    .await;
    restrict_to_media(sheet, media)
}

fn restrict_to_media(css: String, media: &str) -> String {
    if media.is_empty() {
        css
    } else {
        format!("@media {media}{{{css}}}")
    }
}

pub fn absolutize_urls<T: Transport>(
    fetcher: &Fetcher<T>,
    css: &str,
    is_style_attribute: bool,
    base: Option<&Url>,
) -> String {
    refs::rewrite_urls(css, is_style_attribute, |url| {
        fetcher
            .resolve(base, url)
            .map(|absolute| absolute.to_string())
    })
}

pub fn replace_urls(
    css: &str,
    is_style_attribute: bool,
    replacement_urls: &HashMap<String, String>,
) -> String {
    refs::rewrite_urls(css, is_style_attribute, |url| {
        let url = Url::parse(url).ok()?;
        replacement_urls.get(url.as_str()).cloned()
    })
}

pub fn write_css(css: &str, location: CssLocation) -> String {
    match location {
        CssLocation::StyleElement => escape_closing_style_tags(css),
        CssLocation::StyleAttribute => css.replace('&', "&amp;"),
        CssLocation::LinkedStylesheet { media } => {
            let css = escape_closing_style_tags(css);
            match restricting_media(media) {
                Some(media) => format!("<style>@media {media}{{{css}}}</style>"),
                None => format!("<style>{css}</style>"),
            }
        }
    }
}

fn restricting_media(media: Option<String>) -> Option<String> {
    media.filter(|m| !m.trim().is_empty() && !m.trim().eq_ignore_ascii_case("all"))
}

fn escape_closing_style_tags(css: &str) -> String {
    css.replace("</", "<\\/")
}

#[cfg(test)]
mod tests {
    use super::super::fetch::mock::MockTransport;
    use super::*;

    #[test]
    fn puts_the_imported_css_before_the_own_css_of_the_sheet() {
        let fetcher = Fetcher::new(MockTransport::default().ok(
            "https://x.test/fonts.css",
            "@font-face { font-family: Brand; src: url(brand.ttf) }",
        ));
        let site_css = r#"@import url("fonts.css"); body { color: black }"#;
        let base = Url::parse("https://x.test/site.css").unwrap();
        let prepared =
            futures::executor::block_on(prepare_stylesheet(&fetcher, site_css, Some(&base), 0));
        assert_eq!(
            prepared,
            "@font-face{font-family:Brand;src:url(https://x.test/brand.ttf)}body{color:#000}"
        );
    }
}

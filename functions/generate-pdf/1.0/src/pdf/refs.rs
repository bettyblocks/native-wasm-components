use std::convert::Infallible;

use lightningcss::media_query::MediaList;
use lightningcss::rules::CssRule;
use lightningcss::rules::font_face::{FontFaceProperty, FontFormat, Source};
use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleAttribute, StyleSheet};
use lightningcss::traits::ToCss;
use lightningcss::values::image::Image;
use lightningcss::values::url::Url;
use lightningcss::visit_types;
use lightningcss::visitor::{Visit, VisitTypes, Visitor};

#[derive(Debug, PartialEq)]
pub struct Import {
    pub url: String,
    pub media: String,
}

pub struct ExtractedImports {
    pub imports: Vec<Import>,
    pub own_css: String,
}

pub fn extract_imports(css: &str) -> ExtractedImports {
    let unchanged = || ExtractedImports {
        imports: Vec::new(),
        own_css: css.to_string(),
    };
    let Ok(mut sheet) = StyleSheet::parse(css, recovering()) else {
        return unchanged();
    };
    let mut imports = Imports(Vec::new());
    let _ = sheet.visit(&mut imports);
    match sheet.to_css(minified()) {
        Ok(printed) => ExtractedImports {
            imports: imports.0,
            own_css: printed.code,
        },
        Err(_) => unchanged(),
    }
}

pub fn keep_the_first_readable_font_source(css: &str) -> String {
    let Ok(mut sheet) = StyleSheet::parse(css, recovering()) else {
        return css.to_string();
    };
    let _ = sheet.visit(&mut FontSources);
    sheet
        .to_css(minified())
        .map_or_else(|_| css.to_string(), |printed| printed.code)
}

pub fn rewrite_urls(
    css: &str,
    is_style_attribute: bool,
    rewrite: impl FnMut(&str) -> Option<String>,
) -> String {
    let mut urls = Urls(rewrite);
    let printed = if is_style_attribute {
        StyleAttribute::parse(css, recovering())
            .ok()
            .and_then(|mut attribute| {
                let _ = attribute.visit(&mut urls);
                attribute.to_css(minified()).ok()
            })
    } else {
        StyleSheet::parse(css, recovering())
            .ok()
            .and_then(|mut sheet| {
                let _ = sheet.visit(&mut urls);
                sheet.to_css(minified()).ok()
            })
    };
    printed.map_or_else(|| css.to_string(), |printed| printed.code)
}

pub fn urls(css: &str, is_style_attribute: bool) -> Vec<String> {
    let mut urls = Vec::new();
    rewrite_urls(css, is_style_attribute, |url| {
        urls.push(url.to_string());
        None
    });
    urls
}

fn recovering<'i>() -> ParserOptions<'i> {
    ParserOptions {
        error_recovery: true,
        ..ParserOptions::default()
    }
}

fn minified() -> PrinterOptions<'static> {
    PrinterOptions {
        minify: true,
        ..PrinterOptions::default()
    }
}

fn media_text(media: &MediaList) -> String {
    if media.media_queries.is_empty() {
        return String::new();
    }
    media
        .to_css_string(PrinterOptions::default())
        .unwrap_or_default()
}

struct Imports(Vec<Import>);

impl<'i> Visitor<'i> for Imports {
    type Error = Infallible;

    fn visit_types(&self) -> VisitTypes {
        visit_types!(RULES)
    }

    fn visit_rule(&mut self, rule: &mut CssRule<'i>) -> Result<(), Infallible> {
        if let CssRule::Import(import) = rule {
            self.0.push(Import {
                url: import.url.to_string(),
                media: media_text(&import.media),
            });
            *rule = CssRule::Ignored;
        }
        Ok(())
    }
}

struct FontSources;

impl<'i> Visitor<'i> for FontSources {
    type Error = Infallible;

    fn visit_types(&self) -> VisitTypes {
        visit_types!(RULES)
    }

    fn visit_rule(&mut self, rule: &mut CssRule<'i>) -> Result<(), Infallible> {
        let CssRule::FontFace(font_face) = rule else {
            return rule.visit_children(self);
        };
        for property in &mut font_face.properties {
            if let FontFaceProperty::Source(sources) = property {
                keep_the_first_readable_url(sources);
            }
        }
        Ok(())
    }
}

fn keep_the_first_readable_url(sources: &mut Vec<Source>) {
    let mut found = false;
    sources.retain(|source| match source {
        Source::Local(_) => true,
        Source::Url(source) => {
            let keep = !found && fullbleed_reads(&source.format);
            found |= keep;
            keep
        }
    });
}

fn fullbleed_reads(format: &Option<FontFormat>) -> bool {
    !matches!(format, Some(FontFormat::EmbeddedOpenType | FontFormat::SVG))
}

struct Urls<F>(F);

impl<'i, F: FnMut(&str) -> Option<String>> Visitor<'i> for Urls<F> {
    type Error = Infallible;

    fn visit_types(&self) -> VisitTypes {
        visit_types!(URLS | IMAGES)
    }

    fn visit_url(&mut self, url: &mut Url<'i>) -> Result<(), Infallible> {
        if let Some(rewritten) = (self.0)(&url.url) {
            url.url = rewritten.into();
        }
        Ok(())
    }

    fn visit_image(&mut self, image: &mut Image<'i>) -> Result<(), Infallible> {
        let Image::ImageSet(image_set) = image else {
            return image.visit_children(self);
        };
        for option in &mut image_set.options {
            option.image.visit(self)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_urls_and_imports_but_not_inside_comments_or_strings() {
        let css = r#"
            @import url("https://a.test/base.css") print;
            @import 'https://a.test/other.css';
            /* background: url(https://a.test/commented.png) */
            .q::before { content: "url(https://a.test/in-string.png)"; }
            .bg { background: URL( https://a.test/bg.png ) no-repeat; }
            .set { background-image: image-set(url(low.png) 1x, "high.png" 2x); }
            .x { --curl(foo): 1; }
            .after { background: url(after.png); }
        "#;
        assert_eq!(
            extract_imports(css).imports,
            vec![
                Import {
                    url: "https://a.test/base.css".into(),
                    media: "print".into()
                },
                Import {
                    url: "https://a.test/other.css".into(),
                    media: "".into()
                },
            ]
        );
        assert_eq!(
            urls(css, false),
            vec!["https://a.test/bg.png", "low.png", "high.png", "after.png"]
        );
    }

    #[test]
    fn keeps_local_fonts_and_the_first_font_url_fullbleed_reads() {
        let css = r#"@font-face { src: local(A), url(a.eot) format("embedded-opentype"), url(a.woff2) format("woff2"), url(a.ttf) }"#;
        assert_eq!(
            keep_the_first_readable_font_source(css),
            r#"@font-face{src:local(A),url(a.woff2)format("woff2")}"#
        );
    }

    #[test]
    fn rewrites_urls_in_a_style_attribute() {
        let css = "background: url(x.png) red; list-style-image: url('y.png')";
        assert_eq!(
            rewrite_urls(css, true, |url| Some(format!("k-{url}"))),
            "background:red url(k-x.png);list-style-image:url(k-y.png)"
        );
    }
}

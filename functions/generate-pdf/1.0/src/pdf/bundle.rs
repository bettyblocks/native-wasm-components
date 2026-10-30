use fullbleed::{Asset, AssetBundle, AssetKind};

#[derive(Debug, PartialEq)]
pub enum Kind {
    Font(Vec<u8>),
    Image { mime: &'static str, bytes: Vec<u8> },
}

pub fn classify_by_content(bytes: Vec<u8>) -> Result<Kind, String> {
    match bytes.get(..4) {
        Some([0, 1, 0, 0] | b"OTTO" | b"true" | b"ttcf") => return Ok(Kind::Font(bytes)),
        Some(b"wOFF") => {
            return wuff::decompress_woff1(&bytes)
                .map(Kind::Font)
                .map_err(|e| format!("bad WOFF font: {e:?}"));
        }
        Some(b"wOF2") => {
            return wuff::decompress_woff2(&bytes)
                .map(Kind::Font)
                .map_err(|e| format!("bad WOFF2 font: {e:?}"));
        }
        _ => {}
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok(Kind::Image {
            mime: "image/png",
            bytes,
        });
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Ok(Kind::Image {
            mime: "image/jpeg",
            bytes,
        });
    }
    if is_svg(&bytes) {
        return Ok(Kind::Image {
            mime: "image/svg+xml",
            bytes,
        });
    }
    Err(if bytes.starts_with(b"GIF8") {
        "GIF images are not supported".to_string()
    } else if bytes.get(8..12) == Some(b"WEBP") {
        "WebP images are not supported".to_string()
    } else if bytes.get(4..11) == Some(b"ftypavi") {
        "AVIF images are not supported".to_string()
    } else {
        "not a font, PNG, JPEG or SVG".to_string()
    })
}

fn is_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    let head = String::from_utf8_lossy(head);
    let head = head.trim_start_matches('\u{feff}').trim_start();
    (head.starts_with("<svg") || head.starts_with("<?xml") || head.starts_with("<!--"))
        && head.contains("<svg")
}

pub fn add_font(bundle: &mut AssetBundle, key: &str, bytes: Vec<u8>, source: String) {
    bundle.add(Asset::new(
        key.to_string(),
        AssetKind::Font,
        bytes,
        Some(source),
        true,
    ));
}

macro_rules! font_asset {
    ($name:literal) => {
        font_asset!($name => $name)
    };
    ($name:literal => $file:literal) => {
        (
            concat!($name, ".ttf"),
            include_bytes!(concat!("../../fonts/", $file, ".ttf")) as &[u8],
        )
    };
}

const LIBERATION_FONTS: [(&str, &[u8]); 8] = [
    font_asset!("LiberationSans-Regular"),
    font_asset!("LiberationSans-Bold"),
    font_asset!("LiberationSans-Italic"),
    font_asset!("LiberationSans-BoldItalic"),
    font_asset!("LiberationSerif-Regular"),
    font_asset!("LiberationSerif-Bold"),
    font_asset!("LiberationSerif-Italic"),
    font_asset!("LiberationSerif-BoldItalic"),
];

const FULLBLEED_FALLBACK_FONT_FILES: [(&str, &[u8]); 4] = [
    font_asset!("Helvetica" => "LiberationSans-Regular"),
    font_asset!("Arial" => "LiberationSans-Regular"),
    font_asset!("Times-Roman" => "LiberationSerif-Regular"),
    font_asset!("Times New Roman" => "LiberationSerif-Regular"),
];

pub fn add_liberation_fonts(bundle: &mut AssetBundle) {
    for (file, bytes) in LIBERATION_FONTS
        .into_iter()
        .chain(FULLBLEED_FALLBACK_FONT_FILES)
    {
        bundle.add(Asset::new(
            file.to_string(),
            AssetKind::Font,
            bytes.to_vec(),
            None,
            true,
        ));
    }
}

pub fn liberation_font_faces_for_common_families() -> String {
    const STYLES: [(&str, &str, &str); 4] = [
        ("Regular", "normal", "normal"),
        ("Bold", "bold", "normal"),
        ("Italic", "normal", "italic"),
        ("BoldItalic", "bold", "italic"),
    ];
    let common_families_by_liberation_family = [
        (
            "LiberationSans",
            ["Arial", "Helvetica", "Helvetica Neue"].as_slice(),
        ),
        ("LiberationSerif", ["Times New Roman", "Times"].as_slice()),
    ];
    let mut css = String::new();
    for (liberation_family, common_families) in common_families_by_liberation_family {
        for family in common_families {
            for (style, weight, slant) in STYLES {
                css.push_str(&format!(
                    "@font-face{{font-family:\"{family}\";font-weight:{weight};font-style:{slant};src:url({liberation_family}-{style}.ttf)}}"
                ));
            }
        }
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_content_not_by_name() {
        let ttf = include_bytes!("../../fonts/LiberationSans-Regular.ttf").to_vec();
        assert_eq!(classify_by_content(ttf.clone()), Ok(Kind::Font(ttf)));
        assert!(matches!(
            classify_by_content(b"\x89PNG\r\n\x1a\nrest".to_vec()),
            Ok(Kind::Image {
                mime: "image/png",
                ..
            })
        ));
        assert!(matches!(
            classify_by_content(b"\xff\xd8\xff\xe0rest".to_vec()),
            Ok(Kind::Image {
                mime: "image/jpeg",
                ..
            })
        ));
        assert!(matches!(
            classify_by_content(
                b"\xef\xbb\xbf<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>"
                    .to_vec()
            ),
            Ok(Kind::Image {
                mime: "image/svg+xml",
                ..
            })
        ));
        assert_eq!(
            classify_by_content(b"GIF89a....".to_vec()),
            Err("GIF images are not supported".into())
        );
        assert_eq!(
            classify_by_content(b"RIFF\0\0\0\0WEBPVP8 ".to_vec()),
            Err("WebP images are not supported".into())
        );
        assert_eq!(
            classify_by_content(b"<html>404</html>".to_vec()),
            Err("not a font, PNG, JPEG or SVG".into())
        );
    }
}

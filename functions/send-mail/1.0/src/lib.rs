mod renderer;
mod types;

wit_bindgen::generate!({
    world: "email",
    generate_all,
});

use betty_blocks::smtp::client::{
    self, Attachment, Credentials, Message, Recipient, Sender, TlsMode,
};
use exports::betty_blocks::send_mail::send_mail::{
    CollectionHandle, Guest, Input, JsonString, KeyValue, BettyPropertyPath,
};
use futures::future::join_all;
use tracing::debug;
use types::{CollectionData, FileInfo, SendMailOutput, UrlField};
use wstd::http::{Client, Request};

struct SendMailComponent;

impl Guest for SendMailComponent {
    fn send_mail(input: Input) -> Result<JsonString, String> {
        let tls_mode = resolve_tls_mode(input.secure, input.port);

        let creds = Credentials {
            host: input.host,
            port: Some(input.port),
            username: Some(input.username),
            password: Some(input.password),
            tls_mode,
        };

        let attachments = build_attachments(
            input.attachments,
            input.attachments_col,
            input.attachments_col_property,
        )?;

        let variables = input
            .variables
            .map(|vars| vars.into_iter().map(|kv| (kv.key, kv.value)).collect());

        let body = renderer::render_body(input.body.unwrap_or_default(), variables)?;

        let message = Message {
            sender: Sender {
                from: input.sender_email,
                reply_to: input.reply_to,
                display_name: input.sender_name,
            },
            recipient: Recipient {
                to: vec![input.to_email],
                cc: input.cc.map(|s| vec![s]),
                bcc: input.bcc.map(|s| vec![s]),
            },
            subject: input.subject.unwrap_or_default(),
            body,
            attachments,
        };

        let connection_key = client::connect(&creds)?;

        let result = client::send(&connection_key, &message)?;

        client::disconnect(&connection_key)?;

        serde_json::to_string(&SendMailOutput {
            result: result.into(),
        })
        .map_err(|e| format!("Serialization failed: {e}"))
    }
}

fn resolve_tls_mode(secure: Option<bool>, port: u16) -> TlsMode {
    if port == 465 {
        return TlsMode::Implicit;
    }
    match secure {
        Some(false) => TlsMode::None,
        _ => TlsMode::Starttls,
    }
}

fn collect_map_attachments(map_attachments: Option<Vec<KeyValue>>) -> Vec<(String, String)> {
    let Some(list) = map_attachments else {
        return Vec::new();
    };

    list.into_iter()
        .filter_map(|kv| {
            let url = extract_url(&kv.value);
            if kv.key.is_empty() || url.is_empty() {
                debug!("Skipping map attachment: empty filename or url");
                return None;
            }
            Some((kv.key, url))
        })
        .collect()
}

fn collect_col_attachments(
    col: Option<CollectionHandle>,
    props: Option<Vec<BettyPropertyPath>>,
) -> Result<Vec<(String, String)>, String> {
    let (Some(handle), Some(props)) = (col, props) else {
        return Ok(Vec::new());
    };

    let Some(col_json) = handle.data else {
        return Ok(Vec::new());
    };

    let col_data: CollectionData =
        serde_json::from_str(&col_json).map_err(|e| format!("Invalid attachmentsCol: {e}"))?;
    let prop_name = &props.first().ok_or("attachmentsColProperty is empty")?.name;

    Ok(col_data
        .data
        .into_iter()
        .filter_map(|mut item| {
            let file = item.remove(prop_name)?;
            match file {
                // both name and url are present
                FileInfo {
                    name,
                    url: Some(url),
                } if !name.is_empty() && !url.is_empty() => Some((name, url)),
                // name is missing so we derive filename from the url (unsure about this)
                FileInfo { url: Some(url), .. } if !url.is_empty() => {
                    let filename = filename_from_url(&url)?;
                    Some((filename, url))
                }
                // no valid url, skip it
                _ => {
                    debug!("Skipping collection attachment: missing or empty url");
                    None
                }
            }
        })
        .collect())
}

fn build_attachments(
    map_attachments: Option<Vec<KeyValue>>,
    col: Option<CollectionHandle>,
    props: Option<Vec<BettyPropertyPath>>,
) -> Result<Option<Vec<Attachment>>, String> {
    let mut files = collect_map_attachments(map_attachments);
    files.extend(collect_col_attachments(col, props)?);

    if files.is_empty() {
        return Ok(None);
    }

    let attachments = download_files(&Client::new(), files)?;
    Ok(Some(attachments))
}

fn extract_url(value: &str) -> String {
    match serde_json::from_str::<UrlField>(value) {
        Ok(parsed) => parsed.url,
        Err(_) => value.to_string(),
    }
}

fn filename_from_url(url: &str) -> Option<String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return None;
    }
    let path = url.split('?').next().unwrap_or(url);
    let filename = path.rsplit('/').next().unwrap_or(path);
    if filename.is_empty() {
        return None;
    }
    Some(filename.to_string())
}

fn download_files(
    client: &Client,
    files: Vec<(String, String)>,
) -> Result<Vec<Attachment>, String> {
    wstd::runtime::block_on(async {
        let futures = files.into_iter().map(|(filename, url)| {
            let client = client.clone();
            async move {
                let content = download_url(&client, &url).await?;
                let content_type = mime_guess::from_path(&filename)
                    .first_or_octet_stream()
                    .to_string();
                let attachment = Attachment {
                    filename,
                    content_type,
                    content,
                };
                Ok::<Attachment, String>(attachment)
            }
        });

        let attachments = join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        Ok(attachments)
    })
}

async fn download_url(client: &Client, url: &str) -> Result<Vec<u8>, String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(format!("Unsupported URL scheme: {url}"));
    }

    let req = Request::get(url)
        .body(())
        .map_err(|e| format!("Failed to build request: {e}"))?;

    let mut response = client
        .send(req)
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP request failed with status: {status}"));
    }

    let body = response
        .body_mut()
        .contents()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    Ok(body.to_vec())
}

export!(SendMailComponent);

#[cfg(test)]
mod tests {
    use super::{
        collect_col_attachments, collect_map_attachments, resolve_tls_mode, CollectionHandle,
        KeyValue, BettyPropertyPath, TlsMode,
    };

    fn handle(json: &str) -> CollectionHandle {
        CollectionHandle {
            data: Some(json.to_string()),
        }
    }

    fn prop_path(name: &str) -> BettyPropertyPath {
        BettyPropertyPath {
            name: name.to_string(),
            kind: String::new(),
            object_fields: None,
        }
    }

    #[test]
    fn tls_port_465_always_implicit() {
        assert!(matches!(
            resolve_tls_mode(Some(true), 465),
            TlsMode::Implicit
        ));
        assert!(matches!(
            resolve_tls_mode(Some(false), 465),
            TlsMode::Implicit
        ));
        assert!(matches!(resolve_tls_mode(None, 465), TlsMode::Implicit));
    }

    #[test]
    fn tls_starttls_by_default() {
        assert!(matches!(resolve_tls_mode(None, 25), TlsMode::Starttls));
        assert!(matches!(resolve_tls_mode(None, 587), TlsMode::Starttls));
        assert!(matches!(resolve_tls_mode(None, 2525), TlsMode::Starttls));
        assert!(matches!(
            resolve_tls_mode(Some(true), 587),
            TlsMode::Starttls
        ));
        assert!(matches!(
            resolve_tls_mode(Some(true), 25),
            TlsMode::Starttls
        ));
    }

    #[test]
    fn tls_explicit_false_is_none() {
        assert!(matches!(resolve_tls_mode(Some(false), 25), TlsMode::None));
        assert!(matches!(resolve_tls_mode(Some(false), 587), TlsMode::None));
        assert!(matches!(
            resolve_tls_mode(Some(false), 1025),
            TlsMode::None
        ));
        assert!(matches!(resolve_tls_mode(Some(false), 2525), TlsMode::None));
    }

    #[test]
    fn extract_url_plain_string() {
        assert_eq!(
            super::extract_url("https://example.com/file.pdf"),
            "https://example.com/file.pdf"
        );
    }

    #[test]
    fn extract_url_json_object_with_url() {
        assert_eq!(
            super::extract_url(r#"{"url":"https://example.com/file.pdf"}"#),
            "https://example.com/file.pdf"
        );
    }

    #[test]
    fn extract_url_json_object_without_url() {
        assert_eq!(
            super::extract_url(r#"{"name":"file.pdf"}"#),
            r#"{"name":"file.pdf"}"#
        );
    }

    #[test]
    fn extract_url_empty() {
        assert_eq!(super::extract_url(""), "");
    }

    #[test]
    fn mime_pdf() {
        let mime = mime_guess::from_path("report.pdf")
            .first_or_octet_stream()
            .to_string();
        assert_eq!(mime, "application/pdf");
    }

    #[test]
    fn mime_unknown_falls_back_to_octet_stream() {
        let mime = mime_guess::from_path("file.unknownext")
            .first_or_octet_stream()
            .to_string();
        assert_eq!(mime, "application/octet-stream");
    }

    // collect_map_attachments tests

    fn kv(key: &str, value: &str) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn map_attachments_none() {
        assert_eq!(
            collect_map_attachments(None),
            Vec::<(String, String)>::new()
        );
    }

    #[test]
    fn map_attachments_empty_list() {
        assert_eq!(
            collect_map_attachments(Some(vec![])),
            Vec::<(String, String)>::new()
        );
    }

    #[test]
    fn map_attachments_plain_url() {
        let result = collect_map_attachments(Some(vec![kv(
            "report.pdf",
            "https://example.com/report.pdf",
        )]));
        assert_eq!(
            result,
            vec![(
                "report.pdf".to_string(),
                "https://example.com/report.pdf".to_string()
            )]
        );
    }

    #[test]
    fn map_attachments_json_url() {
        let result = collect_map_attachments(Some(vec![kv(
            "file.pdf",
            r#"{"url":"https://example.com/file.pdf"}"#,
        )]));
        assert_eq!(
            result,
            vec![(
                "file.pdf".to_string(),
                "https://example.com/file.pdf".to_string()
            )]
        );
    }

    #[test]
    fn map_attachments_skips_empty_key() {
        let result = collect_map_attachments(Some(vec![kv("", "https://example.com/file.pdf")]));
        assert!(result.is_empty());
    }

    #[test]
    fn map_attachments_skips_empty_value() {
        let result = collect_map_attachments(Some(vec![kv("file.pdf", "")]));
        assert!(result.is_empty());
    }

    #[test]
    fn map_attachments_multiple_mixed() {
        let result = collect_map_attachments(Some(vec![
            kv("a.pdf", "https://example.com/a.pdf"),
            kv("", "https://example.com/skip.pdf"),
            kv("b.pdf", r#"{"url":"https://example.com/b.pdf"}"#),
            kv("c.pdf", ""),
        ]));
        assert_eq!(
            result,
            vec![
                ("a.pdf".to_string(), "https://example.com/a.pdf".to_string()),
                ("b.pdf".to_string(), "https://example.com/b.pdf".to_string()),
            ]
        );
    }

    // collect_col_attachments tests

    #[test]
    fn col_attachments_none_inputs() {
        assert_eq!(collect_col_attachments(None, None).unwrap(), vec![]);
        assert_eq!(
            collect_col_attachments(Some(handle("{}")), None).unwrap(),
            vec![]
        );
        assert_eq!(
            collect_col_attachments(None, Some(vec![])).unwrap(),
            vec![]
        );
    }

    #[test]
    fn col_attachments_handle_with_no_data() {
        let result = collect_col_attachments(
            Some(CollectionHandle { data: None }),
            Some(vec![prop_path("file")]),
        )
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn col_attachments_with_url() {
        let col = r#"{"data":[{"file":{"name":"doc.pdf","url":"https://example.com/doc.pdf"}}]}"#;
        let result =
            collect_col_attachments(Some(handle(col)), Some(vec![prop_path("file")])).unwrap();
        assert_eq!(
            result,
            vec![(
                "doc.pdf".to_string(),
                "https://example.com/doc.pdf".to_string()
            )]
        );
    }

    #[test]
    fn col_attachments_without_url_is_skipped() {
        let col = r#"{"data":[{"file":{"name":"https://example.com/raw.pdf"}}]}"#;
        let result =
            collect_col_attachments(Some(handle(col)), Some(vec![prop_path("file")])).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn col_attachments_empty_name_extracts_from_url() {
        let col = r#"{"data":[{"file":{"name":"","url":"https://example.com/doc.pdf"}}]}"#;
        let result =
            collect_col_attachments(Some(handle(col)), Some(vec![prop_path("file")])).unwrap();
        assert_eq!(
            result,
            vec![(
                "doc.pdf".to_string(),
                "https://example.com/doc.pdf".to_string()
            )]
        );
    }

    #[test]
    fn col_attachments_skips_missing_prop() {
        let col = r#"{"data":[{"other":{"name":"doc.pdf","url":"https://example.com/doc.pdf"}}]}"#;
        let result =
            collect_col_attachments(Some(handle(col)), Some(vec![prop_path("file")])).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn col_attachments_invalid_col_json() {
        let result =
            collect_col_attachments(Some(handle("not json")), Some(vec![prop_path("file")]));
        assert!(result.is_err());
    }

    #[test]
    fn col_attachments_empty_prop_list() {
        let result = collect_col_attachments(Some(handle(r#"{"data":[]}"#)), Some(vec![]));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "attachmentsColProperty is empty");
    }
}

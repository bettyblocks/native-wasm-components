mod renderer;
mod types;

wit_bindgen::generate!({
    world: "email",
    generate_all,
});

use std::borrow::Cow;

use betty_blocks::smtp::client::{
    self, Attachment, Credentials, Message, Recipient, Sender, TlsMode,
};
use exports::betty_blocks::send_mail::send_mail::{Guest, Input, JsonString, KeyValue};
use tracing::debug;
use types::{CollectionData, PropertySpec, SendMailOutput, UrlField};
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
            &input.attachments_col,
            &input.attachments_col_property,
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
    match port {
        465 => TlsMode::Implicit, // reserve port 465 for TLS
        _ => match secure {
            Some(false) => TlsMode::None, // any other port is TLS unless explicitly specified as non-secure
            _ => TlsMode::Implicit,
        },
    }
}

fn collect_map_attachments(map_attachments: Option<Vec<KeyValue>>) -> Vec<(String, String)> {
    let Some(list) = map_attachments else {
        return Vec::new();
    };

    list.into_iter()
        .filter_map(|kv| {
            let url = extract_url(&kv.value);
            match (kv.key.as_str(), &*url) {
                ("", _) => {
                    debug!("Skipping map attachment: empty filename");
                    None
                }
                (_, "") => {
                    debug!("Skipping map attachment: empty url");
                    None
                }
                _ => Some((kv.key, url.into_owned())),
            }
        })
        .collect()
}

fn collect_col_attachments(
    col_json: &Option<String>,
    prop_json: &Option<String>,
) -> Result<Vec<(String, String)>, String> {
    let (Some(col), Some(prop)) = (col_json, prop_json) else {
        return Ok(Vec::new());
    };

    let col_data: CollectionData =
        serde_json::from_str(col).map_err(|e| format!("Invalid attachmentsCol: {e}"))?;
    let props: Vec<PropertySpec> =
        serde_json::from_str(prop).map_err(|e| format!("Invalid attachmentsColProperty: {e}"))?;
    let prop_name = &props.first().ok_or("attachmentsColProperty is empty")?.name;

    Ok(col_data
        .data
        .iter()
        .filter_map(|item| {
            let file = item.get(prop_name)?;
            let url = file.url.as_deref().unwrap_or_default();
            match (file.name.as_str(), url) {
                ("", _) | (_, "") => {
                    debug!("Skipping collection attachment: empty name or url");
                    None
                }
                (name, url) => Some((name.to_string(), url.to_string())),
            }
        })
        .collect())
}

fn build_attachments(
    map_attachments: Option<Vec<KeyValue>>,
    col_json: &Option<String>,
    prop_json: &Option<String>,
) -> Result<Option<Vec<Attachment>>, String> {
    let mut files = collect_map_attachments(map_attachments);
    files.extend(collect_col_attachments(col_json, prop_json)?);

    if files.is_empty() {
        return Ok(None);
    }

    let attachments = download_files(&Client::new(), files)?;
    Ok(Some(attachments))
}

fn extract_url(value: &str) -> Cow<'_, str> {
    match serde_json::from_str::<UrlField>(value) {
        Ok(parsed) => Cow::Owned(parsed.url),
        Err(_) => Cow::Borrowed(value),
    }
}

fn download_files(
    client: &Client,
    files: Vec<(String, String)>,
) -> Result<Vec<Attachment>, String> {
    wstd::runtime::block_on(async {
        let mut attachments = Vec::new();
        for (filename, url) in files {
            let content = download_url(client, &url).await?;
            let content_type = mime_guess::from_path(&filename)
                .first_or_octet_stream()
                .to_string();
            attachments.push(Attachment {
                filename,
                content_type,
                content,
            });
        }
        Ok(attachments)
    })
}

async fn download_url(client: &Client, url: &str) -> Result<Vec<u8>, String> {
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

    response
        .body_mut()
        .contents()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Failed to read response body: {e}"))
}

export!(SendMailComponent);

#[cfg(test)]
mod tests {
    use super::{resolve_tls_mode, TlsMode};

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
    fn tls_secure_by_default() {
        assert!(matches!(resolve_tls_mode(None, 25), TlsMode::Implicit));
        assert!(matches!(
            resolve_tls_mode(Some(true), 587),
            TlsMode::Implicit
        ));
        assert!(matches!(resolve_tls_mode(None, 2525), TlsMode::Implicit));
    }

    #[test]
    fn tls_explicit_false_is_none() {
        assert!(matches!(resolve_tls_mode(Some(false), 25), TlsMode::None));
        assert!(matches!(resolve_tls_mode(Some(false), 587), TlsMode::None));
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
}

use std::collections::HashMap;
use wstd::http::{Body, Client, Request, Response, StatusCode};

mod types;
use types::{Input, SendResult};

wit_bindgen::generate!({
    world: "email",
    generate_all,
});

use betty_blocks::smtp::client::{
    self, Attachment, Credentials, Message, Recipient, Sender, TlsMode,
};

#[wstd::http_server]
async fn main(request: Request<Body>) -> Result<Response<Body>, wstd::http::Error> {
    Ok(handle(request).await)
}

async fn handle(request: Request<Body>) -> Response<Body> {
    let input: Input = match request.into_body().json().await {
        Ok(v) => v,
        Err(e) => {
            return error_response(
                StatusCode::PRECONDITION_FAILED,
                &format!("Invalid body: {e}"),
            );
        }
    };

    match send_mail(input).await {
        Ok(json) => Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(json))
            .unwrap_or_else(|_| Response::new(Body::from("Internal Server Error"))),
        Err(e) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Failed to send email: {e}"),
        ),
    }
}

async fn send_mail(input: Input) -> Result<String, String> {
    let tls_mode = resolve_tls_mode(input.secure, input.port);

    let creds = Credentials {
        host: input.host,
        port: Some(input.port),
        username: input.username,
        password: input.password,
        tls_mode,
    };

    let connection_key = client::connect(&creds).map_err(|e| format!("Connect failed: {e}"))?;

    let attachments = build_attachments(input.attachments).await?;

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
        body: input.body.unwrap_or_default(),
        attachments,
    };

    let result =
        client::send(&connection_key, &message).map_err(|e| format!("Send failed: {e}"))?;

    if let Err(e) = client::disconnect(&connection_key) {
        eprintln!("Failed to disconnect SMTP connection: {e}");
    }

    serde_json::to_string(&SendResult {
        accepted: result.accepted,
        server: result.server,
        message_id: result.message_id,
    })
    .map_err(|e| format!("Serialization failed: {e}"))
}

fn resolve_tls_mode(secure: Option<bool>, port: u16) -> TlsMode {
    if let Some(true) = secure {
        return TlsMode::Implicit;
    }
    match port {
        465 => TlsMode::Implicit,
        25 => TlsMode::None,
        _ => TlsMode::Starttls,
    }
}

async fn build_attachments(
    attachments: Option<HashMap<String, String>>,
) -> Result<Option<Vec<Attachment>>, String> {
    let Some(map) = attachments else {
        return Ok(None);
    };

    if map.is_empty() {
        return Ok(None);
    }

    let http_client = Client::new();
    let mut list = Vec::new();
    for (filename, url) in map {
        let content = download_url(&http_client, &url).await?;
        let content_type = mime_guess::from_path(&filename)
            .first_or_octet_stream()
            .to_string();
        list.push(Attachment {
            filename,
            content_type,
            content,
        });
    }

    Ok(Some(list))
}

async fn download_url(client: &Client, url: &str) -> Result<Vec<u8>, String> {
    let req = Request::get(url)
        .body(Body::empty())
        .map_err(|e| format!("Failed to build request: {e}"))?;

    let mut response = client
        .send(req)
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP request failed with status: {status}"));
    }

    let bytes = response
        .body_mut()
        .contents()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;
    Ok(bytes.to_vec())
}

fn error_response(status: StatusCode, message: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::from(message.to_string()))
        .unwrap_or_else(|_| Response::new(Body::from("Internal Server Error")))
}

#[cfg(test)]
mod tests {
    use super::{resolve_tls_mode, TlsMode};
    use crate::types::Input;

    #[test]
    fn tls_secure_true_always_implicit() {
        assert!(matches!(
            resolve_tls_mode(Some(true), 25),
            TlsMode::Implicit
        ));
        assert!(matches!(
            resolve_tls_mode(Some(true), 587),
            TlsMode::Implicit
        ));
        assert!(matches!(
            resolve_tls_mode(Some(true), 465),
            TlsMode::Implicit
        ));
    }

    #[test]
    fn tls_secure_false_falls_back_to_port() {
        assert!(matches!(
            resolve_tls_mode(Some(false), 465),
            TlsMode::Implicit
        ));
        assert!(matches!(resolve_tls_mode(Some(false), 25), TlsMode::None));
        assert!(matches!(
            resolve_tls_mode(Some(false), 587),
            TlsMode::Starttls
        ));
    }

    #[test]
    fn tls_secure_none_falls_back_to_port() {
        assert!(matches!(resolve_tls_mode(None, 465), TlsMode::Implicit));
        assert!(matches!(resolve_tls_mode(None, 25), TlsMode::None));
        assert!(matches!(resolve_tls_mode(None, 587), TlsMode::Starttls));
        assert!(matches!(resolve_tls_mode(None, 2525), TlsMode::Starttls));
    }

    #[test]
    fn port_as_integer() {
        let json =
            r#"{"host":"smtp.example.com","port":587,"senderEmail":"a@b.com","toEmail":"c@d.com"}"#;
        let input: Input = serde_json::from_str(json).unwrap();
        assert_eq!(input.port, 587);
    }

    #[test]
    fn port_as_string() {
        let json = r#"{"host":"smtp.example.com","port":"465","senderEmail":"a@b.com","toEmail":"c@d.com"}"#;
        let input: Input = serde_json::from_str(json).unwrap();
        assert_eq!(input.port, 465);
    }

    #[test]
    fn port_invalid_string_fails() {
        let json = r#"{"host":"smtp.example.com","port":"notaport","senderEmail":"a@b.com","toEmail":"c@d.com"}"#;
        assert!(serde_json::from_str::<Input>(json).is_err());
    }

    #[test]
    fn port_out_of_range_fails() {
        let json = r#"{"host":"smtp.example.com","port":99999,"senderEmail":"a@b.com","toEmail":"c@d.com"}"#;
        assert!(serde_json::from_str::<Input>(json).is_err());
    }

    #[test]
    fn input_minimal_fields() {
        let json = r#"{"host":"smtp.example.com","port":587,"senderEmail":"sender@example.com","toEmail":"to@example.com"}"#;
        let input: Input = serde_json::from_str(json).unwrap();
        assert_eq!(input.host, "smtp.example.com");
        assert_eq!(input.sender_email, "sender@example.com");
        assert_eq!(input.to_email, "to@example.com");
        assert!(input.username.is_none());
        assert!(input.cc.is_none());
        assert!(input.attachments.is_none());
    }

    #[test]
    fn input_camel_case_fields() {
        let json = r#"{"host":"h","port":25,"senderEmail":"s@s.com","senderName":"Sender","toEmail":"t@t.com","replyTo":"r@r.com"}"#;
        let input: Input = serde_json::from_str(json).unwrap();
        assert_eq!(input.sender_name.unwrap(), "Sender");
        assert_eq!(input.reply_to.unwrap(), "r@r.com");
    }

    #[test]
    fn input_unknown_fields_ignored() {
        let json = r#"{"host":"h","port":25,"senderEmail":"s@s.com","toEmail":"t@t.com","variables":{"key":"val"},"attachmentsCol":null}"#;
        assert!(serde_json::from_str::<Input>(json).is_ok());
    }

    #[test]
    fn mime_pdf() {
        let mime = mime_guess::from_path("report.pdf")
            .first_or_octet_stream()
            .to_string();
        assert_eq!(mime, "application/pdf");
    }

    #[test]
    fn mime_csv() {
        let mime = mime_guess::from_path("data.csv")
            .first_or_octet_stream()
            .to_string();
        assert_eq!(mime, "text/csv");
    }

    #[test]
    fn mime_png() {
        let mime = mime_guess::from_path("image.png")
            .first_or_octet_stream()
            .to_string();
        assert_eq!(mime, "image/png");
    }

    #[test]
    fn mime_unknown_falls_back_to_octet_stream() {
        let mime = mime_guess::from_path("file.unknownext")
            .first_or_octet_stream()
            .to_string();
        assert_eq!(mime, "application/octet-stream");
    }
}

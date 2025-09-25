use wasmcloud_component::http;

use serde::{self, Deserialize, Serialize};

pub mod bindings {
    wit_bindgen::generate!({ generate_all });
}

use crate::bindings::betty_blocks::smtp::client::{
    send, Attachment, Credentials, Message, Recipient, SendResult, Sender,
};

const MAX_READ: u64 = 2u64.pow(24); // 16mb

#[derive(Deserialize, Debug)]
#[serde(remote = "Sender")]
struct SenderDef {
    from: String,
    reply_to: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(remote = "Recipient")]
struct RecipientDef {
    to: Vec<String>,
    cc: Option<Vec<String>>,
    bcc: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
struct AttachmentDef {
    filename: String,
    path: String,
}

#[derive(Deserialize, Debug)]
#[serde(remote = "Message")]
struct MessageDef {
    #[serde(with = "SenderDef")]
    sender: Sender,
    #[serde(with = "RecipientDef")]
    recipient: Recipient,
    subject: String,
    body: String,
    #[serde(default, deserialize_with = "deserialize_attachment_vec")]
    attachment: Option<Vec<Attachment>>,
}

fn deserialize_attachment_vec<'de, D>(deserializer: D) -> Result<Option<Vec<Attachment>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Deserialize;

    let attachment_defs: Option<Vec<AttachmentDef>> = Option::deserialize(deserializer)?;

    Ok(attachment_defs.map(|defs| {
        defs.into_iter()
            .map(|def| Attachment {
                filename: def.filename,
                path: def.path,
            })
            .collect()
    }))
}

struct SmtpSendMailComponent;

#[derive(Deserialize, Debug)]
#[serde(remote = "Credentials")]
struct CredentialsDef {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    secure: Option<bool>,
    ignore_tls: Option<bool>,
    require_tls: Option<bool>,
}

#[derive(Deserialize, Debug)]
struct Input {
    #[serde(with = "CredentialsDef")]
    credentials: Credentials,
    application_id: String,
    #[serde(with = "MessageDef")]
    message: Message,
}

#[derive(Serialize, Debug)]
struct SendResultDef {
    accepted: bool,
    server: Option<String>,
    message_id: Option<String>,
}

impl From<SendResult> for SendResultDef {
    fn from(value: SendResult) -> Self {
        SendResultDef {
            accepted: value.accepted,
            server: value.server,
            message_id: value.message_id,
        }
    }
}

impl http::Server for SmtpSendMailComponent {
    fn handle(
        request: http::IncomingRequest,
    ) -> http::Result<http::Response<impl http::OutgoingBody>, http::ErrorCode> {
        let body = request.body();
        body.subscribe().block();
        let body_bytes = body.read(MAX_READ).map_err(|_| {
            http::ErrorCode::InternalError(Some("Failed to convert body to bytes".to_string()))
        })?;

        let send_input: Input = match serde_json::from_slice::<Input>(&body_bytes) {
            Ok(send_input) => send_input,
            Err(err) => {
                let msg = format!("Invalid body: {}", err);

                return Ok(http::Response::builder()
                    .status(412)
                    .body(msg)
                    .expect("Building response always succeeds"));
            }
        };

        let result = send(
            &send_input.credentials,
            &send_input.application_id,
            &send_input.message,
        )
        .unwrap();

        match serde_json::to_string(&SendResultDef::from(result)) {
            Ok(json) => Ok(http::Response::new(json)),
            Err(e) => {
                eprintln!("Error serializing result: {}", e);
                Err(http::ErrorCode::InternalError(Some(
                    "Invalid output".to_string(),
                )))
            }
        }
    }
}

http::export!(SmtpSendMailComponent);

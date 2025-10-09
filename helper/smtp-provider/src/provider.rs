use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context as _;
use lettre::message::header::ContentType;
use lettre::message::{MultiPart, SinglePart};
use lettre::{SmtpTransport, Transport};
use reqwest::header::HeaderValue;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tracing::info;
use wasmcloud_provider_sdk::initialize_observability;
use wasmcloud_provider_sdk::{
    run_provider, serve_provider_exports, Context, Provider, ProviderInitConfig,
};

pub(crate) mod bindings {
    wit_bindgen_wrpc::generate!();
}

use bindings::exports::betty_blocks::smtp::client::{
    Attachment, Credentials, Handler, Message, SendResult,
};

const PLAIN_TEXT_WIDTH: usize = 90;
const APPLICATION_OCTET_STREAM_HEADER: HeaderValue =
    reqwest::header::HeaderValue::from_static("application/octet-stream");

#[derive(Default, Clone)]
pub struct SmtpProvider {}

impl SmtpProvider {
    fn name() -> &'static str {
        "smtp-provider"
    }

    pub async fn run() -> anyhow::Result<()> {
        initialize_observability!(
            Self::name(),
            std::env::var_os("SMTP_PROVIDER_FLAMEGRAPH_PATH")
        );
        let provider = Self::default();
        let shutdown = run_provider(provider.clone(), SmtpProvider::name())
            .await
            .context("failed to run provider")?;

        let connection = wasmcloud_provider_sdk::get_connection();
        serve_provider_exports(
            &connection
                .get_wrpc_client(connection.provider_key())
                .await
                .context("failed to get wrpc client")?,
            provider,
            shutdown,
            bindings::serve,
        )
        .await
    }
}

impl Handler<Option<Context>> for SmtpProvider {
    async fn send(
        &self,
        _ctx: Option<Context>,
        credentials: Credentials,
        application_id: String,
        message: Message,
    ) -> anyhow::Result<Result<SendResult, String>> {
        Ok(inner_send(credentials, application_id, message)
            .await
            .map_err(|e| e.to_string()))
    }
}

async fn download_attachments(attachments: Vec<Attachment>) -> anyhow::Result<Vec<SinglePart>> {
    let mut set = JoinSet::new();

    for attachment in attachments {
        set.spawn(async {
            let request = reqwest::get(attachment.path).await;
            (attachment.filename, request)
        });
    }

    let requests = set.join_all().await;

    let mut attachments: Vec<SinglePart> = vec![];
    for (filename, req) in requests {
        match req {
            Ok(response) => {
                // NOTE:
                // First extract the headers, as response.bytes() consumes the response
                let headers = response.headers();
                let content_type = headers
                    .get(reqwest::header::CONTENT_TYPE)
                    .cloned()
                    .unwrap_or(APPLICATION_OCTET_STREAM_HEADER);
                let content_header = content_type.to_str()?;

                let filebody: Vec<u8> = response.bytes().await?.to_vec();
                let content_type = ContentType::parse(content_header)?;
                let attachment = lettre::message::Attachment::new(filename.to_string())
                    .body(filebody, content_type);
                attachments.push(attachment);
            }
            Err(_) => {
                return Err(anyhow::anyhow!("Downloading attachments failed"));
            }
        }
    }

    Ok(attachments)
}

async fn inner_send(
    credentials: Credentials,
    _application_id: String,
    message: Message,
) -> anyhow::Result<SendResult> {
    let mut email = lettre::Message::builder()
        .from(message.sender.from.parse()?)
        .subject(message.subject);

    if let Some(reply_to) = message.sender.reply_to {
        email = email.reply_to(reply_to.parse()?);
    }

    for recipient in message.recipient.to {
        email = email.to(recipient.parse()?);
    }

    let downloads = if let Some(attachments) = message.attachment {
        download_attachments(attachments).await?
    } else {
        vec![]
    };

    let body_bytes = message.body.as_bytes();
    let plain_text = html2text::from_read(body_bytes, PLAIN_TEXT_WIDTH)?;

    let mut mixed =
        MultiPart::mixed().multipart(MultiPart::alternative_plain_html(plain_text, message.body));

    for attachment in downloads {
        mixed = mixed.singlepart(attachment);
    }

    let email = email.multipart(mixed)?;

    let mut builder: lettre::transport::smtp::SmtpTransportBuilder = match credentials.secure {
        Some(false) => SmtpTransport::builder_dangerous(&credentials.host),
        _ => SmtpTransport::starttls_relay(&credentials.host)?,
    }
    .port(credentials.port);

    if let (Some(username), Some(password)) = (credentials.username, credentials.password) {
        builder = builder.credentials(lettre::transport::smtp::authentication::Credentials::new(
            username, password,
        ))
    }

    let sender = builder.build();

    sender.send(&email)?;

    Ok(SendResult {
        accepted: true,
        server: None,
        message_id: None,
    })
}

impl Provider for SmtpProvider {
    async fn init(&self, _config: impl ProviderInitConfig) -> anyhow::Result<()> {
        Ok(())
    }
}

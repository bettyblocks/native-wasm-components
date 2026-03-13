use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct Input {
    pub host: String,
    #[serde(deserialize_with = "deserialize_port")]
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub secure: Option<bool>,
    #[serde(rename = "senderEmail")]
    pub sender_email: String,
    #[serde(rename = "senderName")]
    pub sender_name: Option<String>,
    #[serde(rename = "toEmail")]
    pub to_email: String,
    pub cc: Option<String>,
    pub bcc: Option<String>,
    #[serde(rename = "replyTo")]
    pub reply_to: Option<String>,
    pub subject: Option<String>,
    pub body: Option<String>,
    pub attachments: Option<HashMap<String, String>>,
}

#[derive(Serialize)]
pub struct SendResult {
    pub accepted: bool,
    pub server: Option<String>,
    pub message_id: Option<String>,
}

fn deserialize_port<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    use serde::de::Error;
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => n
            .as_u64()
            .and_then(|v| u16::try_from(v).ok())
            .ok_or_else(|| D::Error::custom("invalid port number")),
        serde_json::Value::String(s) => s
            .parse::<u16>()
            .map_err(|_| D::Error::custom("invalid port string")),
        _ => Err(D::Error::custom("port must be a number or string")),
    }
}

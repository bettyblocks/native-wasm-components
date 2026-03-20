use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize)]
pub struct SendMailOutput {
    pub result: SendResult,
}

#[derive(Serialize)]
pub struct SendResult {
    pub accepted: bool,
    pub server: Option<String>,
    pub message_id: Option<String>,
}

impl From<crate::betty_blocks::smtp::client::SendResult> for SendResult {
    fn from(r: crate::betty_blocks::smtp::client::SendResult) -> Self {
        Self {
            accepted: r.accepted,
            server: r.server,
            message_id: r.message_id,
        }
    }
}

#[derive(Deserialize)]
pub struct CollectionData {
    pub data: Vec<HashMap<String, FileInfo>>,
}

#[derive(Deserialize)]
pub struct PropertySpec {
    pub name: String,
}

#[derive(Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub url: Option<String>,
}

#[derive(Deserialize)]
pub struct UrlField {
    pub url: String,
}

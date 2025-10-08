use std::net::IpAddr;
use std::time::Duration;

use futures::StreamExt;
use serde_json::json;
use serial_test::serial;
use testcontainers::core::logs::LogFrame;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers::{CopyDataSource, GenericImage, core::IntoContainerPort, core::WaitFor};
use tokio::process::Command;
use tokio::sync::OnceCell;
use tokio::time::sleep;
use wadm_client::{ClientConnectOptions, ManifestLoader};

const NATS_PORT: u16 = 4222;
const MAILCATCHER_SMTP_PORT: u16 = 25;
const MAILCATCHER_API_PORT: u16 = 80;
const SMTP_COMPONENT_PORT: u16 = 8000;

const WASMCLOUD_VERSION: &str = "1.8.0";
const WADM_VERSION: &str = "v0.15.0-wolfi";
const NATS_VERSION: &str = "2.10.19-alpine";

type ContainerDef = ContainerAsync<GenericImage>;

static ONCES: OnceCell<(ContainerDef, ContainerDef, ContainerDef, ContainerDef)> =
    OnceCell::const_new();

// NOTE: Keeping it here for debugging purposes
fn show_logs(logframe: &LogFrame) {
    match logframe {
        LogFrame::StdOut(bytes) => println!("{}", String::from_utf8_lossy(bytes)),
        LogFrame::StdErr(bytes) => println!("{}", String::from_utf8_lossy(bytes)),
    }
}

async fn build_wasm() {
    let output = Command::new("wash")
        .arg("build")
        .current_dir("../helper/smtp-provider")
        .output()
        .await
        .unwrap();

    if !output.status.success() {
        println!(
            "Failed building provider, Stdout: {}, stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        panic!()
    }

    let output = Command::new("wash")
        .args(vec!["wit", "deps"])
        .current_dir("../components/smtp")
        .output()
        .await
        .unwrap();

    if !output.status.success() {
        println!(
            "Failed fetching deps, Stdout: {}, stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        panic!()
    }

    let output = Command::new("wash")
        .arg("build")
        .current_dir("../components/smtp")
        .output()
        .await
        .unwrap();
    if !output.status.success() {
        println!(
            "Failed building component, Stdout: {}, stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        panic!()
    }
}

async fn start_nats() -> ContainerDef {
    GenericImage::new("nats", NATS_VERSION)
        .with_exposed_port(NATS_PORT.tcp())
        .with_wait_for(WaitFor::message_on_either_std(
            "Listening for client connections",
        ))
        .with_cmd(vec!["-js"])
        .with_container_name("nats_integration_test")
        .with_network("bridge")
        .start()
        .await
        .expect("Failed starting NATS")
}

async fn start_wasmcloud(bridge_ip: &IpAddr) -> ContainerDef {
    GenericImage::new("wasmcloud/wasmcloud", WASMCLOUD_VERSION)
        .with_wait_for(WaitFor::message_on_either_std("wasmCloud host started"))
        .with_exposed_port(SMTP_COMPONENT_PORT.tcp())
        .with_env_var("WASMCLOUD_ALLOW_FILE_LOAD", "true")
        .with_env_var("WASMCLOUD_RPC_HOST", bridge_ip.to_string())
        .with_env_var("WASMCLOUD_RPC_PORT", NATS_PORT.to_string())
        .with_env_var("WASMCLOUD_CTL_HOST", bridge_ip.to_string())
        .with_env_var("WASMCLOUD_CTL_PORT", NATS_PORT.to_string())
        .with_env_var("SMTP_USE_LOCAL_EHLO", "1")
        .with_network("bridge")
        .with_copy_to(
            "/tmp/component.wasm",
            CopyDataSource::File("../components/smtp/build/send_mail_component.wasm".into()),
        )
        .with_copy_to(
            "/tmp/provider.par.gz",
            CopyDataSource::File("../helper/smtp-provider/build/smtp-provider.par.gz".into()),
        )
        .with_container_name("wasmcloud_integration_test")
        // .with_log_consumer(show_logs)
        .start()
        .await
        .expect("Failed starting wasmcloud")
}

async fn start_wadm(bridge_ip: &IpAddr) -> ContainerDef {
    GenericImage::new("ghcr.io/wasmcloud/wadm", WADM_VERSION)
        .with_wait_for(WaitFor::message_on_either_std("connected"))
        .with_env_var("WADM_NATS_SERVER", bridge_ip.to_string())
        .with_network("bridge")
        .with_container_name("wadm_integration_test")
        // .with_log_consumer(show_logs)
        .start()
        .await
        .expect("Failed starting wadm")
}

async fn start_mail_server() -> ContainerDef {
    // NOTE: Using this mail server as it implements authentication
    GenericImage::new("rnwood/smtp4dev", "latest")
        .with_exposed_port(MAILCATCHER_SMTP_PORT.tcp())
        .with_exposed_port(MAILCATCHER_API_PORT.tcp())
        .with_wait_for(WaitFor::message_on_either_std("Now listening on"))
        //.with_log_consumer(show_logs)
        .with_network("bridge")
        .with_container_name("mail_integration_test")
        .start()
        .await
        .expect("Failed to start catcher")
}

async fn start_everything() -> (ContainerDef, ContainerDef, ContainerDef, ContainerDef) {
    let nats = start_nats().await;

    let bridge_ip = nats.get_bridge_ip_address().await.unwrap();
    let wasmcloud = start_wasmcloud(&bridge_ip).await;
    let wadm = start_wadm(&bridge_ip).await;
    let catcher = start_mail_server().await;

    (nats, wasmcloud, wadm, catcher)
}

async fn publish_components(bridge_ip: &IpAddr) {
    let client_connect = ClientConnectOptions {
        url: Some(format!("{}:{}", bridge_ip, NATS_PORT)),
        ..Default::default()
    };

    let client = wadm_client::Client::new("default", None, client_connect)
        .await
        .expect("Failed client");

    let manifest = "wadm.test.yaml"
        .load_manifest()
        .await
        .expect("Failed to load manifest");

    if let Ok(_existing_manifest) = client.get_manifest(&manifest.metadata.name, None).await {
        return;
    }

    client
        .put_and_deploy_manifest(&manifest)
        .await
        .expect("Failed to put and deploy manifest");
    let mut stream = client
        .subscribe_to_status(&manifest.metadata.name)
        .await
        .expect("Failed to subscribe");
    loop {
        let message = stream.next().await.expect("Failed to read next message");
        let payload: serde_json::Value =
            serde_json::from_slice(&message.payload).expect("Failed to parse payload");
        if payload["status"]["type"] == "deployed" {
            // NOTE: Wait for x-seconds to wait so that the http-interface is online
            sleep(Duration::new(2, 0)).await;
            break;
        }
    }
}

async fn get_message_id(api_port: u16) -> String {
    let messages_response = reqwest::get(format!("http://127.0.0.1:{}/api/messages", api_port))
        .await
        .expect("Failed to get");

    let json: serde_json::Value = messages_response.json().await.unwrap();
    let message_id = json["results"][0]["id"].as_str().unwrap();

    message_id.to_string()
}

async fn get_mail_text(api_port: u16) -> String {
    let id = get_message_id(api_port).await;
    let message_response = reqwest::get(format!(
        "http://127.0.0.1:{}/api/messages/{}/plaintext",
        api_port, id,
    ))
    .await
    .expect("Failed to get message");

    message_response.text().await.unwrap()
}

// Example body:
// {
//           "sessionEncoding": "iso-8859-1",
//           "parts": [
//             {
//               "id": "0",
//               "headers": [
//                 {"name":"Content-Type","value":"multipart/mixed; boundary=\"X\""}
//               ],
//               "childParts": [
//                 {
//                   "id": "1",
//                   "headers": [
//                     {"name":"Content-Type","value":"multipart/alternative; boundary=\"Y\""}
//                   ],
//                   "childParts": [
//                     {
//                       "id":"2",
//                       "headers":[
//                         {"name":"Content-Type","value":"text/plain; charset=utf-8"},
//                         {"name":"Content-Transfer-Encoding","value":"7bit"}
//                       ],
//                       "childParts":[],
//                       "attachments":[],
//                       "isAttachment":false
//                     },
//                     {
//                       "id":"3",
//                       "headers":[
//                         {"name":"Content-Type","value":"text/html; charset=utf-8"},
//                         {"name":"Content-Transfer-Encoding","value":"7bit"}
//                       ],
//                       "childParts":[],
//                       "attachments":[],
//                       "isAttachment":false
//                     }
//                   ],
//                   "attachments":[],
//                   "isAttachment":false
//                 },
//                 {
//                   "id":"4",
//                   "headers":[
//                     {"name":"Content-Disposition","value":"attachment; filename=\"betty logo\""},
//                     {"name":"Content-Type","value":"image/svg+xml"},
//                     {"name":"Content-Transfer-Encoding","value":"quoted-printable"}
//                   ],
//                   "childParts":[],
//                   "attachments":[],
//                   "isAttachment":true
//                 }
//               ],
//               "attachments":[
//                 {"fileName":"betty logo","contentId":null,"id":"4","url":"api/messages/.../4/content"}
//               ],
//               "isAttachment":false
//             }
//           ]
//         }
async fn has_attachment_named(api_port: u16, name: &str) -> anyhow::Result<bool> {
    let id = get_message_id(api_port).await;
    let message_response =
        reqwest::get(format!("http://127.0.0.1:{}/api/messages/{}", api_port, id,))
            .await
            .expect("Failed to get message");

    let v: serde_json::Value = message_response
        .json()
        .await
        .expect("failed to get response json");

    fn part_has_named_attachment(part: &serde_json::Value, name: &str) -> bool {
        if let Some(attachments) = part.get("attachments").and_then(|a| a.as_array()) {
            for att in attachments {
                if att
                    .get("fileName")
                    .and_then(|f| f.as_str())
                    .map(|f| f.eq_ignore_ascii_case(name))
                    == Some(true)
                {
                    return true;
                }
            }
        }

        false
    }

    if let Some(parts) = v.get("parts").and_then(|p| p.as_array()) {
        Ok(parts
            .iter()
            .any(|part| part_has_named_attachment(part, name)))
    } else {
        Ok(false)
    }
}

#[tokio::test]
#[serial]
async fn smtp_should_send_an_email() {
    build_wasm().await;

    let (nats, wasmcloud, _wadm, catcher) = ONCES.get_or_init(start_everything).await;
    let bridge_ip = nats.get_bridge_ip_address().await.unwrap();
    publish_components(&bridge_ip).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to start reqwest client");

    let payload = json!({
      "credentials": {
        "host": format!("{}", catcher.get_bridge_ip_address().await.expect("Failed to get catcher host")),
        "port": MAILCATCHER_SMTP_PORT,
         "username": "test",
         "password": "test",
        "secure": false,
        "ignore_tls": false,
        "require_tls": false
      },
      "application_id": "my-app-123",
      "message": {
        "sender": {
          "from": "sender@example.com",
          "reply_to": "noreply@example.com"
        },
        "recipient": {
          "to": ["recipient1@example.com", "recipient2@example.com"],
          "cc": ["cc1@example.com", "cc2@example.com"],
          "bcc": ["bcc1@example.com"]
        },
        "subject": "Test Email Subject",
        "body": "This is the email body content.",
      }
    });

    let wasmcloud_port = wasmcloud
        .get_host_port_ipv4(SMTP_COMPONENT_PORT)
        .await
        .expect("Failed to get wasmcloud port");

    let resp = client
        .post(format!("http://127.0.0.1:{}", wasmcloud_port))
        .json(&payload)
        .send()
        .await
        .expect("Failed to post email");

    assert_eq!(resp.status(), 200);

    let mailcatcher_api_port = catcher
        .get_host_port_ipv4(MAILCATCHER_API_PORT)
        .await
        .expect("Failed to get mailcatcher API port");
    let mail_text = get_mail_text(mailcatcher_api_port).await;

    assert_eq!(mail_text, "This is the email body content.\n");
}

#[tokio::test]
#[serial]
async fn smtp_should_download_and_send_attachments() {
    build_wasm().await;

    let (nats, wasmcloud, _wadm, catcher) = ONCES.get_or_init(start_everything).await;
    let bridge_ip = nats.get_bridge_ip_address().await.unwrap();
    publish_components(&bridge_ip).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to start reqwest client");

    let payload = json!({
      "credentials": {
        "host": format!("{}", catcher.get_bridge_ip_address().await.expect("Failed to get catcher host")),
        "port": MAILCATCHER_SMTP_PORT,
         "username": "test",
         "password": "test",
        "secure": false,
        "ignore_tls": false,
        "require_tls": false
      },
      "application_id": "my-app-123",
      "message": {
        "sender": {
          "from": "sender@example.com",
          "reply_to": "noreply@example.com"
        },
        "recipient": {
          "to": ["recipient1@example.com", "recipient2@example.com"],
          "cc": ["cc1@example.com", "cc2@example.com"],
          "bcc": ["bcc1@example.com"]
        },
        "subject": "Test Email Subject",
        "body": "This is the email body content.",
        "attachment": [
                {
                    "filename": "betty logo",
                    "path": "https://www.bettyblocks.com/hubfs/logo-red.svg"
                }
            ]
      }
    });

    let wasmcloud_port = wasmcloud
        .get_host_port_ipv4(SMTP_COMPONENT_PORT)
        .await
        .expect("Failed to get wasmcloud port");

    let resp = client
        .post(format!("http://127.0.0.1:{}", wasmcloud_port))
        .json(&payload)
        .send()
        .await
        .expect("Failed to post email");

    assert_eq!(resp.status(), 200);

    let mailcatcher_api_port = catcher
        .get_host_port_ipv4(MAILCATCHER_API_PORT)
        .await
        .expect("Failed to get mailcatcher API port");

    assert!(
        has_attachment_named(mailcatcher_api_port, "betty logo")
            .await
            .unwrap()
    );
}

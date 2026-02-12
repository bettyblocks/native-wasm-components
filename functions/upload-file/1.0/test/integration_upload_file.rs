#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::{Context, Result};
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
use tokio::time::timeout;

mod common;
use common::find_available_port;

use wash_runtime::{
    engine::Engine,
    host::{
        http::{DevRouter, HttpServer},
        HostApi, HostBuilder,
    },
    plugin::{
        wasi_blobstore::WasiBlobstore, wasi_config::WasiConfig, wasi_keyvalue::WasiKeyvalue,
        wasi_logging::WasiLogging,
    },
    types::{
        Component, EmptyDirVolume, LocalResources, Volume, VolumeMount, VolumeType, Workload,
        WorkloadStartRequest,
    },
    wit::WitInterface,
};

const UPLOAD_FILE_WASM: &[u8] = include_bytes!("fixtures/upload_file.wasm");
const PRESIGN_POST_GENERATOR_WASM: &[u8] = include_bytes!("fixtures/presign_post_generator.wasm");
const HTTP_ROUTER_WASM: &[u8] = include_bytes!("fixtures/http_router.wasm");

fn store_file_host_interfaces(http_host_config: &str) -> Vec<WitInterface> {
    vec![
        WitInterface {
            namespace: "wasi".to_string(),
            package: "http".to_string(),
            interfaces: ["incoming-handler".to_string()].into_iter().collect(),
            version: Some(semver::Version::parse("0.2.2").unwrap()),
            config: {
                let mut config = HashMap::new();
                config.insert("host".to_string(), http_host_config.to_string());
                config
            },
        },
        WitInterface {
            namespace: "wasi".to_string(),
            package: "http".to_string(),
            interfaces: ["outgoing-handler".to_string()].into_iter().collect(),
            version: Some(semver::Version::parse("0.2.2").unwrap()),
            config: HashMap::new(),
        },
        WitInterface {
            namespace: "wasi".to_string(),
            package: "filesystem".to_string(),
            interfaces: ["types".to_string(), "preopens".to_string()]
                .into_iter()
                .collect(),
            version: Some(semver::Version::parse("0.2.2").unwrap()),
            config: HashMap::new(),
        },
        WitInterface {
            namespace: "wasi".to_string(),
            package: "logging".to_string(),
            interfaces: ["logging".to_string()].into_iter().collect(),
            version: Some(semver::Version::parse("0.1.0-draft").unwrap()),
            config: HashMap::new(),
        },
    ]
}

async fn start_host_with_plugins(addr: SocketAddr) -> Result<impl HostApi> {
    let engine = Engine::builder().build()?;
    let host = HostBuilder::new()
        .with_engine(engine)
        .with_http_handler(Arc::new(HttpServer::new(DevRouter::default(), addr)))
        .with_plugin(Arc::new(WasiBlobstore::new(None)))?
        .with_plugin(Arc::new(WasiKeyvalue::new()))?
        .with_plugin(Arc::new(WasiLogging {}))?
        .with_plugin(Arc::new(WasiConfig::default()))?
        .build()?;

    host.start().await.context("Failed to start host")
}

fn file_upload_workload_request(http_host_config: &str) -> WorkloadStartRequest {
    let tmp_volume = Volume {
        name: "tmp-uploads".to_string(),
        volume_type: VolumeType::EmptyDir(EmptyDirVolume {}),
    };

    let volume_mount = VolumeMount {
        name: "tmp-uploads".to_string(),
        mount_path: "/".to_string(),
        read_only: false,
    };

    WorkloadStartRequest {
        workload_id: uuid::Uuid::new_v4().to_string(),
        workload: Workload {
            namespace: "test".to_string(),
            name: "file-upload-workload".to_string(),
            annotations: HashMap::new(),
            service: None,
            components: vec![
                Component {
                    bytes: bytes::Bytes::from_static(PRESIGN_POST_GENERATOR_WASM),
                    local_resources: LocalResources {
                        memory_limit_mb: 256,
                        cpu_limit: 1,
                        config: HashMap::from([
                            ("region".to_string(), "eu-central-1".to_string()),
                            ("bucket".to_string(), "wasmtesting".to_string()),
                        ]),
                        environment: HashMap::new(),
                        volume_mounts: vec![],
                        allowed_hosts: vec![],
                    },
                    pool_size: 1,
                    max_invocations: 100,
                },
                Component {
                    bytes: bytes::Bytes::from_static(UPLOAD_FILE_WASM),
                    local_resources: LocalResources {
                        memory_limit_mb: 512,
                        cpu_limit: 1,
                        config: HashMap::from([("test_mode".to_string(), "true".to_string())]),
                        environment: HashMap::new(),
                        volume_mounts: vec![volume_mount],
                        allowed_hosts: vec![
                            "www.w3.org".to_string(),
                            "*.wasabisys.com".to_string(),
                            "s3.eu-central-1.wasabisys.com".to_string(),
                        ],
                    },
                    pool_size: 1,
                    max_invocations: 100,
                },
                Component {
                    bytes: bytes::Bytes::from_static(HTTP_ROUTER_WASM),
                    local_resources: LocalResources {
                        memory_limit_mb: 256,
                        cpu_limit: 1,
                        config: HashMap::new(),
                        environment: HashMap::from([
                            (
                                "STORAGE_ACCESS_KEY".to_string(),
                                "get_this_from_the_storage_bucket".to_string(),
                            ),
                            (
                                "STORAGE_SECRET_KEY".to_string(),
                                "get_this_from_the_storage_service".to_string(),
                            ),
                        ]),
                        volume_mounts: vec![],
                        allowed_hosts: vec![],
                    },
                    pool_size: 1,
                    max_invocations: 100,
                },
            ],
            host_interfaces: store_file_host_interfaces(http_host_config),
            volumes: vec![tmp_volume],
        },
    }
}

#[tokio::test]
async fn test_file_upload_integration() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let port = find_available_port().await?;
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let host = start_host_with_plugins(addr).await?;

    let req = file_upload_workload_request("file-upload");

    host.workload_start(req)
        .await
        .context("Failed to start file upload workload")?;

    let client = reqwest::Client::new();

    let test_payload = serde_json::json!({
        "applicationId": "test-app",
        "actionId": "test-action",
        "logId": "test-log",
        "modelName": "TestModel",
        "propertyName": "testFile",
        "url": "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf",
        "filename": "dummy.pdf",
        "content-type": "application/pdf"
    });

    let response = timeout(
        Duration::from_secs(30),
        client
            .post(format!("http://{addr}/"))
            .header("HOST", "file-upload")
            .header("Content-Type", "application/json")
            .json(&test_payload)
            .send(),
    )
    .await
    .context("Request timed out")?
    .context("Failed to make request")?;

    let status = response.status();
    let response_text = response.text().await?;

    eprintln!("Response status: {}", status);
    eprintln!("Response body: {}", response_text);

    assert!(
        status.is_success(),
        "File upload request should succeed. Status: {}, Body: {}",
        status,
        response_text
    );

    assert!(
        response_text.contains("uploaded") || response_text.contains("Reference"),
        "Response should indicate successful upload"
    );

    Ok(())
}

#[tokio::test]
async fn test_file_upload_error_handling() -> Result<()> {
    let port = find_available_port().await?;
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let host = start_host_with_plugins(addr).await?;

    let req = file_upload_workload_request("error-test");

    host.workload_start(req)
        .await
        .context("Failed to start error test workload")?;

    let client = reqwest::Client::new();

    let invalid_payload = serde_json::json!({
        "applicationId": "test-app",
        "actionId": "test-action",
        "logId": "test-log",
        "modelName": "TestModel",
        "propertyName": "testFile",
        "url": "https://invalid-url-that-does-not-exist-12345.com/file.pdf",
        "filename": "test.pdf",
        "content-type": "application/pdf"
    });

    let response = timeout(
        Duration::from_secs(30),
        client
            .post(format!("http://{addr}/"))
            .header("HOST", "error-test")
            .header("Content-Type", "application/json")
            .json(&invalid_payload)
            .send(),
    )
    .await
    .context("Request timed out")?
    .context("Failed to make request")?;

    let status = response.status();

    assert!(
        status.is_client_error() || status.is_server_error(),
        "Invalid URL should result in error status"
    );

    Ok(())
}

#[tokio::test]
async fn test_file_upload_malformed_request() -> Result<()> {
    let port = find_available_port().await?;
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let host = start_host_with_plugins(addr).await?;

    let req = file_upload_workload_request("malformed-test");

    host.workload_start(req)
        .await
        .context("Failed to start malformed test workload")?;

    let client = reqwest::Client::new();

    let malformed_json = r#"{"incomplete": "json""#;

    let response = client
        .post(format!("http://{addr}/"))
        .header("HOST", "malformed-test")
        .header("Content-Type", "application/json")
        .body(malformed_json)
        .send()
        .await;

    if let Ok(response) = response {
        let status = response.status();
        assert!(
            status.is_client_error() || status.is_server_error(),
            "Malformed JSON should result in error status"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_file_upload_concurrent_requests() -> Result<()> {
    let port = find_available_port().await?;
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let host = start_host_with_plugins(addr).await?;

    let req = file_upload_workload_request("concurrent-test");

    host.workload_start(req)
        .await
        .context("Failed to start concurrent test workload")?;

    let client = reqwest::Client::new();

    let mut handles = Vec::new();
    for i in 0..3 {
        let client = client.clone();
        let payload = serde_json::json!({
            "applicationId": "test-app",
            "actionId": format!("test-action-{}", i),
            "logId": format!("test-log-{}", i),
            "modelName": "TestModel",
            "propertyName": "testFile",
            "url": "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf",
            "filename": format!("dummy-{}.pdf", i),
            "content-type": "application/pdf"
        });

        handles.push(tokio::spawn(async move {
            timeout(
                Duration::from_secs(45),
                client
                    .post(format!("http://{addr}/"))
                    .header("HOST", "concurrent-test")
                    .header("Content-Type", "application/json")
                    .json(&payload)
                    .send(),
            )
            .await
            .ok()
            .and_then(|r| r.ok())
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        }));
    }

    let mut successful = 0;
    for handle in handles {
        if handle.await.unwrap_or(false) {
            successful += 1;
        }
    }

    assert!(
        successful == 3,
        "All 3 concurrent requests should succeed, only {successful} succeeded"
    );

    Ok(())
}

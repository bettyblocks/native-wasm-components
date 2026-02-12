use hmac::{Hmac, Mac};
use sha2::Sha256;

pub mod bindings {
    wit_bindgen::generate!({
        generate_all,
    });
}

use bindings::exports::betty_blocks::data_api::data_api_utilities::{
    Guest as FetchGuest, Model, PolicyField, PresignedPost, Property,
};

struct Component;

type HmacSha256 = Hmac<Sha256>;

impl FetchGuest for Component {
    fn fetch_presigned_post(
        _model: Model,
        _property: Property,
        content_type: String,
        filename: String,
    ) -> Result<PresignedPost, String> {
        // TODO: Get these from configuration or environment
        // read credentials from environment and return an error if missing
        let access_key = std::env::var("STORAGE_ACCESS_KEY").map_err(|e| e.to_string())?;
        let secret_key = std::env::var("STORAGE_SECRET_KEY").map_err(|e| e.to_string())?;
        let region = "eu-central-1";
        let bucket = "wasmtesting";
        let expires_in = 3600; // 1 hour

        
        generate_presigned_post(
            &access_key,
            &secret_key,
            region,
            bucket,
            &filename,
            &content_type,
            expires_in,
        )
    }
}

fn generate_presigned_post(
    access_key: &str,
    secret_key: &str,
    region: &str,
    bucket: &str,
    filename: &str,
    content_type: &str,
    expires_in: u32,
) -> Result<PresignedPost, String> {
    let endpoint = format!("https://s3.{}.wasabisys.com", region);
    let url = format!("{}/{}", endpoint, bucket);

    // Get current time
    let now = time::OffsetDateTime::now_utc();

    // Format date as YYYYMMDD
    let date = format!("{:04}{:02}{:02}", now.year(), now.month() as u8, now.day());

    // Format datetime as YYYYMMDDTHHMMSSZ
    let datetime = format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    );

    // Credential scope
    let credential = format!("{}/{}/s3/aws4_request", date, region);
    let x_amz_credential = format!("{}/{}", access_key, credential);

    // Calculate expiration time
    let expiration_time = now + time::Duration::seconds(expires_in as i64);
    let expiration = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000Z",
        expiration_time.year(),
        expiration_time.month() as u8,
        expiration_time.day(),
        expiration_time.hour(),
        expiration_time.minute(),
        expiration_time.second()
    );

    // Policy document
    let policy = serde_json::json!({
        "expiration": expiration,
        "conditions": [
            {"bucket": bucket},
            {"key": filename},
            ["eq", "$Content-Type", content_type],
            {"x-amz-algorithm": "AWS4-HMAC-SHA256"},
            {"x-amz-credential": x_amz_credential},
            {"x-amz-date": datetime},
            ["content-length-range", 0, 500000000]
        ]
    });

    let policy_str = serde_json::to_string(&policy).map_err(|e| e.to_string())?;
    let policy_b64 = base64::encode(policy_str.as_bytes());

    // Calculate signature
    let signature = sign_policy(&policy_b64, secret_key, &date, region)?;

    // Build form fields - convert to WIT types
    let mut fields = Vec::new();
    fields.push(PolicyField {
        key: "key".to_string(),
        value: filename.to_string(),
    });
    fields.push(PolicyField {
        key: "Content-Type".to_string(),
        value: content_type.to_string(),
    });
    fields.push(PolicyField {
        key: "x-amz-algorithm".to_string(),
        value: "AWS4-HMAC-SHA256".to_string(),
    });
    fields.push(PolicyField {
        key: "x-amz-credential".to_string(),
        value: x_amz_credential,
    });
    fields.push(PolicyField {
        key: "x-amz-date".to_string(),
        value: datetime,
    });
    fields.push(PolicyField {
        key: "policy".to_string(),
        value: policy_b64,
    });
    fields.push(PolicyField {
        key: "x-amz-signature".to_string(),
        value: signature,
    });

    Ok(PresignedPost {
        url,
        fields,
        reference: filename.to_string(),
    })
}

fn sign_policy(
    policy_b64: &str,
    secret_key: &str,
    date: &str,
    region: &str,
) -> Result<String, String> {
    let k_secret = format!("AWS4{}", secret_key);
    let k_date = hmac_sha256(k_secret.as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    let k_signing = hmac_sha256(&k_service, b"aws4_request");

    let signature = hmac_sha256(&k_signing, policy_b64.as_bytes());
    Ok(hex::encode(signature))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

bindings::export!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_presigned_post() {
        let result = generate_presigned_post(
            "your-access-key",
            "your-secret-key",
            "eu-central-1",
            "wasmtesting",
            "test-file.pdf",
            "application/pdf",
            1000,
        );

        assert!(result.is_ok());
        let post = result.unwrap();
        assert!(post.url.contains("wasabisys.com"));
        assert!(post.fields.len() > 0);
        assert_eq!(post.reference, "test-file.pdf");
    }
}

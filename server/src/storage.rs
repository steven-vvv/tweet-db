use std::path::Path;

use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::{Client as S3Client, config::Region, primitives::ByteStream};
use bytes::Bytes;
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use crate::{
    config::{Settings, StorageSection},
    error::{AppError, AppResult},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageSubsystemStatus {
    pub active: bool,
    pub provider: String,
    pub bucket: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObjectMetadata {
    pub id: Uuid,
    pub provider: String,
    pub bucket: String,
    pub object_key: String,
    pub content_type: String,
    pub content_length: i64,
    pub etag: Option<String>,
    pub sha256_hex: String,
}

pub fn status(section: &StorageSection) -> StorageSubsystemStatus {
    StorageSubsystemStatus {
        active: !section.provider.trim().is_empty() && !section.bucket.trim().is_empty(),
        provider: section.provider.clone(),
        bucket: section.bucket.clone(),
    }
}

pub fn build_client(settings: &Settings) -> AppResult<S3Client> {
    let storage = &settings.config.storage;
    let mut builder = aws_sdk_s3::config::Builder::new()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new(storage.region.clone()))
        .endpoint_url(storage.endpoint.clone())
        .force_path_style(storage.path_style);

    match (
        settings.secrets.storage_access_key.as_ref(),
        settings.secrets.storage_secret_key.as_ref(),
    ) {
        (Some(access_key), Some(secret_key)) => {
            builder = builder.credentials_provider(Credentials::new(
                access_key.clone(),
                secret_key.clone(),
                None,
                None,
                "tweet-db",
            ));
        }
        (None, None) => {}
        _ => {
            return Err(AppError::config(
                "STORAGE_ACCESS_KEY and STORAGE_SECRET_KEY must be configured together",
            ));
        }
    }

    Ok(S3Client::from_conf(builder.build()))
}

pub fn build_object_key(prefix: &str, media_id: i64, task_id: Uuid, extension: &str) -> String {
    let trimmed_prefix = prefix.trim_matches('/');
    if trimmed_prefix.is_empty() {
        format!("{media_id}/{task_id}.{extension}")
    } else {
        format!("{trimmed_prefix}/{media_id}/{task_id}.{extension}")
    }
}

pub fn resolve_upload_content_type(
    explicit_content_type: Option<&str>,
    source_url: &str,
) -> String {
    normalize_content_type(explicit_content_type)
        .or_else(|| infer_content_type_from_url(source_url))
        .unwrap_or("application/octet-stream")
        .to_owned()
}

pub fn resolve_upload_extension(content_type: &str, source_url: &str) -> String {
    extension_from_content_type(content_type)
        .map(str::to_owned)
        .or_else(|| infer_extension_from_url(source_url))
        .unwrap_or_else(|| "bin".to_owned())
}

pub async fn upload_bytes(
    settings: &Settings,
    client: &S3Client,
    media_id: i64,
    task_id: Uuid,
    source_url: &str,
    explicit_content_type: Option<&str>,
    body: Bytes,
) -> AppResult<StoredObjectMetadata> {
    let content_type = resolve_upload_content_type(explicit_content_type, source_url);
    let extension = resolve_upload_extension(&content_type, source_url);
    let object_key = build_object_key(
        &settings.config.storage.object_key_prefix,
        media_id,
        task_id,
        &extension,
    );
    let content_length = i64::try_from(body.len())
        .map_err(|_| AppError::upstream("object body exceeded i64 length limit"))?;
    let sha256_hex = format!("{:x}", Sha256::digest(&body));

    let response = client
        .put_object()
        .bucket(&settings.config.storage.bucket)
        .key(&object_key)
        .body(ByteStream::from(body))
        .content_type(content_type.clone())
        .content_length(content_length)
        .send()
        .await
        .map_err(|error| {
            AppError::upstream(format!("failed to upload object to storage: {error}"))
        })?;

    Ok(StoredObjectMetadata {
        id: Uuid::now_v7(),
        provider: settings.config.storage.provider.clone(),
        bucket: settings.config.storage.bucket.clone(),
        object_key,
        content_type,
        content_length,
        etag: response.e_tag().map(sanitize_etag),
        sha256_hex,
    })
}

fn sanitize_etag(value: &str) -> String {
    value.trim_matches('"').to_owned()
}

fn normalize_content_type(value: Option<&str>) -> Option<&str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.split(';').next().unwrap_or(value).trim())
        .filter(|value| !value.is_empty())
}

fn infer_content_type_from_url(source_url: &str) -> Option<&'static str> {
    let extension = infer_extension_from_url(source_url)?;
    match extension.as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "mp4" | "m4v" | "mov" => Some("video/mp4"),
        "m3u8" => Some("application/x-mpegURL"),
        _ => None,
    }
}

fn extension_from_content_type(content_type: &str) -> Option<&'static str> {
    let normalized = normalize_content_type(Some(content_type))?;
    if normalized.eq_ignore_ascii_case("image/jpeg") {
        Some("jpg")
    } else if normalized.eq_ignore_ascii_case("image/png") {
        Some("png")
    } else if normalized.eq_ignore_ascii_case("image/webp") {
        Some("webp")
    } else if normalized.eq_ignore_ascii_case("image/gif") {
        Some("gif")
    } else if normalized.eq_ignore_ascii_case("video/mp4") {
        Some("mp4")
    } else if normalized.eq_ignore_ascii_case("application/x-mpegurl")
        || normalized.eq_ignore_ascii_case("application/vnd.apple.mpegurl")
    {
        Some("m3u8")
    } else {
        None
    }
}

fn infer_extension_from_url(source_url: &str) -> Option<String> {
    let path = Url::parse(source_url)
        .ok()
        .map(|url| url.path().to_owned())
        .unwrap_or_else(|| {
            source_url
                .split(['?', '#'])
                .next()
                .unwrap_or(source_url)
                .to_owned()
        });

    Path::new(&path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .and_then(|value| if value.is_empty() { None } else { Some(value) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StorageSection;

    #[test]
    fn storage_subsystem_reports_configured_status() {
        let status = status(&StorageSection {
            provider: "s3_compatible".to_owned(),
            endpoint: "http://127.0.0.1:9000".to_owned(),
            region: "us-east-1".to_owned(),
            bucket: "tweet-db".to_owned(),
            object_key_prefix: "media".to_owned(),
            path_style: true,
        });

        assert!(status.active);
        assert_eq!(status.provider, "s3_compatible");
        assert_eq!(status.bucket, "tweet-db");
    }

    #[test]
    fn object_key_uses_prefix_media_id_and_task_id() {
        let task_id = Uuid::parse_str("0195f1df-0730-7f69-9a92-cd8e4eb4d001").unwrap();

        assert_eq!(
            build_object_key("media", 1712345678901234567, task_id, "jpg"),
            "media/1712345678901234567/0195f1df-0730-7f69-9a92-cd8e4eb4d001.jpg"
        );
    }

    #[test]
    fn upload_content_type_prefers_explicit_value() {
        assert_eq!(
            resolve_upload_content_type(
                Some("video/mp4; charset=binary"),
                "https://video.twimg.com/demo.m3u8"
            ),
            "video/mp4"
        );
        assert_eq!(
            resolve_upload_extension("video/mp4", "https://video.twimg.com/demo.m3u8"),
            "mp4"
        );
    }

    #[test]
    fn upload_content_type_can_be_inferred_from_url() {
        assert_eq!(
            resolve_upload_content_type(None, "https://pbs.twimg.com/media/demo-photo.jpeg"),
            "image/jpeg"
        );
        assert_eq!(
            resolve_upload_extension(
                "application/octet-stream",
                "https://pbs.twimg.com/media/demo-photo.jpeg"
            ),
            "jpeg"
        );
    }
}

use std::time::Duration;

use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::{Client, presigning::PresigningConfig};

use crate::{
    error::{AppError, AppResult},
    state::AppState,
};

pub async fn build_s3_client(state: &AppState) -> AppResult<Client> {
    let access_key = state
        .settings
        .secrets
        .storage_access_key
        .clone()
        .ok_or_else(|| AppError::config("STORAGE_ACCESS_KEY is required"))?;
    let secret_key = state
        .settings
        .secrets
        .storage_secret_key
        .clone()
        .ok_or_else(|| AppError::config("STORAGE_SECRET_KEY is required"))?;
    let credentials = Credentials::new(access_key, secret_key, None, None, "tweet-db");
    let shared_config = aws_config::defaults(BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new(
            state.settings.config.storage.region.clone(),
        ))
        .credentials_provider(credentials)
        .load()
        .await;
    let config = aws_sdk_s3::config::Builder::from(&shared_config)
        .endpoint_url(state.settings.config.storage.endpoint.clone())
        .force_path_style(state.settings.config.storage.path_style)
        .build();
    Ok(Client::from_conf(config))
}

pub async fn presign_get_object_url(
    client: &Client,
    bucket: &str,
    object_key: &str,
    expires_in: Duration,
) -> AppResult<String> {
    let config = PresigningConfig::expires_in(expires_in)
        .map_err(|error| AppError::config(format!("invalid presign expiry: {error}")))?;
    let request = client
        .get_object()
        .bucket(bucket)
        .key(object_key)
        .presigned(config)
        .await
        .map_err(|error| {
            AppError::upstream(format!("failed to presign object download: {error}"))
        })?;
    Ok(request.uri().to_string())
}

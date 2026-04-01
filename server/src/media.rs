use sqlx::{Postgres, Row, Transaction};
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    transfer,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedMediaFamily {
    Image,
    Video,
    AnimatedGif,
}

impl ManagedMediaFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::AnimatedGif => "animated_gif",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedIdentityKind {
    PostSourceUrl,
    ActorAvatarUrl,
    ActorBannerUrl,
}

impl ManagedIdentityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PostSourceUrl => "post_source_url",
            Self::ActorAvatarUrl => "actor_avatar_url",
            Self::ActorBannerUrl => "actor_banner_url",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManagedMediaSpec {
    pub source_kind: String,
    pub media_family: ManagedMediaFamily,
    pub identity_kind: ManagedIdentityKind,
    pub identity_value: String,
    pub fetch_url: String,
    pub display_url: String,
    pub thumb_url: Option<String>,
    pub content_type_hint: Option<String>,
    pub submission_id: Uuid,
    pub observed_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct ManagedMediaRecord {
    pub id: Uuid,
    pub transfer_enqueued: bool,
}

pub async fn register_managed_media(
    tx: &mut Transaction<'_, Postgres>,
    spec: &ManagedMediaSpec,
) -> AppResult<ManagedMediaRecord> {
    let identity_value = spec.identity_value.trim();
    if identity_value.is_empty() {
        return Err(AppError::bad_request(
            "managed media identity_value is required",
        ));
    }

    let fetch_url = spec.fetch_url.trim();
    if fetch_url.is_empty() {
        return Err(AppError::bad_request("managed media fetch_url is required"));
    }

    let row = sqlx::query(
        r#"
        INSERT INTO managed_media (
            id,
            source_kind,
            media_family,
            identity_kind,
            identity_value,
            fetch_url,
            display_url,
            thumb_url,
            content_type_hint,
            first_submission_id,
            last_submission_id,
            first_observed_at,
            last_observed_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10, $11, $11
        )
        ON CONFLICT (source_kind, identity_kind, identity_value) DO UPDATE
        SET media_family = EXCLUDED.media_family,
            fetch_url = EXCLUDED.fetch_url,
            display_url = EXCLUDED.display_url,
            thumb_url = EXCLUDED.thumb_url,
            content_type_hint = CASE
                WHEN EXCLUDED.content_type_hint = '' THEN managed_media.content_type_hint
                ELSE EXCLUDED.content_type_hint
            END,
            last_submission_id = EXCLUDED.last_submission_id,
            last_observed_at = EXCLUDED.last_observed_at,
            updated_at = NOW()
        RETURNING id
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(spec.source_kind.trim())
    .bind(spec.media_family.as_str())
    .bind(spec.identity_kind.as_str())
    .bind(identity_value)
    .bind(fetch_url)
    .bind(spec.display_url.trim())
    .bind(trimmed_option(&spec.thumb_url))
    .bind(trimmed_or_empty(&spec.content_type_hint))
    .bind(spec.submission_id)
    .bind(spec.observed_at)
    .fetch_one(&mut **tx)
    .await?;

    let media_id: Uuid = row.get("id");
    let transfer_enqueued = transfer::enqueue_media_transfer(
        tx,
        media_id,
        spec.source_kind.trim(),
        fetch_url,
        spec.content_type_hint.as_deref(),
    )
    .await?;

    Ok(ManagedMediaRecord {
        id: media_id,
        transfer_enqueued,
    })
}

pub fn normalize_post_source_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_owned())
}

pub fn normalize_actor_avatar_fetch_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.replace("_normal.", "_400x400."))
}

pub fn normalize_actor_banner_fetch_url(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('/');
    if value.is_empty() {
        return None;
    }

    let last = value.rsplit('/').next().unwrap_or_default();
    if is_dimension_segment(last) {
        return Some(value.to_owned());
    }

    Some(format!("{value}/1500x500"))
}

pub fn infer_content_type_from_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let url = Url::parse(value).ok()?;
    if let Some(format) = url
        .query_pairs()
        .find_map(|(key, value)| (key == "format").then(|| value.into_owned()))
    {
        if let Some(content_type) = content_type_for_extension(&format) {
            return Some(content_type.to_owned());
        }
    }

    let last = url.path_segments().and_then(|segments| segments.last())?;
    let (_, ext) = last.rsplit_once('.')?;
    content_type_for_extension(ext).map(ToOwned::to_owned)
}

fn trimmed_option(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

fn trimmed_or_empty(value: &Option<String>) -> String {
    value
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .to_owned()
}

fn is_dimension_segment(value: &str) -> bool {
    let Some((left, right)) = value.split_once('x') else {
        return false;
    };
    !left.is_empty()
        && !right.is_empty()
        && left.chars().all(|ch| ch.is_ascii_digit())
        && right.chars().all(|ch| ch.is_ascii_digit())
}

fn content_type_for_extension(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "mp4" => Some("video/mp4"),
        "m3u8" => Some("application/x-mpegURL"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_actor_avatar_to_full_size() {
        let value = normalize_actor_avatar_fetch_url(
            "https://pbs.twimg.com/profile_images/demo_normal.jpg",
        );
        assert_eq!(
            value.as_deref(),
            Some("https://pbs.twimg.com/profile_images/demo_400x400.jpg")
        );
    }

    #[test]
    fn normalizes_actor_banner_to_download_size() {
        let value =
            normalize_actor_banner_fetch_url("https://pbs.twimg.com/profile_banners/12345/67890");
        assert_eq!(
            value.as_deref(),
            Some("https://pbs.twimg.com/profile_banners/12345/67890/1500x500")
        );
    }

    #[test]
    fn infers_content_type_from_format_query() {
        let value =
            infer_content_type_from_url("https://pbs.twimg.com/media/demo?format=jpg&name=orig");
        assert_eq!(value.as_deref(), Some("image/jpeg"));
    }
}

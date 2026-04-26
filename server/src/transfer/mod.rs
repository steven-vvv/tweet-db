mod common;
mod download;
mod queue;
mod range;
#[cfg(test)]
mod tests;
mod upload;
mod worker;

pub use queue::enqueue_tasks;
pub use worker::start_workers;

use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    config::TransferSection,
    storage,
    tweet_model::{Media, MediaResource, MediaType},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSubsystemStatus {
    pub active: bool,
    pub worker_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedTransferSource {
    pub source_url: String,
    pub source_kind: &'static str,
    pub source_content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnqueueTransferTask {
    pub id: Uuid,
    pub media_id: i64,
    pub source_recorded_at: OffsetDateTime,
    pub source_url: String,
    pub source_kind: String,
    pub source_content_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferEnqueueStatus {
    Enqueued,
    AlreadyQueued,
    MissingSourceRecord,
}

pub fn status(section: &TransferSection) -> TransferSubsystemStatus {
    TransferSubsystemStatus {
        active: section.enabled,
        worker_count: section.worker_count,
    }
}

pub fn select_source(media: &Media, resource: &MediaResource) -> Option<SelectedTransferSource> {
    match media.media_type {
        MediaType::Photo => resource
            .media_url
            .as_ref()
            .map(|source_url| SelectedTransferSource {
                source_url: source_url.clone(),
                source_kind: "media_url",
                source_content_type: Some(storage::resolve_upload_content_type(
                    None,
                    source_url.as_str(),
                )),
            }),
        MediaType::Video | MediaType::AnimatedGif => {
            if let Some(video) = resource.video.as_ref() {
                if let Some(variant) = video
                    .variants
                    .iter()
                    .filter(|variant| variant.content_type.eq_ignore_ascii_case("video/mp4"))
                    .max_by_key(|variant| variant.bitrate.unwrap_or_default())
                    .or_else(|| video.variants.first())
                {
                    return Some(SelectedTransferSource {
                        source_url: variant.url.clone(),
                        source_kind: "video_variant",
                        source_content_type: Some(variant.content_type.clone()),
                    });
                }
            }

            resource
                .media_url
                .as_ref()
                .map(|source_url| SelectedTransferSource {
                    source_url: source_url.clone(),
                    source_kind: "media_url",
                    source_content_type: Some(storage::resolve_upload_content_type(
                        None,
                        source_url.as_str(),
                    )),
                })
        }
    }
}

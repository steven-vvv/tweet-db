use super::*;
use super::{
    common::TransferOptions,
    range::{ActivePartState, RangeSpec, build_range_specs, hash_ready_range_parts},
};
use crate::{
    config::TransferSection,
    tweet_model::{Media, MediaResource, MediaType},
};
use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use time::OffsetDateTime;

fn demo_photo_resource() -> MediaResource {
    MediaResource {
        media_id: 1,
        recorded_at: OffsetDateTime::parse(
            "2026-04-22T12:34:56Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap(),
        media_url: Some("https://pbs.twimg.com/media/demo.jpg".to_owned()),
        availability: Some("Available".to_owned()),
        video: None,
    }
}

#[test]
fn transfer_subsystem_reports_configured_status() {
    let status = status(&TransferSection {
        enabled: true,
        worker_count: 2,
        chunk_size_mb: 10,
        download_parallelism: 4,
        upload_parallelism: 4,
        max_in_flight_parts: 8,
        connect_timeout_seconds: 5,
        read_timeout_seconds: 30,
        attempt_timeout_seconds: 300,
        task_stale_timeout_seconds: 900,
        worker_poll_interval_seconds: 5,
        max_attempts: 8,
    });

    assert!(status.active);
    assert_eq!(status.worker_count, 2);
}

#[test]
fn transfer_options_disable_attempt_deadline_when_timeout_is_zero() {
    let options = TransferOptions::from_section(&TransferSection {
        enabled: true,
        worker_count: 2,
        chunk_size_mb: 10,
        download_parallelism: 4,
        upload_parallelism: 4,
        max_in_flight_parts: 8,
        connect_timeout_seconds: 5,
        read_timeout_seconds: 30,
        attempt_timeout_seconds: 0,
        task_stale_timeout_seconds: 900,
        worker_poll_interval_seconds: 5,
        max_attempts: 8,
    })
    .unwrap();

    assert_eq!(options.chunk_size_bytes, 10 * 1024 * 1024);
    assert_eq!(options.download_parallelism, 4);
    assert_eq!(options.upload_parallelism, 4);
    assert_eq!(options.max_in_flight_parts, 8);
    assert!(options.deadline.is_none());
}

#[test]
fn range_specs_cover_remaining_bytes_after_first_buffer() {
    let specs: Vec<_> = build_range_specs(10, 35, 10).unwrap().into_iter().collect();

    assert_eq!(
        specs,
        vec![
            RangeSpec {
                part_number: 2,
                start: 10,
                end_inclusive: 19
            },
            RangeSpec {
                part_number: 3,
                start: 20,
                end_inclusive: 29
            },
            RangeSpec {
                part_number: 4,
                start: 30,
                end_inclusive: 34
            },
        ]
    );
}

#[test]
fn range_hashing_waits_for_contiguous_parts_and_cleans_uploaded_states() {
    let mut part_states = BTreeMap::new();
    let mut hasher = Sha256::new();
    let mut content_length = 0;
    let mut next_hash_part = 1;

    part_states.insert(
        2,
        ActivePartState {
            bytes: Some(Bytes::from_static(b"bb")),
            upload_done: true,
        },
    );
    hash_ready_range_parts(
        &mut part_states,
        &mut hasher,
        &mut content_length,
        &mut next_hash_part,
    )
    .unwrap();

    assert_eq!(content_length, 0);
    assert_eq!(next_hash_part, 1);
    assert!(part_states.contains_key(&2));

    part_states.insert(
        1,
        ActivePartState {
            bytes: Some(Bytes::from_static(b"aa")),
            upload_done: true,
        },
    );
    hash_ready_range_parts(
        &mut part_states,
        &mut hasher,
        &mut content_length,
        &mut next_hash_part,
    )
    .unwrap();

    assert_eq!(content_length, 4);
    assert_eq!(next_hash_part, 3);
    assert!(part_states.is_empty());
    assert_eq!(
        format!("{:x}", hasher.finalize()),
        format!("{:x}", Sha256::digest(b"aabb"))
    );
}

#[test]
fn photo_transfer_uses_media_url() {
    let source = select_source(
        &Media {
            id: 1,
            media_type: MediaType::Photo,
            ..Default::default()
        },
        &demo_photo_resource(),
    )
    .unwrap();

    assert_eq!(source.source_kind, "media_url");
    assert_eq!(source.source_url, "https://pbs.twimg.com/media/demo.jpg");
    assert_eq!(source.source_content_type.as_deref(), Some("image/jpeg"));
}

#[test]
fn video_transfer_prefers_highest_bitrate_mp4_variant() {
    let source = select_source(
        &Media {
            id: 1,
            media_type: MediaType::Video,
            ..Default::default()
        },
        &MediaResource {
            video: Some(crate::tweet_model::MediaVideo {
                aspect_ratio_w: Some(16),
                aspect_ratio_h: Some(9),
                duration_ms: Some(1200),
                variants: vec![
                    crate::tweet_model::VideoVariant {
                        content_type: "application/x-mpegURL".to_owned(),
                        bitrate: None,
                        url: "https://video.twimg.com/demo.m3u8".to_owned(),
                    },
                    crate::tweet_model::VideoVariant {
                        content_type: "video/mp4".to_owned(),
                        bitrate: Some(832000),
                        url: "https://video.twimg.com/demo-832.mp4".to_owned(),
                    },
                    crate::tweet_model::VideoVariant {
                        content_type: "video/mp4".to_owned(),
                        bitrate: Some(2176000),
                        url: "https://video.twimg.com/demo-2176.mp4".to_owned(),
                    },
                ],
            }),
            ..demo_photo_resource()
        },
    )
    .unwrap();

    assert_eq!(source.source_kind, "video_variant");
    assert_eq!(source.source_url, "https://video.twimg.com/demo-2176.mp4");
    assert_eq!(source.source_content_type.as_deref(), Some("video/mp4"));
}

use std::{future::Future, time::Duration};

use tokio::time::Instant;

use crate::{
    config::TransferSection,
    error::{AppError, AppResult},
};

#[derive(Debug, Clone, Copy)]
pub(super) struct TransferOptions {
    pub(super) chunk_size_bytes: usize,
    pub(super) download_parallelism: usize,
    pub(super) upload_parallelism: usize,
    pub(super) max_in_flight_parts: usize,
    pub(super) deadline: Option<Instant>,
}

impl TransferOptions {
    pub(super) fn from_section(section: &TransferSection) -> AppResult<Self> {
        let chunk_size_bytes = section
            .chunk_size_mb
            .checked_mul(1024 * 1024)
            .ok_or_else(|| AppError::config("transfer.chunk_size_mb is too large"))?;
        let deadline = if section.attempt_timeout_seconds == 0 {
            None
        } else {
            Some(
                Instant::now()
                    .checked_add(Duration::from_secs(section.attempt_timeout_seconds))
                    .ok_or_else(|| {
                        AppError::config("transfer.attempt_timeout_seconds is too large")
                    })?,
            )
        };

        Ok(Self {
            chunk_size_bytes,
            download_parallelism: section.download_parallelism.max(1),
            upload_parallelism: section.upload_parallelism.max(1),
            max_in_flight_parts: section.max_in_flight_parts.max(1),
            deadline,
        })
    }
}

pub(super) fn ensure_valid_part_number(part_number: i32) -> AppResult<()> {
    if (1..=10_000).contains(&part_number) {
        Ok(())
    } else {
        Err(AppError::upstream(
            "multipart upload exceeded the S3 10000 part limit",
        ))
    }
}

pub(super) fn add_content_length(total: &mut u64, len: usize) -> AppResult<()> {
    let len =
        u64::try_from(len).map_err(|_| AppError::upstream("buffer exceeded u64 length limit"))?;
    *total = total
        .checked_add(len)
        .ok_or_else(|| AppError::upstream("object body exceeded u64 length limit"))?;
    Ok(())
}

pub(super) async fn with_deadline<T, F>(
    deadline: Option<Instant>,
    context: &str,
    future: F,
) -> AppResult<T>
where
    F: Future<Output = AppResult<T>>,
{
    if let Some(deadline) = deadline {
        tokio::time::timeout_at(deadline, future)
            .await
            .map_err(|_| AppError::upstream(format!("{context} timed out")))?
    } else {
        future.await
    }
}

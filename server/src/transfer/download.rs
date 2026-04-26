use bytes::{Bytes, BytesMut};
use reqwest::{
    Client as HttpClient, Response, StatusCode,
    header::{ACCEPT_RANGES, CONTENT_TYPE, RANGE},
};
use tokio::time::Instant;

use crate::error::{AppError, AppResult};

use super::{common::with_deadline, range::RangeSpec};

#[derive(Debug)]
pub(super) struct DownloadedPart {
    pub(super) part_number: i32,
    pub(super) bytes: Bytes,
}

pub(super) async fn open_initial_download(
    client: &HttpClient,
    source_url: &str,
    deadline: Option<Instant>,
) -> AppResult<InitialDownload> {
    let response = with_deadline(deadline, "download request", async {
        client.get(source_url).send().await.map_err(AppError::from)
    })
    .await?;
    if !response.status().is_success() {
        return Err(AppError::upstream(format!(
            "download returned status {} for {}",
            response.status(),
            source_url
        )));
    }

    let content_length = response.content_length();
    let supports_ranges = response_supports_ranges(&response);
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    Ok(InitialDownload {
        response,
        content_type,
        content_length,
        supports_ranges,
    })
}

pub(super) async fn download_range_part(
    client: &HttpClient,
    source_url: &str,
    spec: RangeSpec,
    deadline: Option<Instant>,
) -> AppResult<DownloadedPart> {
    let header_value = format!("bytes={}-{}", spec.start, spec.end_inclusive);
    let response = with_deadline(deadline, "range download request", async {
        client
            .get(source_url)
            .header(RANGE, header_value.clone())
            .send()
            .await
            .map_err(AppError::from)
    })
    .await?;

    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(AppError::upstream(format!(
            "range download returned status {} for {}",
            response.status(),
            source_url
        )));
    }

    if response
        .content_length()
        .is_some_and(|length| length != spec.len())
    {
        return Err(AppError::upstream(format!(
            "range download returned length {} for expected length {}",
            response.content_length().unwrap_or_default(),
            spec.len()
        )));
    }

    let expected_len = usize::try_from(spec.len())
        .map_err(|_| AppError::upstream("range part exceeded usize length limit"))?;
    let mut reader = ResponseByteReader::new(response);
    let mut buffer = BytesMut::with_capacity(expected_len);
    while buffer.len() < expected_len {
        let Some(bytes) = reader
            .read_some(expected_len - buffer.len(), deadline)
            .await?
        else {
            return Err(AppError::upstream(format!(
                "range download ended early for {}",
                source_url
            )));
        };
        buffer.extend_from_slice(&bytes);
    }

    Ok(DownloadedPart {
        part_number: spec.part_number,
        bytes: buffer.freeze(),
    })
}

pub(super) struct InitialDownload {
    pub(super) response: Response,
    pub(super) content_type: Option<String>,
    pub(super) content_length: Option<u64>,
    pub(super) supports_ranges: bool,
}

pub(super) struct ResponseByteReader {
    pub(super) response: Response,
    pending: Option<Bytes>,
    pending_offset: usize,
}

impl ResponseByteReader {
    pub(super) fn new(response: Response) -> Self {
        Self {
            response,
            pending: None,
            pending_offset: 0,
        }
    }

    pub(super) async fn read_some(
        &mut self,
        max_len: usize,
        deadline: Option<Instant>,
    ) -> AppResult<Option<Bytes>> {
        loop {
            if let Some(pending) = self.pending.as_ref() {
                if self.pending_offset < pending.len() {
                    let end = (self.pending_offset + max_len).min(pending.len());
                    let bytes = pending.slice(self.pending_offset..end);
                    self.pending_offset = end;
                    if self.pending_offset == pending.len() {
                        self.pending = None;
                        self.pending_offset = 0;
                    }
                    return Ok(Some(bytes));
                }
                self.pending = None;
                self.pending_offset = 0;
            }

            let next = with_deadline(deadline, "download body read", async {
                self.response.chunk().await.map_err(AppError::from)
            })
            .await?;
            match next {
                Some(bytes) if bytes.is_empty() => {}
                Some(bytes) => {
                    self.pending = Some(bytes);
                    self.pending_offset = 0;
                }
                None => return Ok(None),
            }
        }
    }
}

pub(super) async fn read_next_buffer(
    reader: &mut ResponseByteReader,
    chunk_size_bytes: usize,
    deadline: Option<Instant>,
) -> AppResult<Option<Bytes>> {
    let mut buffer = BytesMut::with_capacity(chunk_size_bytes);
    while buffer.len() < chunk_size_bytes {
        let Some(bytes) = reader
            .read_some(chunk_size_bytes - buffer.len(), deadline)
            .await?
        else {
            break;
        };
        buffer.extend_from_slice(&bytes);
    }

    if buffer.is_empty() {
        Ok(None)
    } else {
        Ok(Some(buffer.freeze()))
    }
}

fn response_supports_ranges(response: &Response) -> bool {
    response
        .headers()
        .get(ACCEPT_RANGES)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("bytes"))
        })
}

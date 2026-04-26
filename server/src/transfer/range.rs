use std::collections::{BTreeMap, VecDeque};

use bytes::Bytes;
use futures_util::stream::FuturesUnordered;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

use super::common::{add_content_length, ensure_valid_part_number};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RangeSpec {
    pub(super) part_number: i32,
    pub(super) start: u64,
    pub(super) end_inclusive: u64,
}

impl RangeSpec {
    pub(super) fn len(self) -> u64 {
        self.end_inclusive - self.start + 1
    }
}

#[derive(Debug)]
pub(super) struct ActivePartState {
    pub(super) bytes: Option<Bytes>,
    pub(super) upload_done: bool,
}

impl ActivePartState {
    pub(super) fn uploading_hashed() -> Self {
        Self {
            bytes: None,
            upload_done: false,
        }
    }

    pub(super) fn pending_hash(bytes: Bytes) -> Self {
        Self {
            bytes: Some(bytes),
            upload_done: false,
        }
    }
}

pub(super) fn build_range_specs(
    start_offset: u64,
    content_length: u64,
    chunk_size_bytes: usize,
) -> AppResult<VecDeque<RangeSpec>> {
    let chunk_size = u64::try_from(chunk_size_bytes)
        .map_err(|_| AppError::upstream("transfer chunk size exceeded u64 length limit"))?;
    let mut specs = VecDeque::new();
    let mut cursor = start_offset;
    let mut part_number = 2_i32;

    while cursor < content_length {
        ensure_valid_part_number(part_number)?;
        let next_cursor = cursor.saturating_add(chunk_size).min(content_length);
        specs.push_back(RangeSpec {
            part_number,
            start: cursor,
            end_inclusive: next_cursor - 1,
        });
        cursor = next_cursor;
        part_number += 1;
    }

    Ok(specs)
}

pub(super) fn hash_ready_range_parts(
    part_states: &mut BTreeMap<i32, ActivePartState>,
    hasher: &mut Sha256,
    content_length: &mut u64,
    next_hash_part: &mut i32,
) -> AppResult<()> {
    loop {
        let part_number = *next_hash_part;
        let Some(state) = part_states.get_mut(&part_number) else {
            break;
        };
        let Some(bytes) = state.bytes.take() else {
            break;
        };
        hasher.update(bytes.as_ref());
        add_content_length(content_length, bytes.len())?;
        let remove_state = state.upload_done;
        *next_hash_part += 1;
        if remove_state {
            part_states.remove(&part_number);
        }
    }

    Ok(())
}

pub(super) fn mark_range_part_uploaded(
    part_states: &mut BTreeMap<i32, ActivePartState>,
    part_number: i32,
) {
    let remove_state = if let Some(state) = part_states.get_mut(&part_number) {
        state.upload_done = true;
        state.bytes.is_none()
    } else {
        false
    };

    if remove_state {
        part_states.remove(&part_number);
    }
}

pub(super) fn active_range_part_count<T>(
    downloads: &FuturesUnordered<T>,
    part_states: &BTreeMap<i32, ActivePartState>,
) -> usize {
    downloads.len() + part_states.len()
}

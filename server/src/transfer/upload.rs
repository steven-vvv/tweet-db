use std::collections::BTreeMap;

use aws_sdk_s3::Client as S3Client;
use bytes::Bytes;
use futures_util::{
    StreamExt,
    future::{BoxFuture, FutureExt},
    stream::FuturesUnordered,
};
use reqwest::Client as HttpClient;
use sha2::{Digest, Sha256};
use tokio::{sync::Semaphore, time::Instant};
use uuid::Uuid;

use crate::{
    config::Settings,
    error::{AppError, AppResult},
    storage::{self, StoredObjectMetadata},
};

use super::{
    common::{TransferOptions, add_content_length, ensure_valid_part_number, with_deadline},
    download::{
        DownloadedPart, ResponseByteReader, download_range_part, open_initial_download,
        read_next_buffer,
    },
    range::{
        ActivePartState, active_range_part_count, build_range_specs, hash_ready_range_parts,
        mark_range_part_uploaded,
    },
};

type UploadFuture<'a> = BoxFuture<'a, AppResult<storage::UploadedPart>>;
type DownloadFuture<'a> = BoxFuture<'a, AppResult<DownloadedPart>>;

pub(super) async fn transfer_source_to_storage(
    settings: &Settings,
    download_client: &HttpClient,
    storage_client: &S3Client,
    media_id: i64,
    task_id: Uuid,
    source_url: &str,
    source_content_type: Option<&str>,
) -> AppResult<StoredObjectMetadata> {
    let options = TransferOptions::from_section(&settings.config.transfer)?;
    let initial = open_initial_download(download_client, source_url, options.deadline).await?;
    let explicit_content_type = source_content_type.or(initial.content_type.as_deref());
    let object = storage::prepare_upload(
        settings,
        media_id,
        task_id,
        source_url,
        explicit_content_type,
    );
    let mut reader = ResponseByteReader::new(initial.response);

    let Some(first_buffer) =
        read_next_buffer(&mut reader, options.chunk_size_bytes, options.deadline).await?
    else {
        return put_single_object(storage_client, object, Bytes::new(), options.deadline).await;
    };

    if initial
        .content_length
        .is_some_and(|content_length| content_length <= first_buffer.len() as u64)
        || first_buffer.len() < options.chunk_size_bytes
    {
        return put_single_object(storage_client, object, first_buffer, options.deadline).await;
    }

    if initial.supports_ranges
        && options.download_parallelism > 1
        && let Some(content_length) = initial.content_length
    {
        drop(reader);
        return upload_multipart_range(
            download_client,
            storage_client,
            object,
            source_url,
            first_buffer,
            content_length,
            options,
        )
        .await;
    }

    let mut preloaded_buffers = Vec::new();
    if initial.content_length.is_none() {
        match read_next_buffer(&mut reader, options.chunk_size_bytes, options.deadline).await? {
            Some(second_buffer) => preloaded_buffers.push(second_buffer),
            None => {
                return put_single_object(storage_client, object, first_buffer, options.deadline)
                    .await;
            }
        }
    }

    upload_multipart_sequential(
        storage_client,
        object,
        first_buffer,
        preloaded_buffers,
        reader,
        options,
    )
    .await
}

async fn put_single_object(
    storage_client: &S3Client,
    object: storage::PreparedStorageObject,
    body: Bytes,
    deadline: Option<Instant>,
) -> AppResult<StoredObjectMetadata> {
    let sha256_hex = format!("{:x}", Sha256::digest(&body));
    with_deadline(
        deadline,
        "single object upload",
        storage::put_object(storage_client, object, body, sha256_hex),
    )
    .await
}

async fn upload_multipart_sequential(
    storage_client: &S3Client,
    object: storage::PreparedStorageObject,
    first_buffer: Bytes,
    preloaded_buffers: Vec<Bytes>,
    mut reader: ResponseByteReader,
    options: TransferOptions,
) -> AppResult<StoredObjectMetadata> {
    let upload = with_deadline(
        options.deadline,
        "multipart upload creation",
        storage::create_multipart_upload(storage_client, &object),
    )
    .await?;
    let result = upload_multipart_sequential_inner(
        storage_client,
        object.clone(),
        upload.clone(),
        first_buffer,
        preloaded_buffers,
        &mut reader,
        options,
    )
    .await;

    finish_or_abort_multipart(storage_client, &object, &upload, result, options.deadline).await
}

async fn upload_multipart_sequential_inner(
    storage_client: &S3Client,
    object: storage::PreparedStorageObject,
    upload: storage::MultipartUpload,
    first_buffer: Bytes,
    preloaded_buffers: Vec<Bytes>,
    reader: &mut ResponseByteReader,
    options: TransferOptions,
) -> AppResult<StoredObjectMetadata> {
    let upload_semaphore = Semaphore::new(options.upload_parallelism);
    let mut uploads: FuturesUnordered<UploadFuture<'_>> = FuturesUnordered::new();
    let mut completed_parts = Vec::new();
    let mut hasher = Sha256::new();
    let mut content_length = 0_u64;
    let mut next_part_number = 1_i32;

    enqueue_sequential_part(
        storage_client,
        &object,
        &upload,
        &upload_semaphore,
        options,
        &mut uploads,
        &mut hasher,
        &mut content_length,
        &mut next_part_number,
        first_buffer,
    )?;

    for buffer in preloaded_buffers {
        wait_for_sequential_capacity(&mut uploads, options, &mut completed_parts).await?;
        enqueue_sequential_part(
            storage_client,
            &object,
            &upload,
            &upload_semaphore,
            options,
            &mut uploads,
            &mut hasher,
            &mut content_length,
            &mut next_part_number,
            buffer,
        )?;
    }

    while let Some(buffer) =
        read_next_buffer(reader, options.chunk_size_bytes, options.deadline).await?
    {
        wait_for_sequential_capacity(&mut uploads, options, &mut completed_parts).await?;
        enqueue_sequential_part(
            storage_client,
            &object,
            &upload,
            &upload_semaphore,
            options,
            &mut uploads,
            &mut hasher,
            &mut content_length,
            &mut next_part_number,
            buffer,
        )?;
    }

    while let Some(uploaded) = uploads.next().await {
        completed_parts.push(uploaded?);
    }
    drop(uploads);
    drop(upload_semaphore);

    complete_multipart_with_hash(
        storage_client,
        object,
        upload,
        completed_parts,
        content_length,
        hasher,
        options.deadline,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
fn enqueue_sequential_part<'a>(
    storage_client: &'a S3Client,
    object: &'a storage::PreparedStorageObject,
    upload: &'a storage::MultipartUpload,
    upload_semaphore: &'a Semaphore,
    options: TransferOptions,
    uploads: &mut FuturesUnordered<UploadFuture<'a>>,
    hasher: &mut Sha256,
    content_length: &mut u64,
    next_part_number: &mut i32,
    buffer: Bytes,
) -> AppResult<()> {
    ensure_valid_part_number(*next_part_number)?;
    hasher.update(buffer.as_ref());
    add_content_length(content_length, buffer.len())?;
    uploads.push(
        upload_part_with_limit(
            storage_client,
            object,
            upload,
            upload_semaphore,
            options.deadline,
            *next_part_number,
            buffer,
        )
        .boxed(),
    );
    *next_part_number += 1;
    Ok(())
}

async fn wait_for_sequential_capacity<'a>(
    uploads: &mut FuturesUnordered<UploadFuture<'a>>,
    options: TransferOptions,
    completed_parts: &mut Vec<storage::UploadedPart>,
) -> AppResult<()> {
    while uploads.len() >= options.max_in_flight_parts {
        let Some(uploaded) = uploads.next().await else {
            break;
        };
        completed_parts.push(uploaded?);
    }
    Ok(())
}

async fn upload_multipart_range(
    download_client: &HttpClient,
    storage_client: &S3Client,
    object: storage::PreparedStorageObject,
    source_url: &str,
    first_buffer: Bytes,
    content_length: u64,
    options: TransferOptions,
) -> AppResult<StoredObjectMetadata> {
    let upload = with_deadline(
        options.deadline,
        "multipart upload creation",
        storage::create_multipart_upload(storage_client, &object),
    )
    .await?;
    let result = upload_multipart_range_inner(
        download_client,
        storage_client,
        object.clone(),
        upload.clone(),
        source_url,
        first_buffer,
        content_length,
        options,
    )
    .await;

    finish_or_abort_multipart(storage_client, &object, &upload, result, options.deadline).await
}

#[allow(clippy::too_many_arguments)]
async fn upload_multipart_range_inner(
    download_client: &HttpClient,
    storage_client: &S3Client,
    object: storage::PreparedStorageObject,
    upload: storage::MultipartUpload,
    source_url: &str,
    first_buffer: Bytes,
    content_length: u64,
    options: TransferOptions,
) -> AppResult<StoredObjectMetadata> {
    let upload_semaphore = Semaphore::new(options.upload_parallelism);
    let mut downloads: FuturesUnordered<DownloadFuture<'_>> = FuturesUnordered::new();
    let mut uploads: FuturesUnordered<UploadFuture<'_>> = FuturesUnordered::new();
    let mut pending_ranges = build_range_specs(
        first_buffer.len() as u64,
        content_length,
        options.chunk_size_bytes,
    )?;
    let mut part_states = BTreeMap::<i32, ActivePartState>::new();
    let mut completed_parts = Vec::new();
    let mut hasher = Sha256::new();
    let mut hashed_content_length = 0_u64;
    let mut next_hash_part = 1_i32;

    part_states.insert(1, ActivePartState::uploading_hashed());
    uploads.push(
        upload_part_with_limit(
            storage_client,
            &object,
            &upload,
            &upload_semaphore,
            options.deadline,
            1,
            first_buffer.clone(),
        )
        .boxed(),
    );
    part_states
        .get_mut(&1)
        .expect("first part state exists")
        .bytes = Some(first_buffer);
    hash_ready_range_parts(
        &mut part_states,
        &mut hasher,
        &mut hashed_content_length,
        &mut next_hash_part,
    )?;

    loop {
        while downloads.len() < options.download_parallelism
            && active_range_part_count(&downloads, &part_states) < options.max_in_flight_parts
            && let Some(spec) = pending_ranges.pop_front()
        {
            downloads.push(
                download_range_part(download_client, source_url, spec, options.deadline).boxed(),
            );
        }

        if pending_ranges.is_empty()
            && downloads.is_empty()
            && uploads.is_empty()
            && part_states.is_empty()
        {
            break;
        }

        tokio::select! {
            downloaded = downloads.next(), if !downloads.is_empty() => {
                let downloaded = downloaded
                    .expect("download future existed")?;
                part_states.insert(downloaded.part_number, ActivePartState::pending_hash(downloaded.bytes.clone()));
                uploads.push(upload_part_with_limit(
                    storage_client,
                    &object,
                    &upload,
                    &upload_semaphore,
                    options.deadline,
                    downloaded.part_number,
                    downloaded.bytes,
                ).boxed());
                hash_ready_range_parts(
                    &mut part_states,
                    &mut hasher,
                    &mut hashed_content_length,
                    &mut next_hash_part,
                )?;
            }
            uploaded = uploads.next(), if !uploads.is_empty() => {
                let uploaded = uploaded
                    .expect("upload future existed")?;
                mark_range_part_uploaded(&mut part_states, uploaded.part_number);
                completed_parts.push(uploaded);
            }
        }
    }

    if hashed_content_length != content_length {
        return Err(AppError::upstream(format!(
            "downloaded content length {} did not match expected content length {}",
            hashed_content_length, content_length
        )));
    }
    drop(downloads);
    drop(uploads);
    drop(upload_semaphore);

    complete_multipart_with_hash(
        storage_client,
        object,
        upload,
        completed_parts,
        hashed_content_length,
        hasher,
        options.deadline,
    )
    .await
}

async fn finish_or_abort_multipart(
    storage_client: &S3Client,
    object: &storage::PreparedStorageObject,
    upload: &storage::MultipartUpload,
    result: AppResult<StoredObjectMetadata>,
    deadline: Option<Instant>,
) -> AppResult<StoredObjectMetadata> {
    match result {
        Ok(uploaded) => Ok(uploaded),
        Err(error) => {
            if let Err(abort_error) = with_deadline(
                deadline,
                "multipart upload abort",
                storage::abort_multipart_upload(storage_client, object, upload),
            )
            .await
            {
                tracing::warn!(error = %abort_error, "failed to abort multipart upload after transfer error");
            }
            Err(error)
        }
    }
}

async fn complete_multipart_with_hash(
    storage_client: &S3Client,
    object: storage::PreparedStorageObject,
    upload: storage::MultipartUpload,
    completed_parts: Vec<storage::UploadedPart>,
    content_length: u64,
    hasher: Sha256,
    deadline: Option<Instant>,
) -> AppResult<StoredObjectMetadata> {
    let content_length = i64::try_from(content_length)
        .map_err(|_| AppError::upstream("object body exceeded i64 length limit"))?;
    let sha256_hex = format!("{:x}", hasher.finalize());

    with_deadline(
        deadline,
        "multipart upload completion",
        storage::complete_multipart_upload(
            storage_client,
            object,
            upload,
            completed_parts,
            content_length,
            sha256_hex,
        ),
    )
    .await
}

async fn upload_part_with_limit(
    storage_client: &S3Client,
    object: &storage::PreparedStorageObject,
    upload: &storage::MultipartUpload,
    upload_semaphore: &Semaphore,
    deadline: Option<Instant>,
    part_number: i32,
    buffer: Bytes,
) -> AppResult<storage::UploadedPart> {
    let _permit = upload_semaphore
        .acquire()
        .await
        .map_err(|_| AppError::upstream("multipart upload semaphore was closed"))?;
    with_deadline(
        deadline,
        "multipart part upload",
        storage::upload_multipart_part(storage_client, object, upload, part_number, buffer),
    )
    .await
}

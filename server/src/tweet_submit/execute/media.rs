use super::*;

pub(super) async fn write_media_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<Media>],
    results: &mut [ObjectResultBuilder],
) {
    if items.is_empty() {
        return;
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.upsert_media_changed(&values).await {
        Ok(changed) => {
            for item in items {
                if changed.contains(&item.value.id) {
                    results[item.index].accepted("media", "inserted_or_filled");
                } else {
                    results[item.index].skipped("media", "unchanged_or_existing");
                }
            }
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("media", error.to_string());
            }
        }
    }
}

pub(super) async fn write_media_resources_batch(
    store: &TweetStore<'_>,
    items: &[Indexed<MediaResource>],
    results: &mut [ObjectResultBuilder],
) -> HashMap<(i64, OffsetDateTime), ConditionalWrite> {
    if items.is_empty() {
        return HashMap::new();
    }

    let values = items
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    match store.append_media_resources_if_changed_many(&values).await {
        Ok(statuses) => {
            for item in items {
                let key = (item.value.media_id, item.value.recorded_at);
                match statuses.get(&key).copied() {
                    Some(write) => {
                        record_conditional_write(&mut results[item.index], "media_resource", write)
                    }
                    None => {
                        results[item.index].failed("media_resource", "missing batch write status")
                    }
                }
            }
            statuses
        }
        Err(error) => {
            for item in items {
                results[item.index].failed("media_resource", error.to_string());
            }
            HashMap::new()
        }
    }
}

pub(super) async fn enqueue_prepared_media_transfers(
    state: &AppState,
    media_items: &[Indexed<Media>],
    resource_items: &[Indexed<MediaResource>],
    resource_statuses: &HashMap<(i64, OffsetDateTime), ConditionalWrite>,
    results: &mut [ObjectResultBuilder],
) {
    let mut media_by_index = HashMap::with_capacity(media_items.len());
    for item in media_items {
        media_by_index.insert(item.index, &item.value);
    }

    let transfer_status = transfer::status(&state.settings.config.transfer);
    let mut tasks = Vec::new();
    let mut task_indices = Vec::new();

    for item in resource_items {
        let key = (item.value.media_id, item.value.recorded_at);
        if !should_enqueue_media_transfer(resource_statuses.get(&key).copied()) {
            continue;
        }

        if !transfer_status.active {
            results[item.index].skipped("media_transfer", "transfer_disabled");
            continue;
        }

        let Some(media) = media_by_index.get(&item.index).copied() else {
            results[item.index].failed("media_transfer", "missing_media_payload");
            continue;
        };

        let Some(source) = transfer::select_source(media, &item.value) else {
            results[item.index].skipped("media_transfer", "source_unavailable");
            continue;
        };

        tasks.push(EnqueueTransferTask {
            id: Uuid::now_v7(),
            media_id: item.value.media_id,
            source_recorded_at: item.value.recorded_at,
            source_url: source.source_url,
            source_kind: source.source_kind.to_owned(),
            source_content_type: source.source_content_type,
        });
        task_indices.push((item.index, key));
    }

    if tasks.is_empty() {
        return;
    }

    match transfer::enqueue_tasks(&state.db, &tasks).await {
        Ok(statuses) => {
            for (index, key) in task_indices {
                match statuses.get(&key).copied() {
                    Some(TransferEnqueueStatus::Enqueued) => {
                        results[index].accepted("media_transfer", "enqueued");
                    }
                    Some(TransferEnqueueStatus::AlreadyQueued) => {
                        results[index].skipped("media_transfer", "already_enqueued");
                    }
                    Some(TransferEnqueueStatus::MissingSourceRecord) => {
                        results[index].failed("media_transfer", "missing_source_record");
                    }
                    None => results[index].failed("media_transfer", "missing_enqueue_status"),
                }
            }
        }
        Err(error) => {
            for (index, _) in task_indices {
                results[index].failed("media_transfer", error.to_string());
            }
        }
    }
}

fn should_enqueue_media_transfer(status: Option<ConditionalWrite>) -> bool {
    matches!(status, Some(ConditionalWrite::Inserted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_transfer_enqueue_requires_inserted_resource() {
        assert!(should_enqueue_media_transfer(Some(
            ConditionalWrite::Inserted
        )));
        assert!(!should_enqueue_media_transfer(Some(
            ConditionalWrite::SkippedUnchanged
        )));
        assert!(!should_enqueue_media_transfer(Some(
            ConditionalWrite::SkippedDuplicate
        )));
        assert!(!should_enqueue_media_transfer(Some(
            ConditionalWrite::SkippedMissingParent
        )));
        assert!(!should_enqueue_media_transfer(None));
    }
}

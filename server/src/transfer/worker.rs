use std::{sync::Arc, time::Duration};

use aws_sdk_s3::Client as S3Client;
use reqwest::Client as HttpClient;
use sqlx::PgPool;
use time::OffsetDateTime;
use tokio::time::sleep;

use crate::{
    config::Settings,
    error::{AppError, AppResult},
    state::AppState,
    storage,
};

use super::{
    queue::{
        ClaimedTransferTask, claim_next_task, expire_stale_tasks, mark_task_failed,
        persist_completed_task,
    },
    status,
    upload::transfer_source_to_storage,
};

#[derive(Clone)]
struct WorkerRuntime {
    settings: Arc<Settings>,
    download_client: HttpClient,
    storage_client: S3Client,
}

pub fn start_workers(state: AppState) -> AppResult<()> {
    let status = status(&state.settings.config.transfer);
    if !status.active {
        tracing::info!("media transfer workers are disabled by config");
        return Ok(());
    }

    if status.worker_count == 0 {
        tracing::info!("media transfer queue is enabled with zero workers");
        return Ok(());
    }

    let runtime = Arc::new(WorkerRuntime {
        settings: state.settings.clone(),
        download_client: build_download_client(&state.settings)?,
        storage_client: storage::build_client(&state.settings)?,
    });

    for worker_index in 0..status.worker_count {
        let runtime = runtime.clone();
        let db = state.db.clone();
        let worker_name = format!("transfer-worker-{}", worker_index + 1);
        tokio::spawn(async move {
            run_worker_loop(db, runtime, worker_name).await;
        });
    }

    tracing::info!(
        worker_count = status.worker_count,
        "started media transfer workers"
    );
    Ok(())
}

fn build_download_client(settings: &Settings) -> AppResult<HttpClient> {
    let transfer = &settings.config.transfer;
    let mut builder = HttpClient::builder().redirect(reqwest::redirect::Policy::none());
    if transfer.connect_timeout_seconds > 0 {
        builder = builder.connect_timeout(Duration::from_secs(transfer.connect_timeout_seconds));
    }
    if transfer.read_timeout_seconds > 0 {
        builder = builder.read_timeout(Duration::from_secs(transfer.read_timeout_seconds));
    }
    builder
        .build()
        .map_err(|error| AppError::config(format!("failed to build transfer http client: {error}")))
}

async fn run_worker_loop(db: PgPool, runtime: Arc<WorkerRuntime>, worker_name: String) {
    let poll_interval = Duration::from_secs(
        runtime
            .settings
            .config
            .transfer
            .worker_poll_interval_seconds,
    );
    let task_stale_timeout =
        Duration::from_secs(runtime.settings.config.transfer.task_stale_timeout_seconds);
    let max_attempts = runtime.settings.config.transfer.max_attempts;

    loop {
        let stale_cutoff = OffsetDateTime::now_utc() - duration_to_time(task_stale_timeout);
        if let Err(error) = expire_stale_tasks(&db, stale_cutoff, max_attempts).await {
            tracing::warn!(worker = %worker_name, error = %error, "failed to expire stale media transfer tasks");
        }

        match claim_next_task(&db, stale_cutoff, max_attempts, &worker_name).await {
            Ok(Some(task)) => {
                tracing::info!(
                    worker = %worker_name,
                    task_id = %task.id,
                    media_id = task.media_id,
                    source_kind = %task.source_kind,
                    source_recorded_at = %task.source_recorded_at,
                    attempt_count = task.attempt_count,
                    "processing media transfer task"
                );

                if let Err(error) = process_task(&db, &runtime, &task).await {
                    let message = truncate_error_message(&error.to_string(), 2048);
                    tracing::warn!(
                        worker = %worker_name,
                        task_id = %task.id,
                        media_id = task.media_id,
                        error = %message,
                        "media transfer task failed"
                    );
                    if let Err(update_error) =
                        mark_task_failed(&db, task.id, &message, max_attempts).await
                    {
                        tracing::warn!(
                            worker = %worker_name,
                            task_id = %task.id,
                            error = %update_error,
                            "failed to update media transfer task status after error"
                        );
                    }
                }
            }
            Ok(None) => sleep(poll_interval).await,
            Err(error) => {
                tracing::warn!(worker = %worker_name, error = %error, "failed to claim media transfer task");
                sleep(poll_interval).await;
            }
        }
    }
}

async fn process_task(
    db: &PgPool,
    runtime: &WorkerRuntime,
    task: &ClaimedTransferTask,
) -> AppResult<()> {
    let uploaded = transfer_source_to_storage(
        &runtime.settings,
        &runtime.download_client,
        &runtime.storage_client,
        task.media_id,
        task.id,
        &task.source_url,
        task.source_content_type.as_deref(),
    )
    .await?;
    persist_completed_task(db, task.id, uploaded).await
}

fn duration_to_time(duration: Duration) -> time::Duration {
    time::Duration::seconds(i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
}

fn truncate_error_message(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

use std::time::Duration;

use sqlx::PgPool;
use time::OffsetDateTime;
use tokio::time::sleep;

use crate::{error::AppResult, state::AppState};

use super::queue::{claim_next_tasks, expire_stale_tasks, mark_task_completed, mark_task_failed};

pub fn start_workers(state: AppState) -> AppResult<()> {
    let Some(search) = state.search.clone() else {
        return Ok(());
    };

    let config = &state.settings.config.search;
    for worker_index in 0..config.worker_count {
        let db = state.db.clone();
        let search = search.clone();
        let worker_name = format!("search-worker-{}", worker_index + 1);
        let poll_interval = Duration::from_secs(config.commit_interval_seconds);
        let stale_timeout = Duration::from_secs(config.stale_timeout_seconds);
        let max_attempts = config.max_attempts;
        let batch_size = config.queue_batch_size;

        tokio::spawn(async move {
            run_worker_loop(
                db,
                search,
                worker_name,
                poll_interval,
                stale_timeout,
                max_attempts,
                batch_size,
            )
            .await;
        });
    }

    tracing::info!(
        worker_count = config.worker_count,
        "started search index workers"
    );
    Ok(())
}

async fn run_worker_loop(
    db: PgPool,
    search: super::SearchState,
    worker_name: String,
    poll_interval: Duration,
    stale_timeout: Duration,
    max_attempts: i32,
    batch_size: usize,
) {
    loop {
        let stale_cutoff = OffsetDateTime::now_utc() - duration_to_time(stale_timeout);
        if let Err(error) = expire_stale_tasks(&db, stale_cutoff, max_attempts).await {
            tracing::warn!(worker = %worker_name, error = %error, "failed to expire stale search index tasks");
        }

        match claim_next_tasks(&db, stale_cutoff, max_attempts, &worker_name, batch_size).await {
            Ok(tasks) if tasks.is_empty() => sleep(poll_interval).await,
            Ok(tasks) => {
                for task in tasks {
                    tracing::info!(
                        worker = %worker_name,
                        task_id = %task.id,
                        target_kind = %task.target_kind,
                        target_id = task.target_id,
                        attempt_count = task.attempt_count,
                        "processing search index task"
                    );

                    match search.index_task(&db, &task).await {
                        Ok(()) => match mark_task_completed(&db, &task).await {
                            Ok(true) => {}
                            Ok(false) => tracing::info!(
                                worker = %worker_name,
                                task_id = %task.id,
                                "search index task claim was refreshed before completion"
                            ),
                            Err(error) => {
                                tracing::warn!(
                                    worker = %worker_name,
                                    task_id = %task.id,
                                    error = %error,
                                    "failed to mark search index task completed"
                                );
                            }
                        },
                        Err(error) => {
                            tracing::warn!(
                                worker = %worker_name,
                                task_id = %task.id,
                                target_kind = %task.target_kind,
                                target_id = task.target_id,
                                error = %error,
                                "search index task failed"
                            );
                            match mark_task_failed(&db, &task, &error.to_string(), max_attempts)
                                .await
                            {
                                Ok(true) => {}
                                Ok(false) => tracing::info!(
                                    worker = %worker_name,
                                    task_id = %task.id,
                                    "search index task claim was refreshed before failure update"
                                ),
                                Err(update_error) => {
                                    tracing::warn!(
                                        worker = %worker_name,
                                        task_id = %task.id,
                                        error = %update_error,
                                        "failed to update search index task status after error"
                                    );
                                }
                            }
                        }
                    }
                }
            }
            Err(error) => {
                tracing::warn!(worker = %worker_name, error = %error, "failed to claim search index tasks");
                sleep(poll_interval).await;
            }
        }
    }
}

fn duration_to_time(duration: Duration) -> time::Duration {
    time::Duration::seconds(i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
}

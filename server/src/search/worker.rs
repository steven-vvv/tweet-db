use std::time::Duration;

use sqlx::PgPool;
use time::OffsetDateTime;
use tokio::time::sleep;

use crate::{error::AppResult, state::AppState};

use super::{
    IndexTargetKind, SearchState,
    queue::{
        ClaimedIndexTask, claim_next_tasks, expire_stale_tasks, mark_task_completed,
        mark_task_failed, mark_tasks_completed,
    },
};

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
                process_task_batch(&db, &search, &worker_name, &tasks, max_attempts).await;
            }
            Err(error) => {
                tracing::warn!(worker = %worker_name, error = %error, "failed to claim search index tasks");
                sleep(poll_interval).await;
            }
        }
    }
}

async fn process_task_batch(
    db: &PgPool,
    search: &SearchState,
    worker_name: &str,
    tasks: &[ClaimedIndexTask],
    max_attempts: i32,
) {
    match count_tasks_by_kind(tasks) {
        Ok(counts) => tracing::info!(
            worker = %worker_name,
            task_count = tasks.len(),
            user_count = counts.users,
            tweet_count = counts.tweets,
            "processing search index task batch"
        ),
        Err(error) => {
            tracing::warn!(
                worker = %worker_name,
                error = %error,
                "failed to classify search index task batch"
            );
            process_tasks_individually(db, search, worker_name, tasks, max_attempts).await;
            return;
        }
    }

    match search.index_tasks(db, tasks).await {
        Ok(()) => match mark_tasks_completed(db, tasks).await {
            Ok(updated) => {
                if updated < tasks.len() {
                    tracing::info!(
                        worker = %worker_name,
                        task_count = tasks.len(),
                        completed_count = updated,
                        "some search index task claims were refreshed before completion"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(
                    worker = %worker_name,
                    task_count = tasks.len(),
                    error = %error,
                    "failed to mark search index task batch completed"
                );
            }
        },
        Err(error) => {
            tracing::warn!(
                worker = %worker_name,
                task_count = tasks.len(),
                error = %error,
                "search index task batch failed"
            );
            process_tasks_individually(db, search, worker_name, tasks, max_attempts).await;
        }
    }
}

async fn process_tasks_individually(
    db: &PgPool,
    search: &SearchState,
    worker_name: &str,
    tasks: &[ClaimedIndexTask],
    max_attempts: i32,
) {
    for task in tasks {
        process_single_task(db, search, worker_name, task, max_attempts).await;
    }
}

async fn process_single_task(
    db: &PgPool,
    search: &SearchState,
    worker_name: &str,
    task: &ClaimedIndexTask,
    max_attempts: i32,
) {
    tracing::info!(
        worker = %worker_name,
        task_id = %task.id,
        target_kind = %task.target_kind,
        target_id = task.target_id,
        attempt_count = task.attempt_count,
        "processing search index task"
    );

    match search.index_task(db, task).await {
        Ok(()) => match mark_task_completed(db, task).await {
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
            match mark_task_failed(db, task, &error.to_string(), max_attempts).await {
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

#[derive(Debug, Default, PartialEq, Eq)]
struct TaskKindCounts {
    users: usize,
    tweets: usize,
}

fn count_tasks_by_kind(tasks: &[ClaimedIndexTask]) -> AppResult<TaskKindCounts> {
    let mut counts = TaskKindCounts::default();
    for task in tasks {
        match task.parsed_kind()? {
            IndexTargetKind::User => counts.users += 1,
            IndexTargetKind::Tweet => counts.tweets += 1,
        }
    }
    Ok(counts)
}

fn duration_to_time(duration: Duration) -> time::Duration {
    time::Duration::seconds(i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;
    use uuid::Uuid;

    use super::*;

    fn task(kind: &str, target_id: i64) -> ClaimedIndexTask {
        ClaimedIndexTask {
            id: Uuid::from_u128(u128::try_from(target_id).unwrap()),
            target_kind: kind.to_owned(),
            target_id,
            attempt_count: 1,
            claimed_by: "test-worker".to_owned(),
            claimed_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn counts_tasks_by_kind() {
        let tasks = vec![task("user", 1), task("tweet", 2), task("tweet", 3)];

        assert_eq!(
            count_tasks_by_kind(&tasks).unwrap(),
            TaskKindCounts {
                users: 1,
                tweets: 2,
            }
        );
    }

    #[test]
    fn rejects_unknown_task_kind() {
        let tasks = vec![task("other", 1)];

        assert!(count_tasks_by_kind(&tasks).is_err());
    }
}

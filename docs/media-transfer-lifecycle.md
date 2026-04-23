# Media Transfer Lifecycle

本文档记录 `tweet-db` 当前媒体转储闭环的服务端行为，覆盖自动入队、后台 worker、对象存储写入和本地联调方式。

## Data Flow

1. `x-monkey` 或其他公开调用方通过 `POST /api/v1/tweet/submit` 提交 `media` 对象。
2. 服务端沿用现有 `tweet.media` 与 `tweet.media_resource` 写入策略。
3. 当本次提交写入了新的 `tweet.media_resource` 版本时，服务端按源选择规则创建 `media.transfer_task`。
4. 后台 worker claim `pending` 任务，下载远端媒体资源并上传到对象存储。
5. 上传完成后写入 `media.storage_object`，并把任务状态更新为 `completed`。

## Source Selection

- `photo`：使用 `tweet.media_resource.media_url`。
- `video` 和 `animated_gif`：优先选择 `video.variants` 中码率最高的 `video/mp4`。
- `video` 和 `animated_gif` 的回退路径：使用首个可用 variant；当 variant 缺失时回退到 `tweet.media_resource.media_url`。
- 当前轮次只在成功选出 `source_url` 时入队；否则提交结果中的 `media_transfer` 记为 `source_unavailable`。

## Schemas

- `tweet.media_resource`
  - 继续承担媒体资源版本追加职责。
  - `(media_id, recorded_at)` 代表一个可转储资源版本。
- `media.transfer_task`
  - 以 `(media_id, source_recorded_at)` 去重。
  - `status` 取值为 `pending`、`processing`、`completed`、`failed`。
  - `storage_object_id` 在上传完成后关联到 `media.storage_object`。
- `media.storage_object`
  - 保存对象存储位置、内容类型、长度、ETag、SHA-256。
- `media.v_latest_transfer_overview`
  - 提供按 `media_id` 汇总的最新任务与对象存储状态视图。

## Runtime

- worker 由 `server/src/app.rs` 在服务启动时按 `[transfer]` 配置自动启动。
- `transfer.enabled = true` 时，提交链路会自动入队。
- `worker_count` 控制并发 worker 数量。
- `attempt_timeout_seconds` 控制单次 claim 的超时回收窗口。
- 失败任务在 `attempt_count < max_attempts` 时回到 `pending`，达到上限后进入 `failed`。

## Object Keys

- 对象 key 规则固定为：

```text
{object_key_prefix}/{media_id}/{transfer_task_id}.{ext}
```

- `ext` 优先从 content-type 推导，之后回退到源 URL 后缀。
- 当前实现使用内存缓冲区下载并上传整个对象。

## Local Smoke Test

1. 准备 PostgreSQL，并让服务端能执行 migration。
2. 准备 S3-compatible 存储，例如 MinIO，并配置：
   - `STORAGE_ACCESS_KEY`
   - `STORAGE_SECRET_KEY`
   - `server/config/default.toml` 中的 `[storage]`
3. 启动服务端：

```bash
cargo run --manifest-path server/Cargo.toml
```

4. 使用公开接口提交带有 `mediaUrl` 或 `video.variants` 的媒体 payload。
5. 观察提交结果中的 `media_transfer` 操作状态。
6. 用以下 SQL 检查任务与对象状态：

```sql
SELECT *
FROM media.transfer_task
ORDER BY created_at DESC
LIMIT 20;
```

```sql
SELECT *
FROM media.v_latest_transfer_overview
ORDER BY media_id DESC
LIMIT 20;
```

## Current Boundaries

- 当前闭环只覆盖 tweet media。
- 用户头像、横幅、卡片资源仍在后续范围内。
- 当前上传路径使用单对象缓冲上传，`chunk_size_mb`、`download_parallelism`、`upload_parallelism`、`max_in_flight_parts` 还没有进入 multipart 上传实现。

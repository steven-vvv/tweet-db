# 架构说明

本文档记录当前实现的主要设计。数据库结构以 `server/migrations/` 为准；这里只解释为什么这么组织、写入时应遵守什么规则。

## 系统结构

项目分成两部分：

- `server/`：Rust Axum 服务，负责认证、API、数据库写入、查询和媒体转储。
- `webui/`：Vue 前端，包含账号页和管理员管理台。

服务启动流程：

1. 读取 TOML 配置和环境变量。
2. 连接 PostgreSQL。
3. 执行 SQLx migrations。
4. 加载 `tweet.string_dict` 缓存。
5. 按 `[transfer]` 配置启动媒体转储 worker。
6. 挂载 API 路由和前端静态文件路由。

## 服务端代码组织

服务端模块按领域和执行阶段分层：

- `config/`：配置模型、路径解析、TLS/HTTPS 校验和配置测试。
- `admin/`：管理员管理接口、cursor 时间兼容、分页、转储动作和响应格式化。
- `tweet_submit/`：提交 API 合同、handler、批处理状态、结果汇总、转换和写入执行。
- `tweet_query/`：查询 API 合同、数据库读取类型、查询 fetch、decode 和 JSON build。
- `tweet_store/`：数据库读写封装，tweet 写入按 places、tweets、edits、policies、stats、community notes 拆分。
- `transfer/`：媒体转储队列、worker、下载、range 分片、上传和传输测试。

对外入口集中在各领域的 `mod.rs`，路由层继续通过 `admin::*`、`tweet_submit::submit_tweets`、`tweet_query::query_tweets` 和 `transfer::*` 调用。

## 管理台

管理台面向内部排查和常用运维操作：

- Overview 汇总账号、tweet v2 对象、转储队列和近期失败任务。
- Accounts 管理本地账号启停。
- X Users、Tweets、Media、Storage Objects 提供列表、详情、关联对象和原始 JSON。
- Transfers 提供状态筛选、失败重试、排队取消和处理中任务释放。

管理列表使用无状态 cursor 分页。cursor 中包含版本号、筛选条件和排序锚点；服务端每次按当前数据库状态查询。正文检索当前交给外部搜索引擎规划，管理台只做 ID、状态、类型和前缀类筛选。

## 数据库结构

数据库对象按 schema 分开：

- `tweet`：X 用户、帖子、媒体、关系表、字典和查询视图。
- `iam`：本地用户、SSO subject、授权和会话。
- `media`：媒体转储任务和对象存储记录。
- `audit`：后台操作审计。
- `vector`：预留给后续 embedding 和检索。

约定：

- 应用 SQL 必须写 schema 名，例如 `tweet.tweet`、`iam.users`。
- 不依赖 `search_path`。
- `public` 不放应用自己的表、视图、类型或函数；只保留扩展对象，例如 `public.citext`。
- 草稿 SQL 不是结构真相。改表时新增 migration。

## Tweet 写入

一次提交不一定是完整快照。请求里可能只有用户、只有帖子、只有媒体，或者三者混在一起。因此写入逻辑按单个对象处理，不假设同一批数据完整、有序。

核心表关系：

- 用户基础记录在 `tweet.twitter_user`。
- 用户资料版本写入 `tweet.user_snapshot`。
- 用户统计写入 `tweet.user_stats`。
- 帖子主体写入 `tweet.tweet`。
- 帖子统计写入 `tweet.tweet_stats`。
- 媒体主体写入 `tweet.media`。
- 媒体 URL、可用性和视频资源写入 `tweet.media_resource`。
- 帖子和媒体、提及、标签、符号的当前关系写入 `tweet.tweet_*_ref` 表。
- 最新数据读取优先使用 `tweet.v_latest_*` 视图。

写入规则：

- 旧值为空、新值非空时可以补齐。
- 旧值非空、新值为空时不要清掉旧值。
- 当前态表用 UPSERT，例如 `tweet.tweet_policy` 和 `tweet.tweet_community_note`。
- 版本表追加新版本，例如 `tweet.user_snapshot` 和 `tweet.media_resource`。
- 统计表先判断数据是否变化，再判断是否达到最小采样间隔。
- 单个对象失败不应让整批请求失败。

限制：

- 当前 schema 不保存 `article`、`card`、`faces`。
- 当前不保存帖子正文历史版本。
- `tweet_mention_ref.user_id` 没有物理外键，用于兼容只知道被提及用户 ID 的情况。
- `tweet_media_ref.media_id` 有物理外键，只能引用已存在媒体。
- 数据库不做全文检索、模糊搜索或 URL 搜索。

## 字符串字典

`tweet.string_dict` 保存常见短字符串，例如媒体可用性、敏感提示、语言和标签类型。

应用层使用进程内双向缓存：

- `(semantic, value) -> id`
- `id -> value`

新增低基数字符串时，优先复用 `string_dict`，不要为每个字段新建小枚举表。

## 媒体转储

媒体转储流程：

1. 公开接口提交 `media` 对象。
2. 写入新的 `tweet.media_resource` 版本。
3. 服务端选择一个可下载的源 URL。
4. 创建 `media.transfer_task`。
5. 后台 worker 下载源文件并上传到 S3-compatible 存储。
6. 上传完成后写入 `media.storage_object`，并把任务标记为 `completed`。

源 URL 选择规则：

- `photo` 使用 `tweet.media_resource.media_url`。
- `video` 和 `animated_gif` 优先使用最高码率的 `video/mp4` variant。
- 没有可用 mp4 时使用第一个可用 variant。
- variant 缺失时回退到 `media_url`。
- 找不到源 URL 时，提交结果中的 `media_transfer` 为 `source_unavailable`。

任务和对象：

- `media.transfer_task` 用 `(media_id, source_recorded_at)` 去重。
- 任务状态为 `pending`、`processing`、`completed`、`failed`。
- 失败任务在未达到 `max_attempts` 前会回到 `pending`。
- `media.storage_object` 保存对象 key、content type、长度、ETag 和 SHA-256。
- 对象 key 格式为 `{object_key_prefix}/{media_id}/{transfer_task_id}.{ext}`。

传输实现：

- 每个 worker 同时只处理一个源 URL；`worker_count` 控制 URL 级并发。
- worker 先填充首个 `chunk_size_mb` 缓冲区；如果首块内下载结束，直接 `PutObject` 上传。
- 大文件使用 S3 multipart upload。下载完成一个块后立即排队上传，`upload_parallelism` 控制并发上传数。
- `max_in_flight_parts` 控制单个 worker 可同时持有的块数量；内存上限约为 `worker_count * chunk_size_mb * max_in_flight_parts`。
- 源响应带 `Content-Length` 且 `Accept-Ranges: bytes` 时，首块之后可按 `download_parallelism` 并发 range 下载；否则继续顺序读取原响应。
- `connect_timeout_seconds`、`read_timeout_seconds` 和 `attempt_timeout_seconds` 为 0 时关闭对应超时；`task_stale_timeout_seconds` 单独控制卡在 `processing` 的任务回收。

当前限制：

- 只转储 tweet media。
- 用户头像、横幅和卡片资源还没接入。

## 本地检查

常用检查：

```bash
cargo test
cargo fmt -- --check
bun run type
bun run build
```

本地试媒体转储需要 PostgreSQL、S3-compatible 存储和以下环境变量：

```text
DATABASE_URL
APP_TOKEN
SESSION_HMAC_KEY
STORAGE_ACCESS_KEY
STORAGE_SECRET_KEY
```

检查最近转储任务：

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

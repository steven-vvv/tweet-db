# 架构说明

本文档记录当前实现的主要设计。数据库结构以 `server/migrations/` 为准；这里只解释为什么这么组织、写入时应遵守什么规则。

## 系统结构

项目分成两部分：

- `server/`：Rust Axum 服务，负责认证、API、数据库写入、查询和媒体转储。
- `webui/`：Vue MPA 前端，包含浏览视图、账号页和管理员管理台。

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
- `admin/`：legacy WebUI 管理接口，保留 `/internal/v1/admin/*` 的页面导向响应。
- `internal_api/v2/`：无头内部资源 API，按 identity、tweet、media、storage、transfer、search、audit、system 拆分。
- `tweet_submit/`：提交 API 合同、handler、批处理状态、结果汇总、转换和写入执行。
- `tweet_query/`：查询 API 合同、数据库读取类型、查询 fetch、decode 和 JSON build。
- `tweet_store/`：数据库读写封装，tweet 写入按 places、tweets、edits、policies、stats、community notes 拆分。
- `transfer/`：媒体转储队列、worker、下载、range 分片、上传和传输测试。
- `search/`：Tantivy 全文索引、分词器、索引队列、worker 和管理台搜索辅助。

对外入口集中在各领域的 `mod.rs`。路由层通过 `admin::v1` 保持现有 WebUI 接口，通过 `internal_api::v2` 暴露新的资源 API，通过 `tweet_submit::submit_tweets` 和 `tweet_query::query_tweets` 提供公开 tweet 同步能力。

## WebUI

当前 WebUI 使用 Vite 多入口构建：

- `/browse`：桌面优先的帖子浏览视图，使用 `/internal/v2/*` 资源 API。
- `/account`：登录、首次注册和会话状态入口。
- `/admin`：管理员管理台，继续承载现有管理工作流。

后端静态路由会按入口返回对应 HTML，根路径 `/` 重定向到 `/browse`。浏览视图当前遵循 v2 capability 规则，使用管理员会话访问。

## 管理台

当前 Vue 管理台继续使用 `/internal/v1/admin/*`。这组接口面向已有页面，返回 `items/nextCursor`、`summary/record/related` 等页面友好结构：

- Overview 汇总账号、tweet v2 对象、转储队列和近期失败任务。
- Accounts 管理本地账号启停。
- X Users、Tweets、Media、Storage Objects 提供列表、详情、关联对象和原始 JSON。
- X Users 和 Tweets 的 `q` 搜索使用 Tantivy；空 `q` 列表继续按数据库时间 cursor 分页。
- Transfers 提供状态筛选、失败重试、排队取消和处理中任务释放。

管理列表使用无状态 cursor 分页。cursor 中包含版本号、筛选条件和排序锚点；服务端每次按当前数据库状态查询。Tantivy 搜索路径使用 offset cursor，命中 ID 再回表读取管理台展示字段。

## 内部资源 API

`/internal/v2/*` 是无头内部 API，目标是把服务端能力和 WebUI 设计解耦。接口按数据模型和 CRUD 组织：

- `identity/*`：本地用户、会话、SSO 授权和账号启停。
- `twitter-users`、`tweets`、`media`：tweet schema 中的事实资源和子资源。
- `storage/objects`、`transfer/tasks`：对象存储记录、媒体转储任务和状态变更。
- `search/index-tasks`：Tantivy 索引队列查询和重新入队。
- `audit/events`、`system/summary`：审计记录和系统计数摘要。

v2 响应统一使用 `data`、`pagination`、`included`、`result`。资源详情只返回资源事实；关联数据通过 `include` 或子资源列表读取。初版 capability 全部映射到管理员会话，handler 已按 `IdentityRead`、`TweetRead`、`MediaTransferWrite` 等能力调用鉴权函数，后续可以逐步开放只读或局部写权限。

浏览视图使用 tweet v2 include 拉取时间线所需数据。`GET /internal/v2/tweets` 支持把作者、最新统计、媒体预览和最新媒体 resource hydrate 到列表项，避免时间线逐条请求关联资源。`legacyText` 和 `noteText` 输出前端友好的实体结构，保留 URL、mention、hashtag、symbol、media ref 和 style range。

## 数据库结构

数据库对象按 schema 分开：

- `tweet`：X 用户、帖子、媒体、关系表、字典和查询视图。
- `iam`：本地用户、SSO subject、授权和会话。
- `media`：媒体转储任务和对象存储记录。
- `search`：Tantivy 索引任务队列和索引 worker 状态。
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
- 全文检索由 Tantivy 子系统负责。PostgreSQL 保存事实数据、索引任务和回表展示数据。

## 全文搜索

搜索子系统使用 Tantivy 嵌入在服务进程内。索引目录由 `[search].index_dir` 配置，当前包含 `users-v1` 和 `tweets-v2` 两个索引。

索引内容：

- Users：`tweet.user_snapshot` 最新版本的 `user_name` 和 `display_name`。
- Tweets：`tweet.tweet` 的正文，优先使用 `note_text.body`，缺失时使用 `legacy_text.body`。
- Tweets 同时写入 author、relation、published_at、created_at、updated_at fast field，用于筛选和时间排序。

写入流程：

1. 提交接口调用数据库写入函数。
2. 用户主体、用户 snapshot 或 tweet 主体发生变化时，数据库函数在同一事务内刷新 `search.index_queue`。
3. 服务启动时比较 Tantivy 文档数和数据库事实表数量；存在差异时按批次把现有用户和 tweet 刷新进 `search.index_queue`。
4. search worker 领取 pending 任务，读取数据库最新态，更新 Tantivy 文档并 commit。
5. 管理台搜索从 Tantivy 取命中 ID，再按 ID 回表生成列表 JSON。

用户索引文档始终以 `tweet.twitter_user` 为主体；缺少 snapshot 的用户也会写入 ID 字段，后续 snapshot 到达后同一队列目标会刷新为带 handle/display name 的文档。

分词：

- 正文、display name 和 handle 的主要全文字段使用 `tantivy-jieba`。
- ID、handle 辅助字段使用 Tantivy ngram tokenizer，支持前缀和片段匹配。
- 查询框使用简化语法，默认 AND，支持引号短语，字段名和范围语法会被清洗为普通文本。

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

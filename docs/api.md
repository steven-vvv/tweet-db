# API 参考

本文档记录当前服务端提供的 HTTP 接口。接口实现以 `server/src/routes.rs` 为准。

通用约定：

- 业务接口返回 JSON。
- 认证使用会话 Cookie。
- 请求级错误通常返回：

```json
{
  "error": "..."
}
```

## Public API

公开接口统一使用 `/api/v1/...` 前缀。公开接口给浏览器脚本和外部调用方使用。

### `GET /api/v1/session`

查询当前请求的登录状态。未登录也返回 `200`。

已登录并完成注册：

```json
{
  "authenticated": true,
  "registered": true,
  "username": "demo_user",
  "expires_at": "2026-04-01T08:00:00Z",
  "account_url": null
}
```

未登录：

```json
{
  "authenticated": false,
  "registered": false,
  "username": null,
  "expires_at": null,
  "account_url": "http://127.0.0.1:3001/account"
}
```

字段说明：

- `authenticated`：是否有有效会话。
- `registered`：是否完成本地账号绑定。
- `username`：本地用户名；未登录或未绑定时为 `null`。
- `expires_at`：会话过期时间。
- `account_url`：需要用户继续登录或绑定账号时可跳转的地址。

### `POST /api/v1/tweet/submit`

批量提交用户、帖子和媒体对象。

认证要求：需要已登录、已完成注册并且是管理员。缺少会话或未完成注册返回 `401`，非管理员返回 `403`。

请求体根结构：

```json
{
  "users": [],
  "tweets": [],
  "media": []
}
```

请求规则：

- `users`、`tweets`、`media` 可省略，服务端按空数组处理。
- 对象 ID 和关联 ID 使用字符串形式的有符号 64 位整数。
- 同一根数组中重复 ID 只处理最后一个合法输入，前面的结果会标记为 `skipped`。
- 三个数组合计对象数不能超过 `ingest.max_items_per_batch`。

最小请求示例：

```json
{
  "tweets": [
    {
      "id": "1912345678901234567",
      "publishedAt": "2026-04-10T08:30:00Z",
      "authorId": "1234567890",
      "content": {
        "legacyText": {
          "text": "hello world",
          "entities": {}
        },
        "mediaIds": []
      },
      "conversation": {
        "conversationId": "1912345678901234567"
      }
    }
  ]
}
```

成功响应结构：

```json
{
  "summary": {
    "total": 1,
    "accepted": 1,
    "skipped": 0,
    "partial": 0,
    "failed": 0
  },
  "users": [],
  "tweets": [
    {
      "id": "1912345678901234567",
      "status": "accepted",
      "operations": [
        {
          "name": "tweet_author",
          "status": "accepted",
          "reason": "inserted_minimal"
        },
        {
          "name": "tweet",
          "status": "accepted",
          "reason": "inserted_or_filled"
        }
      ]
    }
  ],
  "media": []
}
```

响应说明：

- `summary` 按对象数量统计 `accepted`、`skipped`、`partial`、`failed`。
- `users`、`tweets`、`media` 与输入顺序对应。
- `accepted` 表示对象相关操作成功或无需重复写入。
- `skipped` 表示没有产生有效写入，例如重复输入、数据未变化或采样间隔未到。
- `partial` 表示部分操作成功、部分失败。
- `failed` 表示对象没有完成有效处理。
- 当提交写入新的 `media_resource` 版本时，服务端会尝试创建媒体转储任务。

### `POST /api/v1/tweet/query`

按对象 ID 批量查询当前保存的数据。

认证要求：需要已登录并完成注册。缺少会话或未完成注册返回 `401`。

请求体根结构：

```json
{
  "users": [{ "id": "1234567890" }],
  "tweets": [{ "id": "1912345678901234567" }],
  "media": [{ "id": "1712345678901234567" }]
}
```

请求规则：

- `users`、`tweets`、`media` 可省略，服务端按空数组处理。
- 三个数组合计对象数不能超过 `ingest.max_items_per_batch`。
- 单个选择器中的 `id` 无效时，不会导致整个请求失败；对应结果返回 `status = failed`。

成功响应结构：

```json
{
  "summary": {
    "total": 2,
    "found": 1,
    "missing": 1,
    "failed": 0
  },
  "users": [
    {
      "id": "1234567890",
      "status": "found",
      "data": {
        "id": "1234567890",
        "registeredAt": "2020-01-01T00:00:00Z",
        "profile": null,
        "pinnedTweetIds": [],
        "identity": null,
        "professional": null,
        "stats": null,
        "features": null
      }
    }
  ],
  "tweets": [
    {
      "id": "1912345678901234567",
      "status": "missing"
    }
  ],
  "media": []
}
```

响应说明：

- `summary` 按对象数量统计 `found`、`missing`、`failed`。
- `found` 表示查到对象，`data` 是当前规范化 JSON。
- `missing` 表示对象不存在。
- `failed` 表示选择器无效或服务端解码失败。
- `data` 字段使用提交接口的 `camelCase` 命名，不保证和原始提交内容逐字节一致。

## Internal API

内部接口分为两层：

- `/internal/v1/...`：当前 Vue WebUI 使用的 legacy 接口。
- `/internal/v2/...`：无头内部资源 API，按功能和数据模型设计，供后续 UIUX 独立接入。

### `GET /internal/v1/session`

返回当前会话的内部视图，用于 `/account` 页面判断登录、注册和管理员状态。

```json
{
  "authenticated": true,
  "registered": true,
  "is_admin": true,
  "disabled": false,
  "user_id": "0195f1df-0d69-7f7d-8c24-0c1af2d75001",
  "username": "demo_user",
  "subject_id": "0195f1de-fce0-7d91-b86d-7d7e8f2c1001",
  "authorization_id": "0195f1df-0730-7f69-9a92-cd8e4eb4d001",
  "expires_at": "2026-04-01T08:00:00Z"
}
```

### `POST /internal/v1/auth/registration`

首登后绑定本地用户名。

认证要求：需要有效会话，并且当前会话处于待注册状态。

```json
{
  "username": "demo_user"
}
```

### `DELETE /internal/v1/session`

注销当前会话，并尝试撤销对应 SSO 授权。未登录也允许调用。

```json
{
  "ok": true
}
```

## Internal API v2

v2 内部接口统一使用 `/internal/v2/...` 前缀。当前实现仍要求管理员会话；服务端按 capability 调用鉴权函数，后续可把部分 capability 映射给普通已注册用户或只读角色。

通用响应：

```json
{
  "data": [],
  "pagination": {
    "limit": 50,
    "nextCursor": null
  }
}
```

详情响应：

```json
{
  "data": {},
  "included": {}
}
```

动作响应：

```json
{
  "data": {},
  "result": {
    "ok": true
  }
}
```

通用约定：

- JSON 字段使用 `camelCase`。
- `BIGINT` ID 返回字符串，UUID 返回字符串，时间返回 RFC3339。
- 列表使用 `limit` 和 `cursor`，默认 50，上限 100。
- `include` 使用逗号分隔，例如 `include=latest-resource,transfer-tasks`。
- cursor 包含版本号、筛选条件和排序锚点；筛选条件变化时返回 `400`。

### Session

- `GET /internal/v2/me`

返回当前内部会话资源。可选 `include=capabilities`。

### Identity

- `GET /internal/v2/identity/users`
- `GET /internal/v2/identity/users/{user_id}`
- `PATCH /internal/v2/identity/users/{user_id}`
- `GET /internal/v2/identity/users/{user_id}/sessions`
- `GET /internal/v2/identity/users/{user_id}/sso-authorizations`

用户列表参数：

- `q`
- `status=all|active|disabled`
- `limit`
- `cursor`

用户详情支持 `include=sessions,sso-authorizations,audit-events`。

`PATCH` 请求体：

```json
{
  "disabled": true
}
```

### Tweet Domain

- `GET /internal/v2/twitter-users`
- `GET /internal/v2/twitter-users/{user_id}`
- `GET /internal/v2/twitter-users/{user_id}/snapshots`
- `GET /internal/v2/twitter-users/{user_id}/stats`
- `GET /internal/v2/tweets`
- `GET /internal/v2/tweets/{tweet_id}`
- `GET /internal/v2/tweets/{tweet_id}/media`
- `GET /internal/v2/media`
- `GET /internal/v2/media/{media_id}`
- `GET /internal/v2/media/{media_id}/resources`
- `GET /internal/v2/media/{media_id}/tweets`
- `GET /internal/v2/media/{media_id}/transfer-tasks`
- `POST /internal/v2/media/{media_id}/transfer-tasks`

tweet 列表参数：

- `q`
- `authorId`
- `relation=all|original|reply|quote|repost`
- `limit`
- `cursor`

媒体列表参数：

- `q`
- `mediaType=all|photo|video|animated_gif`
- `transferStatus=all|pending|processing|completed|failed|canceled`
- `limit`
- `cursor`

tweet 详情支持 `include=stats,edit,policy,community-note,media`。媒体详情支持 `include=latest-resource,transfer-tasks,tweets`。

### Storage And Transfer

- `GET /internal/v2/storage/objects`
- `GET /internal/v2/storage/objects/{object_id}`
- `GET /internal/v2/storage/objects/{object_id}/transfer-tasks`
- `POST /internal/v2/storage/objects/{object_id}/presigned-url`
- `GET /internal/v2/transfer/tasks`
- `GET /internal/v2/transfer/tasks/{task_id}`
- `POST /internal/v2/transfer/tasks/{task_id}/transitions`

存储对象列表参数：

- `q`
- `limit`
- `cursor`

转储任务列表参数：

- `q`
- `status=all|pending|processing|completed|failed|canceled`
- `limit`
- `cursor`

创建 presigned URL 返回 JSON：

```json
{
  "data": {
    "id": "0195f1df-0d69-7f7d-8c24-0c1af2d75001",
    "url": "https://...",
    "expiresAt": "2026-04-01T08:05:00Z"
  },
  "result": {
    "ok": true
  }
}
```

转储任务状态变更请求体：

```json
{
  "type": "retry"
}
```

`type` 可选 `retry`、`cancel`、`release`。

### Search, Audit, System

- `GET /internal/v2/search/index-tasks`
- `GET /internal/v2/search/index-tasks/{task_id}`
- `POST /internal/v2/search/index-tasks`
- `GET /internal/v2/audit/events`
- `GET /internal/v2/audit/events/{event_id}`
- `GET /internal/v2/system/summary`

搜索索引任务列表参数：

- `q`
- `status=all|pending|processing|completed|failed`
- `targetKind=all|user|tweet`
- `limit`
- `cursor`

重新入队索引任务请求体：

```json
{
  "targets": [
    {
      "targetKind": "tweet",
      "targetId": "1912345678901234567"
    }
  ]
}
```

审计列表参数：

- `actorUserId`
- `resourceType`
- `resourceId`
- `eventType`
- `limit`
- `cursor`

### `GET /internal/v1/admin/users`

查询用户列表。

认证要求：需要管理员会话。

查询参数：

- `q`
- `status=all|active|disabled`
- `limit`
- `cursor`

响应结构：

```json
{
  "items": [],
  "nextCursor": "..."
}
```

### `GET /internal/v1/admin/users/:user_id`

查询用户详情，包括摘要、原始用户记录、最近会话、授权和审计记录。

```json
{
  "summary": {},
  "record": {},
  "related": {}
}
```

### `POST /internal/v1/admin/users/:user_id/disable`

禁用其他用户，并清除该用户的会话。当前不允许管理员禁用自己。

### `POST /internal/v1/admin/users/:user_id/enable`

恢复已禁用用户。

### 管理列表分页约定

以下管理列表统一使用服务端无状态 cursor 分页：

- `limit` 默认 50，上限 100。
- `cursor` 内包含版本号、筛选条件和排序锚点。
- cursor 和当前筛选条件不一致时返回 `400`。
- 每次请求按当前数据库状态查询；数据变化时某一页数量可能浮动。

响应结构：

```json
{
  "items": [],
  "nextCursor": "..."
}
```

### `GET /internal/v1/admin/overview`

返回管理台总览，包括账号计数、tweet v2 对象计数、转储队列计数、存储配置摘要、最近帖子和最近失败任务。

### `GET /internal/v1/admin/twitter-users`

查询 X 用户列表。查询参数：

- `q`：用户 ID、handle、display name 搜索。传入 `q` 时使用 Tantivy 全文索引。
- `sort=relevance|time`：传入 `q` 时可选；默认 `relevance`。
- `limit`
- `cursor`

### `GET /internal/v1/admin/twitter-users/:user_id`

查询 X 用户详情，包括基础记录、最新 profile snapshot、最新 stats、最近帖子和相关媒体。

### `GET /internal/v1/admin/tweets`

查询帖子列表。查询参数：

- `q`：tweet ID、author ID 或帖子正文搜索。帖子正文索引使用 `note_text.body` 优先，缺失时使用 `legacy_text.body`。
- `authorId`
- `relation=all|original|reply|quote|repost`
- `sort=relevance|time`：传入 `q` 时可选；默认 `relevance`。
- `limit`
- `cursor`

### `GET /internal/v1/admin/tweets/:tweet_id`

查询帖子详情，包括原始记录、最新统计、策略、编辑信息、社区笔记、作者和媒体。

### `GET /internal/v1/admin/media`

查询媒体列表。查询参数：

- `q`：media ID、origin tweet ID 或 origin user ID 前缀。
- `mediaType=all|photo|video|animated_gif`
- `transferStatus=all|pending|processing|completed|failed|canceled`
- `limit`
- `cursor`

### `GET /internal/v1/admin/media/:media_id`

查询媒体详情，包括原始记录、最新 media resource、关联帖子和转储任务。

### `POST /internal/v1/admin/media/:media_id/transfer-tasks`

为媒体最新资源创建转储任务。已有同一 `(media_id, source_recorded_at)` 任务时返回 `created=false`。

### `GET /internal/v1/admin/storage-objects`

查询已转储存储对象。查询参数：

- `q`：object ID 或 object key 前缀。
- `limit`
- `cursor`

### `GET /internal/v1/admin/storage-objects/:object_id`

查询存储对象详情和关联转储任务。

### `GET /internal/v1/admin/storage-objects/:object_id/open`

管理员点击访问存储对象时生成短期 S3 presigned URL，并返回 `302` 跳转。

### `GET /internal/v1/admin/transfers/overview`

返回转储 worker 配置和任务状态计数。

### `GET /internal/v1/admin/transfers/tasks`

查询转储任务列表。查询参数：

- `q`：task ID、media ID 或 object key 前缀。
- `status=all|pending|processing|completed|failed|canceled`
- `limit`
- `cursor`

### `GET /internal/v1/admin/transfers/tasks/:task_id`

查询转储任务详情，包括原始任务记录、相关媒体和审计事件。

### `POST /internal/v1/admin/transfers/tasks/:task_id/retry`

将 `failed` 或 `canceled` 任务恢复为 `pending`，清除 claim 和错误信息，并写入审计事件。

### `POST /internal/v1/admin/transfers/tasks/:task_id/cancel`

将 `pending` 任务标记为 `canceled`，写入 `canceled_by_admin` 和审计事件。

### `POST /internal/v1/admin/transfers/tasks/:task_id/release`

将 `processing` 任务释放回 `pending`，清除 worker claim，并写入审计事件。

## Integrations

集成入口使用 `/integrations/...` 前缀。

### `GET /integrations/sso/callback`

SSO 登录回调。外部 SSO 完成授权后跳转到这里，服务端换取授权结果并创建本地会话。

当前行为：

- 成功后重定向到 `/account`。
- 上游返回错误时重定向到 `/account?error=<error>`。

### `POST /integrations/sso/webhooks/revocations`

SSO 授权撤销 webhook。服务端收到通知后更新授权状态，并删除相关会话。

请求体：

```json
{
  "authorization_id": "0195f1df-0730-7f69-9a92-cd8e4eb4d001"
}
```

成功响应：`204 No Content`。

## Removed Endpoints

以下旧公开接口已移除：

- `POST /api/v1/ingest/submissions`
- `POST /api/v1/posts/status/query`
- `POST /api/v1/auth/login-url`
- `POST /api/v1/auth/registration`
- `DELETE /api/v1/session`

以下旧内部接口已移除：

- `/internal/v1/admin/posts/*`
- `/internal/v1/admin/actors/*`

当前管理接口基于 tweet v2、media transfer 和 iam schema。

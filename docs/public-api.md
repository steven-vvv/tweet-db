# 对外 API 简明说明

本文档仅覆盖当前仓库已经实现、且面向前端或采集端直接调用的接口：

- 登录引导与本地注册
- 当前身份状态检查
- 帖子数据接入
- 帖子状态查询

未纳入本文档的接口包括内部专用 API，以及面向外部系统的 SSO 回调与 webhook。

说明：
所有公开业务接口统一使用 `/api/v1/...` 前缀。内部接口请参见 `docs/internal-api.md`，集成入口请参见 `docs/integrations.md`。

## 通用约定

- 服务默认监听地址为 `http://127.0.0.1:3001`。
- 认证方式为基于 Cookie 的会话认证，默认会话 Cookie 名称为 `tweet_db_sid`。
- 除 `GET /api/v1/session` 外，本文档中的帖子类接口均要求“已登录且已完成注册绑定”的会话。
- 错误响应统一为 JSON 结构：`{"error":"<message>"}`。
- 帖子类接口中的 `sourceKind` 会在服务端做去空格并转小写处理。

## 1. 当前身份状态检查

### `POST /api/v1/auth/login-url`

用于创建一次 SSO 登录跳转地址，并写入待完成登录的短期状态 Cookie。

认证要求：
无。

成功响应示例：

```json
{
  "login_url": "http://127.0.0.1:3000/sso/authorize?client_id=tweet-db&code_challenge=..."
}
```

### `POST /api/v1/auth/registration`

用于首登后完成本地用户名绑定。

认证要求：
需要有效会话，且当前会话处于“待注册绑定”状态。

请求体示例：

```json
{
  "username": "demo_user"
}
```

成功响应示例：

```json
{
  "user_id": "0195f1df-0d69-7f7d-8c24-0c1af2d75001",
  "username": "demo_user"
}
```

常见失败：

- `400`：用户名长度或字符集不合法，或当前会话并非待注册状态。
- `401`：`session required`。

### `GET /api/v1/session`

用于查询当前请求对应的登录状态、注册绑定状态，以及源站登录、注册、身份管理入口 URL。

认证要求：
无。未登录时也会返回 `200`，但 `authenticated=false`。

响应示例：

```json
{
  "authenticated": true,
  "registered": true,
  "username": "demo_user",
  "expires_at": "2026-04-01T08:00:00Z",
  "source_login_url": "http://127.0.0.1:3000/login",
  "source_register_url": "http://127.0.0.1:3000/register",
  "source_manage_url": "http://127.0.0.1:3000/account"
}
```

字段说明：

- `authenticated`：当前是否存在有效会话。
- `registered`：当前会话是否已经完成本地账号绑定。帖子类接口要求该值为 `true`。
- `username`：本地用户名。未完成注册或未登录时为 `null`。
- `expires_at`：会话过期时间，RFC 3339 格式。
- `source_login_url`：源站登录入口。
- `source_register_url`：源站注册入口。
- `source_manage_url`：源站身份管理入口。

未登录时返回：

```json
{
  "authenticated": false,
  "registered": false,
  "username": null,
  "expires_at": null,
  "source_login_url": "http://127.0.0.1:3000/login",
  "source_register_url": "http://127.0.0.1:3000/register",
  "source_manage_url": "http://127.0.0.1:3000/account"
}
```

### `DELETE /api/v1/session`

用于注销当前会话，并尝试撤销对应 SSO 授权。

认证要求：
无。未登录时也允许调用。

成功响应示例：

```json
{
  "ok": true
}
```

## 2. 帖子数据接入

### `POST /api/v1/ingest/submissions`

用于将一次采集批次中的帖子相关数据写入服务端，包括用户、帖子、媒体、抓包记录和时间线观测结果。

认证要求：
需要有效且已完成注册绑定的会话。否则返回 `401`。

请求体：

```json
{
  "sourceKind": "x",
  "clientContext": {},
  "captures": [],
  "users": [],
  "tweets": [],
  "media": [],
  "timelineEvents": []
}
```

关键字段说明：

- `sourceKind`：来源标识，必填，例如 `x`。
- `clientContext`：客户端上下文，原样入库。
- `users`：用户资料数组。
- `tweets`：帖子数组。
- `media`：媒体数组。
- `captures`：采集到的 XHR 报文数组。
- `timelineEvents`：时间线命中数组。

当前实现要点：

- 单批次数量限制由 `ingest.max_items_per_batch` 控制，默认配置为 `5000`。
- 数量统计口径为 `users + tweets + media + captures + timelineEvents` 的总和。
- 各数组元素中的关键标识为空时，服务端会跳过该条并在 `warnings` 中记录原因，不会中断整批处理。
- 媒体自动转存任务仅对 `video` 和 `animated_gif` 类型触发，且要求 `sourceUrl` 非空。

成功响应示例：

```json
{
  "submissionId": "0195f1f5-70d4-70f4-8b25-6842d5e16001",
  "status": "partial",
  "acceptedCount": 128,
  "transferJobsEnqueued": 6,
  "warnings": [
    "skipped media with empty id"
  ]
}
```

响应字段说明：

- `submissionId`：本次接入批次标识。
- `status`：`success` 或 `partial`。只要存在告警即为 `partial`。
- `acceptedCount`：成功写入或更新的记录数。
- `transferJobsEnqueued`：已入队的媒体转存任务数。
- `warnings`：非致命告警列表。

常见失败：

- `400`：`sourceKind is required`、`batch exceeds max_items_per_batch (...)`。
- `401`：`session required`、`registration must be completed`。

## 3. 帖子状态查询

### `POST /api/v1/posts/status/query`

用于按帖子 ID 批量查询服务端已保存的帖子、作者、媒体、时间线命中情况，以及媒体转存进度。

认证要求：
需要有效且已完成注册绑定的会话。否则返回 `401`。

请求体示例：

```json
{
  "sourceKind": "x",
  "postIds": ["190001", "190002"]
}
```

请求约束：

- `postIds` 不允许为空数组。
- `postIds` 至少要包含一个非空字符串。
- 返回结果顺序与请求中的 `postIds` 顺序一致。

成功响应示例：

```json
{
  "items": [
    {
      "sourceKind": "x",
      "postId": "190001",
      "found": true,
      "post": {
        "sourcePostId": "190001",
        "authorSourceActorId": "u_1",
        "conversationSourcePostId": "190001",
        "fullText": "hello world",
        "legacyFullText": "hello world",
        "noteText": null,
        "lang": "en",
        "sourceCreatedAtRaw": "Wed Apr 01 10:00:00 +0000 2026",
        "inReplyToSourcePostId": null,
        "inReplyToSourceActorId": null,
        "quotedSourcePostId": null,
        "retweetedSourcePostId": null,
        "viewCount": 10,
        "possiblySensitive": false,
        "favoriteCount": 1,
        "retweetCount": 0,
        "replyCount": 0,
        "quoteCount": 0,
        "bookmarkCount": 0,
        "mediaSourceIds": ["m_1"],
        "sourceLabel": "web"
      },
      "author": {
        "sourceActorId": "u_1",
        "name": "demo",
        "screenName": "demo_user",
        "description": "",
        "location": "",
        "avatarUrl": "https://example.com/avatar.jpg",
        "profileUrl": null,
        "bannerUrl": null,
        "verifiedType": null
      },
      "media": [
        {
          "sourceMediaId": "m_1",
          "mediaKey": "3_1",
          "sourcePostId": "190001",
          "mediaType": "video",
          "sourceUrl": "https://video.example.com/v.mp4",
          "thumbUrl": "https://img.example.com/t.jpg",
          "width": 1280,
          "height": 720,
          "altText": null,
          "allowDownload": true,
          "durationMs": 30000,
          "transferStatus": "succeeded",
          "storageObjectKey": "x/190001/m_1.mp4",
          "lastError": null
        }
      ],
      "missingMediaSourceIds": [],
      "timelineHits": [
        {
          "timelineKind": "bookmark",
          "timelineKey": "default",
          "observedAt": "2026-04-01T10:00:00Z"
        }
      ],
      "captureSummary": {
        "firstSubmissionId": "0195f1f5-70d4-70f4-8b25-6842d5e16001",
        "lastSubmissionId": "0195f1f5-70d4-70f4-8b25-6842d5e16001",
        "firstObservedAt": "2026-04-01T10:00:00Z",
        "lastObservedAt": "2026-04-01T10:00:00Z"
      },
      "transferSummary": {
        "pending": 0,
        "processing": 0,
        "succeeded": 1,
        "failed": 0
      }
    },
    {
      "sourceKind": "x",
      "postId": "190002",
      "found": false,
      "post": null,
      "author": null,
      "media": [],
      "missingMediaSourceIds": [],
      "timelineHits": [],
      "captureSummary": null,
      "transferSummary": {
        "pending": 0,
        "processing": 0,
        "succeeded": 0,
        "failed": 0
      }
    }
  ]
}
```

响应字段说明：

- `found`：该帖子是否已在库中命中。
- `post`：帖子主体信息；未命中时为 `null`。
- `author`：作者信息；帖子已命中但作者未命中时可能为 `null`。
- `media`：已命中的媒体及其转存状态。
- `missingMediaSourceIds`：帖子引用了但当前未查询到详情的媒体 ID。
- `timelineHits`：该帖子在哪些时间线观测中出现过。
- `captureSummary`：首次/最近一次观测该帖的批次与时间。
- `transferSummary`：媒体转存任务汇总。

`transferSummary` 状态口径：

- `pending`：包含 `pending` 与 `retryable` 两类任务。
- `processing`：转存处理中。
- `succeeded`：转存成功，通常可同时看到 `storageObjectKey`。
- `failed`：转存已终态失败。

常见失败：

- `400`：`sourceKind is required`、`postIds must not be empty`、`postIds must contain at least one non-empty value`。
- `401`：`session required`、`registration must be completed`。

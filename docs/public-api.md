# 对外 API 简明说明

本文档仅覆盖面向 Tampermonkey 脚本或其他外部采集端直接调用的公开接口。

当前公开 API 只有一个目标：为采集脚本提供中心化后端，用于检查本站登录状态，以及提交、检索帖子数据。认证流程本身不通过公开 API 编排；外部调用方只需在未登录或未完成本地绑定时，将用户引导到返回的 `account_url`，由本站 `/account` SPA 继续处理登录或注册。

所有公开业务接口统一使用 `/api/v1/...` 前缀。内部接口请参见 `docs/internal-api.md`，集成入口请参见 `docs/integrations.md`。

## 通用约定

- 服务默认监听地址为 `http://127.0.0.1:3001`。
- 认证方式为基于 Cookie 的会话认证，默认会话 Cookie 名称为 `tweet_db_sid`。
- 除 `GET /api/v1/session` 外，本文档中的帖子类接口均要求“已登录且已完成注册绑定”的会话。
- 错误响应统一为 JSON 结构：`{"error":"<message>"}`。
- 帖子类接口中的 `sourceKind` 会在服务端做去空格并转小写处理。

## 1. 当前身份状态检查

### `GET /api/v1/session`

用于查询当前请求对应的登录状态。外部脚本仅需依赖此接口判断是否可以继续调用帖子相关接口。

当用户未登录，或虽已登录但尚未完成本地用户名绑定时，响应中会返回 `account_url`，调用方应将用户引导到该地址，由本站 SPA 继续处理后续登录或注册流程。

认证要求：
无。未登录时也会返回 `200`。

已登录且已完成注册响应示例：

```json
{
  "authenticated": true,
  "registered": true,
  "username": "demo_user",
  "expires_at": "2026-04-01T08:00:00Z",
  "account_url": null
}
```

未登录响应示例：

```json
{
  "authenticated": false,
  "registered": false,
  "username": null,
  "expires_at": null,
  "account_url": "http://127.0.0.1:3001/account"
}
```

已登录但尚未完成注册响应示例：

```json
{
  "authenticated": true,
  "registered": false,
  "username": null,
  "expires_at": "2026-04-01T08:00:00Z",
  "account_url": "http://127.0.0.1:3001/account"
}
```

字段说明：

- `authenticated`：当前是否存在有效会话。
- `registered`：当前会话是否已经完成本地账号绑定。帖子类接口要求该值为 `true`。
- `username`：本地用户名。未完成注册或未登录时为 `null`。
- `expires_at`：会话过期时间，RFC 3339 格式。
- `account_url`：当调用方需要用户进入本站账号流程时可用的绝对地址；已登录且已完成注册时返回 `null`。

## 2. 帖子数据接入

### `POST /api/v1/ingest/submissions`

用于将一次采集批次中的帖子相关数据写入服务端。公开接入协议仅接受解析后的用户、帖子和媒体实体，不接受原始响应、客户端上下文或时间线命中数据。

认证要求：
需要有效且已完成注册绑定的会话。否则返回 `401`。

请求体：

```json
{
  "sourceKind": "x",
  "users": [],
  "tweets": [],
  "media": []
}
```

关键字段说明：

- `sourceKind`：来源标识，必填，例如 `x`。
- `users`：用户资料数组。
- `tweets`：帖子数组。
- `media`：媒体数组。

当前实现要点：

- 单批次数量限制由 `ingest.max_items_per_batch` 控制，默认配置为 `5000`。
- 数量统计口径为 `users + tweets + media` 的总和。
- 顶层请求体包含未声明字段时会直接返回 `400`。
- 各数组元素中的关键标识为空时，服务端会跳过该条并在 `warnings` 中记录原因，不会中断整批处理。
- 服务端会为帖子媒体以及作者头像、横幅注册统一媒体资产；转存任务覆盖图片、视频、GIF。
- 转存工作者会先下载固定块大小的数据；若文件小于该块大小则单对象上传，否则改为 multipart 上传。
- 作者资料在服务端内部按版本管理。只有资料字段发生变化时才会创建新版本；粉丝数等动态指标仍单独记录。
- 服务端会保留站内 submission 台账，但不会落库存储原始响应或客户端上下文。

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
- `transferJobsEnqueued`：本次提交中新建或重新激活的媒体转存任务数，包含帖子媒体与作者资料媒体。
- `warnings`：非致命告警列表。

常见失败：

- `400`：`sourceKind is required`、`batch exceeds max_items_per_batch (...)`。
- `401`：`session required`、`registration must be completed`。

## 3. 帖子状态查询

### `POST /api/v1/posts/status/query`

用于按帖子 ID 批量查询服务端已保存的帖子、作者、媒体和媒体转存进度。帖子中的互动计数字段取自该帖子最新一条指标观测记录；作者字段返回当前资料版本的快照。

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
      "media": [],
      "missingMediaSourceIds": [],
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

常见失败：

- `400`：`postIds must not be empty`、`postIds must contain at least one non-empty value`。
- `401`：`session required`、`registration must be completed`。

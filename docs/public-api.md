# Public API Reference

本文档记录当前对外开放的公开接口。

所有公开接口统一使用 `/api/v1/...` 前缀。内部接口请参见 [internal-api.md](/home/steven/code/tweet-db/docs/internal-api.md)。

当前公开接口面包含三类能力：

- 会话状态检查
- tweet v2 批量提交
- tweet v2 批量查询

公开接口统一使用会话 Cookie 鉴权。鉴权失败或请求级错误时，返回体统一为：

```json
{
  "error": "..."
}
```

## `GET /api/v1/session`

用于查询当前请求对应的登录状态。外部调用方仅应依赖此接口判断是否需要引导用户进入本站账号流程。

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

字段说明：

- `authenticated`：当前是否存在有效会话。
- `registered`：当前会话是否已经完成本地账号绑定。
- `username`：本地用户名；未登录或未完成绑定时为 `null`。
- `expires_at`：会话过期时间，RFC 3339 格式。
- `account_url`：需要用户继续完成账号流程时可跳转的绝对地址。

说明：

- 已登录但尚未完成注册绑定时，同样返回 `200`，此时 `authenticated = true`、`registered = false`，且 `account_url` 不为空。
- 该接口适合作为外部调用方的唯一公开登录态探针；不负责执行登录、注册或注销动作。

## `POST /api/v1/tweet/submit`

用于向 tweet v2 数据域批量提交用户、帖子与媒体对象。

认证要求：
需要已登录、已完成注册绑定且 `is_admin = true` 的会话。缺少会话或仍处于待绑定状态时返回 `401`，非管理员返回 `403`。

请求体根结构：

```json
{
  "users": [],
  "tweets": [],
  "media": []
}
```

请求约束：

- 三个根数组均可省略或传空，服务端按空数组处理。
- `users.id`、`tweets.id`、`media.id` 以及相关联的 tweet/user/media 引用 ID，均使用字符串形式的有符号 64 位整数。
- 同一根数组内若出现重复对象 ID，仅最后一个合法输入参与处理，前序重复项会在结果中标记为 `skipped`，原因为 `shadowed_by_duplicate_input`。
- 三个根数组合计对象数不得超过 `ingest.max_items_per_batch`，否则请求直接返回 `400`。

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
        },
        {
          "name": "tweet_media_ref",
          "status": "accepted",
          "reason": "replaced"
        }
      ]
    }
  ],
  "media": []
}
```

字段与语义：

- `summary`：按对象维度汇总 `accepted`、`skipped`、`partial`、`failed` 数量。
- `users`、`tweets`、`media`：分别返回与输入顺序对齐的对象级处理结果。
- `status = accepted`：该对象涉及的操作全部成功或被视为成功写入。
- `status = skipped`：该对象未产生有效写入，例如重复输入被遮蔽、数据未变化或统计采样间隔未到。
- `status = partial`：同一对象的部分子操作成功、部分失败。
- `status = failed`：该对象未能完成有效处理。
- `operations`：列出对象内部各子步骤，例如 `twitter_user`、`user_snapshot`、`tweet`、`tweet_stats`、`tweet_media_ref` 等。

处理特性：

- 这是对象级 best-effort 批处理接口。单个对象失败通常不会使整个批次返回非 `200`。
- 请求级错误仅在鉴权失败、JSON 反序列化失败或对象总数超限等场景下返回非 `200`。

## `POST /api/v1/tweet/query`

用于按对象 ID 批量查询 tweet v2 当前态数据。

认证要求：
需要已登录且已完成注册绑定的会话。缺少会话或仍处于待绑定状态时返回 `401`。

请求体根结构：

```json
{
  "users": [{ "id": "1234567890" }],
  "tweets": [{ "id": "1912345678901234567" }],
  "media": [{ "id": "1712345678901234567" }]
}
```

请求约束：

- 三个根数组均可省略或传空，服务端按空数组处理。
- 三个根数组合计对象数不得超过 `ingest.max_items_per_batch`，否则请求直接返回 `400`。
- 单个选择器中的 `id` 若不是字符串形式的有符号 64 位整数，不会导致整个批次失败，而是在对应结果项中返回 `status = failed`。

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

字段与语义：

- `summary`：按对象维度汇总 `found`、`missing`、`failed` 数量。
- `status = found`：查询到对象，`data` 返回当前规范化 JSON。
- `status = missing`：对象不存在。
- `status = failed`：该选择器本身无效，或服务端在解码对象时遇到错误；此时返回 `error` 字段。
- `data` 中的字段命名沿用提交接口使用的 `camelCase` 约定，返回值以当前数据库状态为准，不保证与原始提交载荷逐字节对称。

## Removed Endpoints

以下旧公开接口已移除，不再对外暴露：

- `POST /api/v1/ingest/submissions`
- `POST /api/v1/posts/status/query`

当前 tweet v2 公开写入/查询能力统一迁移为：

- `POST /api/v1/tweet/submit`
- `POST /api/v1/tweet/query`

另外，以下旧公开认证路由同样不再提供：

- `POST /api/v1/auth/login-url`
- `POST /api/v1/auth/registration`
- `DELETE /api/v1/session`

登录、首登绑定与注销能力分别通过 `/account` 页面和 `/internal/v1/...` 内部接口承载，不再作为公开业务 API 暴露。

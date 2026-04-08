# Public API Reference

本文档仅记录当前仍然对外开放的接口。

tweet v2 数据域正在进行数据库优先的大型重构。旧版公开 tweet ingest/query 接口已经移除；本阶段唯一保留的公开接口是会话状态检查。

所有公开接口统一使用 `/api/v1/...` 前缀。内部接口请参见 [internal-api.md](/home/steven/code/tweet-db/docs/internal-api.md)。

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

## Removed Endpoints

以下公开接口已在本轮 tweet v2 重构中移除：

- `POST /api/v1/ingest/submissions`
- `POST /api/v1/posts/status/query`

后续若重新开放 tweet 数据接入接口，将以新 schema 为基础重新定义契约，而不是兼容旧版请求体与响应形状。

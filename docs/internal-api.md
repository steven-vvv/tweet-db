# Internal API Reference

本文档记录仅供本站 SPA 或内部功能调用的接口，统一使用 `/internal/v1/...` 前缀。

这些接口不属于对外公开集成契约；外部脚本不应依赖它们。

## `GET /internal/v1/session`

返回当前会话的内部明细视图，适用于本站 `/account` 页面读取账号状态并决定显示登录、注册或已登录界面。

响应示例：

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

未登录时返回同结构，其中标识字段与 `expires_at` 为 `null`。

字段补充说明：

- `is_admin`：当前已登录本地用户是否具备管理控制台访问权限。
- `disabled`：当前本地用户是否处于禁用状态。被禁用用户的既有会话会在后续请求中被服务端清除，因此正常情况下该字段多为 `false`。

## `POST /internal/v1/auth/registration`

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

## `DELETE /internal/v1/session`

用于注销当前会话，并尝试撤销对应 SSO 授权。

认证要求：
无。未登录时也允许调用。

成功响应示例：

```json
{
  "ok": true
}
```

## 管理控制台 API

以下接口统一使用 `/internal/v1/admin/...` 前缀，仅供本站管理控制台 SPA 调用。

认证要求：
需要“已登录、已完成注册绑定、且 `is_admin = true`”的会话。非管理员返回 `403`。

通用约定：

- 列表接口统一支持 `limit`、`cursor` 与可选 `q`。
- 当前实现采用无服务器端状态的游标分页；响应统一为：

```json
{
  "items": [],
  "nextCursor": "..."
}
```

- 详情接口统一返回：

```json
{
  "summary": {},
  "record": {},
  "related": {}
}
```

- `record` 与 `related` 以数据库条目 JSON 为主，用于简化前端渲染与排障查看。

### 用户管理

- `GET /internal/v1/admin/users`
  - 查询参数：`q`、`status=all|active|disabled`、`limit`、`cursor`
- `GET /internal/v1/admin/users/:user_id`
- `POST /internal/v1/admin/users/:user_id/disable`
  - 用于禁用其他账户，并清除对应会话。
- `POST /internal/v1/admin/users/:user_id/enable`

说明：

- 当前产品不提供“设置管理员”接口；管理员仍需直接通过数据库维护 `users.is_admin`。
- 当前产品不允许自禁用当前管理员账户。

### 帖子与作者浏览

- `GET /internal/v1/admin/posts`
  - 查询参数：`q`、`sourceKind`、`limit`、`cursor`
- `GET /internal/v1/admin/posts/:source_kind/:source_post_id`
- `GET /internal/v1/admin/actors`
  - 查询参数：`q`、`sourceKind`、`limit`、`cursor`
- `GET /internal/v1/admin/actors/:source_kind/:source_actor_id`
- `GET /internal/v1/admin/media/:media_id`

说明：

- 帖子详情返回作者摘要、媒体摘要、最新指标快照与原始帖子数据库记录。
- 作者详情返回当前资料版本、最新指标、最近帖子以及头像/横幅媒体跳转标识。
- 媒体详情返回 `managed_media` 记录、来源媒体行、转储任务、转储尝试以及已绑定资源对象。

### 资源对象与签名访问

- `GET /internal/v1/admin/storage-objects`
  - 查询参数：`q`、`limit`、`cursor`
- `GET /internal/v1/admin/storage-objects/:object_id`
- `POST /internal/v1/admin/storage-objects/:object_id/sign`

签名接口成功响应示例：

```json
{
  "url": "https://storage.example.com/...",
  "expiresAt": "2026-04-02T10:30:00Z"
}
```

说明：

- 当前签名 URL 固定为 30 分钟有效。
- 资源详情返回对象元数据以及反向绑定的 `managed_media` 摘要。

### 转储系统状态管理

- `GET /internal/v1/admin/transfers/overview`
- `GET /internal/v1/admin/transfers/jobs`
  - 查询参数：`q`、`status`、`limit`、`cursor`
- `GET /internal/v1/admin/transfers/jobs/:job_id`
- `POST /internal/v1/admin/transfers/jobs/:job_id/requeue`

说明：

- `overview` 返回当前配置摘要、任务状态计数、最近失败任务与最近转储尝试。
- `requeue` 仅允许 `failed`、`retryable`、`succeeded` 三类任务回到 `pending`，不会对 `processing` 中任务做强制中断。

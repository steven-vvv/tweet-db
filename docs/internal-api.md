# Internal API Reference

本文档记录仅供本站 SPA 或内部功能调用的接口，统一使用 `/internal/v1/...` 前缀。

tweet v2 重构期间，内部接口面已经收缩为账号与管理员用户管理相关接口。旧的帖子、作者、媒体、对象存储与转存任务浏览接口已移除。

## Session And Registration

### `GET /internal/v1/session`

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

### `POST /internal/v1/auth/registration`

用于首登后完成本地用户名绑定。

认证要求：
需要有效会话，且当前会话处于待注册绑定状态。

请求体示例：

```json
{
  "username": "demo_user"
}
```

### `DELETE /internal/v1/session`

用于注销当前会话，并尝试撤销对应 SSO 授权。

认证要求：
无。未登录时也允许调用。

成功响应示例：

```json
{
  "ok": true
}
```

## Admin User Management

以下接口统一使用 `/internal/v1/admin/...` 前缀，仅供本站管理控制台 SPA 调用。

认证要求：
需要已登录、已完成注册绑定且 `is_admin = true` 的会话。非管理员返回 `403`。

### `GET /internal/v1/admin/users`

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

返回用户摘要、原始用户记录，以及最近的会话、授权与审计记录。

响应结构：

```json
{
  "summary": {},
  "record": {},
  "related": {}
}
```

### `POST /internal/v1/admin/users/:user_id/disable`

用于禁用其他账户，并清除对应会话。

当前实现不允许自禁用当前管理员账户。

### `POST /internal/v1/admin/users/:user_id/enable`

用于恢复已禁用账户。

## Removed Internal Endpoints

以下旧接口已在本轮重构中移除：

- `/internal/v1/admin/posts/*`
- `/internal/v1/admin/actors/*`
- `/internal/v1/admin/media/*`
- `/internal/v1/admin/storage-objects/*`
- `/internal/v1/admin/transfers/*`

这些能力后续若恢复，将基于新的 tweet v2 数据模型重新设计。

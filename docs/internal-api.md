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
  "user_id": "0195f1df-0d69-7f7d-8c24-0c1af2d75001",
  "username": "demo_user",
  "subject_id": "0195f1de-fce0-7d91-b86d-7d7e8f2c1001",
  "authorization_id": "0195f1df-0730-7f69-9a92-cd8e4eb4d001",
  "expires_at": "2026-04-01T08:00:00Z"
}
```

未登录时返回同结构，其中标识字段与 `expires_at` 为 `null`。

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

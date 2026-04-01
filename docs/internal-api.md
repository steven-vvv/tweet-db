# Internal API Reference

本文档记录仅供一方内部功能调用的接口，统一使用 `/internal/v1/...` 前缀。

## `GET /internal/v1/session`

返回当前会话的内部明细视图，适用于需要读取内部标识的自有功能，不应暴露给第三方客户端。

响应示例：

```json
{
  "authenticated": true,
  "registered": true,
  "user_id": "0195f1df-0d69-7f7d-8c24-0c1af2d75001",
  "username": "demo_user",
  "subject_id": "0195f1de-fce0-7d91-b86d-7d7e8f2c1001",
  "authorization_id": "0195f1df-0730-7f69-9a92-cd8e4eb4d001",
  "expires_at": "2026-04-01T08:00:00Z",
  "source_login_url": "http://127.0.0.1:3000/login",
  "source_register_url": "http://127.0.0.1:3000/register",
  "source_manage_url": "http://127.0.0.1:3000/account"
}
```

未登录时返回同结构，其中标识字段与 `expires_at` 为 `null`。

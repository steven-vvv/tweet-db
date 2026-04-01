# Integrations Reference

本文档记录面向外部系统的集成入口，统一使用 `/integrations/...` 前缀。

## `GET /integrations/sso/callback`

SSO 登录回调入口。外部 SSO 提供方完成授权后重定向到该路径，服务端据此换取授权结果并建立本地会话。

当前行为：

- 成功且已完成本地绑定时，重定向到 `/account`
- 成功但尚未完成本地绑定时，重定向到 `/register`
- 上游返回错误时，重定向到 `/login?error=<error>`

## `POST /integrations/sso/webhooks/revocations`

SSO 授权吊销 webhook。外部 SSO 提供方通知授权已撤销后，服务端会更新授权状态并删除关联会话。

请求体示例：

```json
{
  "authorization_id": "0195f1df-0730-7f69-9a92-cd8e4eb4d001"
}
```

成功响应：

- `204 No Content`

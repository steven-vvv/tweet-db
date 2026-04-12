# Tweet V2 Refactor TODO

本文件记录本轮为了完成 tweet v2 数据库对齐而明确延期或移除的事项。

## Deferred

- 基于新 schema 重建管理员帖子、作者、媒体浏览接口与对应 Web UI。
- 重新接入 `media` schema 下的 transfer/storage 子系统，并确定新 tweet `media` 主表之外的本地资产建模方式。
- 为用户头像、横幅等非 tweet media 资源设计独立资产注册与转存方案。
- 设计 `vector` schema 下的 embedding、索引与召回结构。
- 扩展 `audit` schema，使其覆盖 tweet 管理动作而不止用户管理审计。
- 为 tweet v2 仓储层补充真实数据库集成测试夹具。

## Removed In This Pass

- 旧版 `actors` / `posts` / `post_media_sources` / `managed_media` 表族及其迁移脚本。
- 旧版公开 ingest/status 查询接口；现已由 `/api/v1/tweet/submit` 与 `/api/v1/tweet/query` 取代。
- 旧版管理员 posts/actors/media/storage/transfers 页面与路由。

## Re-entry Constraints

- 后续扩展公开接口时，不回退到旧版 JSON 协议形状。
- 后续恢复转存系统时，不直接复用已删除的 `managed_media` 设计。
- 新增能力应优先复用 `string_dict` 运行时缓存与 tweet v2 维表/关系表模型。
- 后续新增数据库对象时，继续显式放入所属 schema，不回流到 `public`。

# Tweet V2 Schema Notes

## 状态

- 当前 [tweet-v2-schema-draft.sql](/home/steven/code/tweet-db/docs/tweet-v2-schema-draft.sql) 已升级为数据库 bootstrap 草案，而不再只是单一 `public` schema 下的 tweet 表草稿。
- 当前表关系、写入策略与版本管理约定见 [tweet-v2-schema-design.md](/home/steven/code/tweet-db/docs/tweet-v2-schema-design.md)。
- 本轮 schema 分层已定稿，后续默认不再回到“全部对象落在 public”的组织方式。
- 可执行 migration 已按子系统拆成多文件顺序执行；设计草案文件继续保留为单文件总览。

## 已锁定的 schema 划分

- `tweet`：
  tweet v2 核心数据域，承载字典、复合类型、维表、主表、关系表与便利视图。
- `iam`：
  本站用户、SSO subject、authorization、session。
- `audit`：
  审计与日志域；当前已迁入 `audit_events`。
- `media`：
  预留给未来媒体资产、OSS 文件、转储工作者；当前仅保留 schema 占位。
- `vector`：
  预留给未来 embedding、向量索引、召回；当前仅保留 schema 占位。

## 实现侧约定

- 应用层 SQL 一律显式使用 schema-qualified 名称，例如 `tweet.tweet`、`iam.users`、`audit.audit_events`。
- 不依赖 `search_path`，也不在配置系统、连接封装或 `DATABASE_URL` 上引入“默认 schema”概念。
- `public` 不承载应用自有表、视图、类型或函数；仅允许保留扩展对象，例如 `public.citext`。
- `tweet.string_dict` 继续采用进程内双向缓存：
  `(semantic, value) -> id` 与 `id -> value`。
- `tweet` 域新增媒体敏感性提示语义时，继续复用 `tweet.string_dict`，
  不引入独立“自动枚举表”。
- 插入时间统一使用 `created_at` / `updated_at` 命名；
  `recorded_at` 仅保留给快照/时序表表示业务采样时间。

## 当前已锁定的 tweet 侧方向

- 字符串搜索外置：
  数据库不承担全文检索、模糊匹配或 URL 搜索职责。
- 富文本内联保留：
  `tweet.annotated_text` 继续保留在正文、长文、bio 与社区附注等核心场景。
- 关系表只做反查优化：
  `tweet.tweet_media_ref`、`tweet.tweet_mention_ref`、`tweet.tweet_hashtag_ref`、`tweet.tweet_symbol_ref` 主要服务反向查询或顺序恢复。
- 选择性外键策略保持不变：
  直接卫星表与关系表保留物理外键；乱序、缺失、跨批次引用继续仅存 ID。

## 非目标

- 不在数据库中引入 `tsvector`、`GIN`、`trgm` 或其他全文检索结构。
- 不为 `media`、`vector` schema 提前补充半成品业务表。
- 不使用 schema 配置开关去隐藏数据库子系统边界。

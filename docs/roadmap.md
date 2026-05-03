# 后续计划

本文档记录当前已经完成的功能、还没做的功能，以及后续开发时要遵守的限制。

## 已完成

- 账号登录、SSO 回调、本地用户名绑定和注销。
- 登录用户和管理员两类角色。
- 管理员用户列表、用户详情、禁用用户、恢复用户。
- 管理员总览、X 用户列表与详情、帖子列表与详情、媒体列表与详情。
- 管理员查看存储对象、点击生成短期访问链接。
- 管理员查看转储队列、重试失败任务、取消排队任务、释放处理中任务。
- 公开接口：
  - `GET /api/v1/session`
  - `POST /api/v1/tweet/submit`
  - `POST /api/v1/tweet/query`
- tweet v2 表结构和写入函数。
- 用户、帖子、媒体的批量写入和查询。
- x-monkey 请求/响应契约夹具。
- tweet media 转储任务表、后台 worker 和对象存储写入。
- HTTP/HTTPS 服务模式配置。
- Tantivy 搜索子系统、`tweets-v5` 和 `users-v2` 版本化索引、索引队列、启动回填和搜索接口。
- 管理台和 v2 tweet/user 搜索使用 Tantivy 命中，再从 PostgreSQL 回源展示。

## 还没做

- 登录用户查看帖子列表和帖子详情。
- 登录用户查看已转储媒体。
- 帖子可见性字段和后台修改入口。
- 用户头像、横幅等非 tweet media 资源转储。
- `vector` schema 下的 embedding、索引和召回。
- 帖子管理动作的审计记录。
- tweet v2 仓储层真实数据库集成测试。

## 下一步

下一阶段建议顺序如下：

1. 为 tweet v2 仓储层补真实数据库测试，覆盖用户、帖子、媒体、关系表和重复提交。
2. 补普通登录用户浏览页：帖子列表、帖子详情、可访问媒体。
3. 设计帖子可见性字段、管理动作和审计事件。
4. 为 Tantivy 搜索补运维入口：手动全量重建、索引版本切换观测、异常任务批量重试。

当前搜索职责归属 Tantivy。PostgreSQL 保存事实数据、索引队列和回源展示数据；后续新增搜索能力优先扩展 Tantivy 索引版本。

## 产品方向

项目要支持四类使用者：

- x-monkey 脚本：检查登录状态，提交帖子数据，查询同步结果。
- 登录用户：浏览已保存帖子和可看的媒体。
- 管理员：管理用户、帖子、媒体和转储任务。
- 后台 worker：把媒体从原始 URL 转储到对象存储。

当前重点已经从旧版 posts/actors/media 表切到 tweet v2 表。后续功能应继续基于 tweet v2 模型实现。

## 开发约束

- 不恢复旧版 `/api/v1/ingest/submissions` 和 `/api/v1/posts/status/query`。
- 不恢复旧版 `actors`、`posts`、`post_media_sources`、`managed_media` 表设计。
- 新增数据库对象必须放到明确的 schema，不能放回 `public`。
- 新增低基数字符串优先复用 `tweet.string_dict`。
- 新增公开接口时继续使用当前 tweet v2 JSON 形状。
- 新增搜索业务能力优先落到 Tantivy；PostgreSQL 侧只保留事实读取、结构化分页和索引队列所需索引。

## 迁移收尾检查

每次继续做 tweet v2 相关功能时，先检查这几件事：

- 新代码是否还在使用旧 posts/actors/media 命名。
- 新接口是否直接读 `tweet.*`、`media.*`、`iam.*`，而不是重新引入旧表概念。
- 新页面是否按当前表结构展示数据，不依赖已删除的旧 admin posts/media 接口。
- 新测试是否覆盖重复提交、缺失关联对象、媒体转储入队、查询当前态。
- 新文档是否只引用 `docs/api.md`、`docs/architecture.md`、`docs/roadmap.md`。

# 后续计划

本文档记录当前已经完成的功能、还没做的功能，以及后续开发时要遵守的限制。

## 已完成

- 账号登录、SSO 回调、本地用户名绑定和注销。
- 登录用户和管理员两类角色。
- 管理员用户列表、用户详情、禁用用户、恢复用户。
- 公开接口：
  - `GET /api/v1/session`
  - `POST /api/v1/tweet/submit`
  - `POST /api/v1/tweet/query`
- tweet v2 表结构和写入函数。
- 用户、帖子、媒体的批量写入和查询。
- x-monkey 请求/响应契约夹具。
- tweet media 转储任务表、后台 worker 和对象存储写入。
- HTTP/HTTPS 服务模式配置。

## 还没做

- 登录用户查看帖子列表和帖子详情。
- 登录用户查看已转储媒体。
- 管理员查看和管理帖子。
- 管理员查看和管理媒体资源。
- 管理员查看和重试转储任务。
- 帖子可见性字段和后台修改入口。
- 用户头像、横幅等非 tweet media 资源转储。
- `vector` schema 下的 embedding、索引和召回。
- tweet 管理动作的审计记录。
- tweet v2 仓储层真实数据库集成测试。

## 下一步

先做数据模型迁移收尾，再补新页面。建议顺序如下：

1. 提交当前文档整理，先让 README 和 docs 只指向当前实现。
2. 为 tweet v2 仓储层补真实数据库测试，覆盖用户、帖子、媒体、关系表和重复提交。
3. 基于 tweet v2 表补只读接口：帖子列表、帖子详情、媒体详情、转储任务列表。
4. 前端先接管理员只读页面，再做普通用户浏览页面。
5. 最后补管理动作，例如帖子可见性、媒体重试、转储任务重试和审计记录。

这一步不要急着改 schema。先把当前 schema 的读取、测试和页面补齐，再判断还缺哪些字段。

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

## 迁移收尾检查

每次继续做 tweet v2 相关功能时，先检查这几件事：

- 新代码是否还在使用旧 posts/actors/media 命名。
- 新接口是否直接读 `tweet.*`、`media.*`、`iam.*`，而不是重新引入旧表概念。
- 新页面是否按当前表结构展示数据，不依赖已删除的旧 admin posts/media 接口。
- 新测试是否覆盖重复提交、缺失关联对象、媒体转储入队、查询当前态。
- 新文档是否只引用 `docs/api.md`、`docs/architecture.md`、`docs/roadmap.md`。

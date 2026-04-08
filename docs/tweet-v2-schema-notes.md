# Tweet V2 Schema Notes

## 状态

- 当前 [tweet-v2-schema-draft.sql](/home/steven/code/tweet-db/docs/tweet-v2-schema-draft.sql) 可视为现阶段冻结候选版本。
- 后续默认不再继续扩展 DDL，除非出现明确的结构性矛盾、实现阻塞或上游模型再次发生实质变化。

## 当前已锁定的方向

- 字符串搜索外置：
  数据库不承担全文检索、模糊匹配或 URL 搜索职责。后续若接入外部搜索引擎，当前参考方向为 Tantivy，但本阶段不回写任何 DDL 变更。
- 富文本内联保留：
  `annotated_text` 继续保留在正文、长文、bio、社区附注等核心场景，正向读取不依赖维表 JOIN 组装文本。
- 关系表只做反查优化：
  `tweet_media_ref`、`tweet_mention_ref`、`tweet_hashtag_ref`、`tweet_symbol_ref` 主要服务反向查询或顺序恢复，不承担正文主读取路径。
- 命名字典只做归一化：
  `string_dict` 仅负责受控短字符串归一化，不承担搜索系统角色。

## 实现侧约定

- 建议服务端维护 `string_dict` 的本地缓存：
  `(semantic, value) -> id` 与 `id -> value` 两个方向均建议缓存，以减少高频查表、重复插入争用和读取侧反解开销。
- `tweet_mention_ref`、`tweet_hashtag_ref`、`tweet_symbol_ref` 的数据来源应限定为 tweet 正文相关实体，写入时由应用层负责去重。
- `user_professional.category_ids`、`annotated_text.hashtags`、`annotated_text.symbols` 的装配与反解由应用层承担，数据库不为复合类型内部元素建立物理外键。

## 非目标

- 不在数据库中引入 `tsvector`、`GIN`、`trgm` 或其他全文检索结构。
- 不新增 URL 专用反查表或 URL 裁剪字段。
- 不为了搜索系统预留额外同步列、镜像文本列或派生搜索索引字段。

## 后续若需重新打开 DDL 的触发条件

- 上游 `tweet-schema.ts` 再次删除、重命名或重构核心结构。
- 当前维表或关系表在写入链路中证明存在不可接受的复杂度或一致性问题。
- 外部搜索引擎接入后，确实需要数据库补充稳定的同步元数据，而该元数据无法在应用层自然生成。

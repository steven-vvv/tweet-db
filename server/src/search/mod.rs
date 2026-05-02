use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tantivy::{
    Index, IndexReader, IndexWriter, Order, TantivyDocument, Term, doc,
    query::{AllQuery, BooleanQuery, Occur, Query, QueryParser, TermQuery},
    schema::{
        Field, IndexRecordOption, NumericOptions, STORED, Schema, TextFieldIndexing, TextOptions,
        Value,
    },
    tokenizer::{LowerCaser, RemoveLongFilter, TextAnalyzer},
};
use time::OffsetDateTime;

use crate::{
    config::{SearchSection, Settings},
    error::{AppError, AppResult},
};

mod queue;
mod worker;

pub use queue::{EnqueueStatus, IndexTarget, IndexTargetKind, enqueue_targets};
pub use worker::start_workers;

const TWEET_INDEX_DIR: &str = "tweets-v3";
const JIEBA_TOKENIZER: &str = "jieba";
const MAX_QUERY_CHARS: usize = 256;

#[derive(Clone)]
pub struct SearchState {
    inner: Arc<SearchRuntime>,
}

struct SearchRuntime {
    tweets: SearchIndex<TweetFields>,
}

struct SearchIndex<F> {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    fields: F,
}

#[derive(Clone, Copy)]
struct TweetFields {
    id: Field,
    author_id: Field,
    body: Field,
    relation: Field,
    published_at: Field,
    created_at: Field,
    updated_at: Field,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSort {
    Relevance,
    Time,
}

impl SearchSort {
    pub fn parse(raw: Option<&str>, has_query: bool) -> AppResult<Self> {
        match raw.unwrap_or(if has_query { "relevance" } else { "time" }) {
            "relevance" => Ok(Self::Relevance),
            "time" => Ok(Self::Time),
            _ => Err(AppError::bad_request("sort must be one of relevance, time")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TweetSearchSort {
    Relevance,
    PublishedAt,
    CreatedAt,
    UpdatedAt,
}

impl TweetSearchSort {
    pub fn parse(raw: Option<&str>) -> AppResult<Self> {
        match raw.unwrap_or("relevance").trim() {
            "relevance" => Ok(Self::Relevance),
            "publishedAt" => Ok(Self::PublishedAt),
            "createdAt" => Ok(Self::CreatedAt),
            "updatedAt" => Ok(Self::UpdatedAt),
            _ => Err(AppError::bad_request(
                "sort must be one of relevance, publishedAt, createdAt, updatedAt",
            )),
        }
    }
}

impl From<SearchSort> for TweetSearchSort {
    fn from(value: SearchSort) -> Self {
        match value {
            SearchSort::Relevance => Self::Relevance,
            SearchSort::Time => Self::PublishedAt,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub id: i64,
    pub score: Option<f32>,
    pub sort_time: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct TweetSearchFilters {
    pub tweet_ids: Vec<i64>,
    pub author_ids: Vec<i64>,
    pub author_id: Option<i64>,
    pub relation: Option<String>,
}

pub fn build_state(settings: &Settings) -> AppResult<Option<SearchState>> {
    if !settings.config.search.enabled {
        tracing::info!("search subsystem is disabled by config");
        return Ok(None);
    }

    let tweets = open_tweet_index(&settings.config.search)?;
    tracing::info!(
        index_dir = %settings.config.search.index_dir.display(),
        "initialized search indexes"
    );

    Ok(Some(SearchState {
        inner: Arc::new(SearchRuntime { tweets }),
    }))
}

impl SearchState {
    fn tweet_document_count(&self) -> AppResult<u64> {
        self.inner.tweets.document_count()
    }

    pub async fn search_tweets(
        &self,
        raw_query: Option<&str>,
        filters: &TweetSearchFilters,
        sort: TweetSearchSort,
        limit: usize,
        offset: usize,
    ) -> AppResult<Vec<SearchHit>> {
        let index = &self.inner.tweets;
        index.reload()?;
        let query = build_tweet_query(index, raw_query, filters)?;
        collect_tweet_hits(index, &*query, sort, index.fields.id, limit, offset)
    }

    async fn index_task(&self, pool: &PgPool, task: &queue::ClaimedIndexTask) -> AppResult<()> {
        match task.parsed_kind()? {
            IndexTargetKind::User => Ok(()),
            IndexTargetKind::Tweet => self.index_tweets(pool, &[task.target_id]).await,
        }
    }

    async fn index_tasks(&self, pool: &PgPool, tasks: &[queue::ClaimedIndexTask]) -> AppResult<()> {
        let mut tweet_ids = Vec::new();
        for task in tasks {
            match task.parsed_kind()? {
                IndexTargetKind::User => {}
                IndexTargetKind::Tweet => tweet_ids.push(task.target_id),
            }
        }

        self.index_tweets(pool, &tweet_ids).await
    }

    async fn index_tweets(&self, pool: &PgPool, tweet_ids: &[i64]) -> AppResult<()> {
        if tweet_ids.is_empty() {
            return Ok(());
        }

        let records = fetch_tweet_documents(pool, tweet_ids).await?;
        let index = &self.inner.tweets;
        let mut writer = index.lock_writer()?;
        for tweet_id in tweet_ids {
            writer.delete_term(Term::from_field_i64(index.fields.id, *tweet_id));
            if let Some(record) = records.get(tweet_id) {
                writer.add_document(doc!(
                    index.fields.id => record.id,
                    index.fields.author_id => record.author_id,
                    index.fields.body => record.body.as_str(),
                    index.fields.relation => record.relation.as_str(),
                    index.fields.published_at => record.published_at.unix_timestamp(),
                    index.fields.created_at => record.created_at.unix_timestamp(),
                    index.fields.updated_at => record.updated_at.unix_timestamp(),
                ))?;
            }
        }
        writer.commit()?;
        drop(writer);
        index.reload()?;
        Ok(())
    }
}

impl<F> SearchIndex<F> {
    fn lock_writer(&self) -> AppResult<std::sync::MutexGuard<'_, IndexWriter>> {
        self.writer
            .lock()
            .map_err(|_| AppError::search("index writer lock was poisoned"))
    }

    fn reload(&self) -> AppResult<()> {
        self.reader.reload().map_err(search_error)
    }

    fn document_count(&self) -> AppResult<u64> {
        self.reload()?;
        Ok(self.reader.searcher().num_docs())
    }
}

pub async fn enqueue_startup_backfill(
    db: &PgPool,
    search: &SearchState,
    batch_size: usize,
) -> AppResult<()> {
    let tweet_db_count = count_tweets(db).await?;
    let tweet_index_count = search.tweet_document_count()?;
    if should_backfill_index(tweet_index_count, tweet_db_count) {
        let queued = enqueue_existing_targets(db, IndexTargetKind::Tweet, batch_size).await?;
        tracing::info!(
            db_count = tweet_db_count,
            index_count = tweet_index_count,
            queued,
            "queued startup tweet search backfill"
        );
    }

    Ok(())
}

async fn count_tweets(db: &PgPool) -> AppResult<u64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM tweet.tweet
        "#,
    )
    .fetch_one(db)
    .await?;

    Ok(u64::try_from(count).unwrap_or(0))
}

fn should_backfill_index(index_count: u64, db_count: u64) -> bool {
    index_count != db_count
}

async fn enqueue_existing_targets(
    db: &PgPool,
    kind: IndexTargetKind,
    batch_size: usize,
) -> AppResult<u64> {
    let sql = match kind {
        IndexTargetKind::User => {
            "SELECT id FROM tweet.twitter_user WHERE ($1::BIGINT IS NULL OR id > $1) ORDER BY id LIMIT $2"
        }
        IndexTargetKind::Tweet => {
            "SELECT id FROM tweet.tweet WHERE ($1::BIGINT IS NULL OR id > $1) ORDER BY id LIMIT $2"
        }
    };
    let mut last_id = None;
    let mut queued = 0u64;
    let limit = i64::try_from(batch_size).unwrap_or(i64::MAX);

    loop {
        let ids = sqlx::query_scalar::<_, i64>(sql)
            .bind(last_id)
            .bind(limit)
            .fetch_all(db)
            .await?;
        if ids.is_empty() {
            break;
        }

        let targets = ids
            .iter()
            .map(|id| IndexTarget { kind, id: *id })
            .collect::<Vec<_>>();
        queue::enqueue_targets(db, &targets).await?;
        queued = queued.saturating_add(u64::try_from(targets.len()).unwrap_or(u64::MAX));
        last_id = ids.last().copied();
    }

    Ok(queued)
}

fn open_tweet_index(config: &SearchSection) -> AppResult<SearchIndex<TweetFields>> {
    let mut builder = Schema::builder();
    let numeric = NumericOptions::default()
        .set_indexed()
        .set_fast()
        .set_stored();
    let id = builder.add_i64_field("id", numeric.clone());
    let author_id = builder.add_i64_field("author_id", numeric.clone());
    let body = builder.add_text_field("body", jieba_text_options());
    let relation = builder.add_text_field("relation", STORED | tantivy::schema::STRING);
    let published_at = builder.add_i64_field("published_at", numeric.clone());
    let created_at = builder.add_i64_field("created_at", numeric.clone());
    let updated_at = builder.add_i64_field("updated_at", numeric);
    let schema = builder.build();
    let index = open_index(
        &config.index_dir.join(TWEET_INDEX_DIR),
        schema,
        config.writer_memory_mb,
    )?;
    register_tokenizers(&index)?;
    let reader = index.reader()?;
    let writer = index.writer_with_num_threads(1, memory_bytes(config.writer_memory_mb))?;

    Ok(SearchIndex {
        index,
        reader,
        writer: Mutex::new(writer),
        fields: TweetFields {
            id,
            author_id,
            body,
            relation,
            published_at,
            created_at,
            updated_at,
        },
    })
}

fn open_index(path: &Path, schema: Schema, _writer_memory_mb: usize) -> AppResult<Index> {
    fs::create_dir_all(path)?;
    if path.join("meta.json").is_file() {
        Index::open_in_dir(path).map_err(search_error)
    } else {
        Index::create_in_dir(path, schema).map_err(search_error)
    }
}

fn register_tokenizers(index: &Index) -> AppResult<()> {
    let jieba = TextAnalyzer::builder(tantivy_jieba::JiebaTokenizer::new())
        .filter(RemoveLongFilter::limit(80))
        .filter(LowerCaser)
        .build();
    index.tokenizers().register(JIEBA_TOKENIZER, jieba);
    Ok(())
}

fn jieba_text_options() -> TextOptions {
    TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(JIEBA_TOKENIZER)
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored()
}

fn build_tweet_query(
    index: &SearchIndex<TweetFields>,
    raw_query: Option<&str>,
    filters: &TweetSearchFilters,
) -> AppResult<Box<dyn Query>> {
    let mut parts = Vec::<(Occur, Box<dyn Query>)>::new();
    if let Some(query_text) = normalized_query(raw_query) {
        let mut parser = QueryParser::for_index(&index.index, vec![index.fields.body]);
        parser.set_conjunction_by_default();
        parser.set_field_boost(index.fields.body, 2.0);
        parts.push((
            Occur::Must,
            parser.parse_query(&query_text).map_err(search_error)?,
        ));
    }

    push_i64_filter(&mut parts, index.fields.id, &filters.tweet_ids);

    let author_ids = filters
        .author_id
        .into_iter()
        .chain(filters.author_ids.iter().copied())
        .collect::<Vec<_>>();
    push_i64_filter(&mut parts, index.fields.author_id, &author_ids);

    if let Some(relation) = filters.relation.as_deref().filter(|value| *value != "all") {
        parts.push((
            Occur::Must,
            Box::new(TermQuery::new(
                Term::from_field_text(index.fields.relation, relation),
                IndexRecordOption::Basic,
            )),
        ));
    }

    if parts.is_empty() {
        Ok(Box::new(AllQuery))
    } else if parts.len() == 1 {
        Ok(parts.remove(0).1)
    } else {
        Ok(Box::new(BooleanQuery::new(parts)))
    }
}

fn push_i64_filter(parts: &mut Vec<(Occur, Box<dyn Query>)>, field: Field, values: &[i64]) {
    if values.is_empty() {
        return;
    }

    let mut values = values.to_vec();
    values.sort_unstable();
    values.dedup();
    let mut terms = values
        .into_iter()
        .map(|value| {
            (
                Occur::Should,
                Box::new(TermQuery::new(
                    Term::from_field_i64(field, value),
                    IndexRecordOption::Basic,
                )) as Box<dyn Query>,
            )
        })
        .collect::<Vec<_>>();
    let query = if terms.len() == 1 {
        terms.remove(0).1
    } else {
        Box::new(BooleanQuery::new(terms)) as Box<dyn Query>
    };
    parts.push((Occur::Must, query));
}

fn collect_tweet_hits(
    index: &SearchIndex<TweetFields>,
    query: &dyn Query,
    sort: TweetSearchSort,
    id_field: Field,
    limit: usize,
    offset: usize,
) -> AppResult<Vec<SearchHit>> {
    let (time_field, time_field_name) = tweet_sort_field(index.fields, sort);
    let searcher = index.reader.searcher();
    match sort {
        TweetSearchSort::Relevance => {
            let docs = searcher
                .search(
                    query,
                    &tantivy::collector::TopDocs::with_limit(limit)
                        .and_offset(offset)
                        .order_by_score(),
                )
                .map_err(search_error)?;
            docs.into_iter()
                .map(|(score, address)| {
                    let doc = searcher
                        .doc::<TantivyDocument>(address)
                        .map_err(search_error)?;
                    Ok(SearchHit {
                        id: stored_i64(&doc, id_field)?,
                        score: Some(score),
                        sort_time: stored_i64_opt(&doc, time_field),
                    })
                })
                .collect()
        }
        TweetSearchSort::PublishedAt | TweetSearchSort::CreatedAt | TweetSearchSort::UpdatedAt => {
            let docs = searcher
                .search(
                    query,
                    &tantivy::collector::TopDocs::with_limit(limit)
                        .and_offset(offset)
                        .order_by_fast_field::<i64>(time_field_name, Order::Desc),
                )
                .map_err(search_error)?;
            docs.into_iter()
                .map(|(sort_time, address)| {
                    let doc = searcher
                        .doc::<TantivyDocument>(address)
                        .map_err(search_error)?;
                    Ok(SearchHit {
                        id: stored_i64(&doc, id_field)?,
                        score: None,
                        sort_time: sort_time.or_else(|| stored_i64_opt(&doc, time_field)),
                    })
                })
                .collect()
        }
    }
}

fn tweet_sort_field(fields: TweetFields, sort: TweetSearchSort) -> (Field, &'static str) {
    match sort {
        TweetSearchSort::Relevance | TweetSearchSort::PublishedAt => {
            (fields.published_at, "published_at")
        }
        TweetSearchSort::CreatedAt => (fields.created_at, "created_at"),
        TweetSearchSort::UpdatedAt => (fields.updated_at, "updated_at"),
    }
}

fn normalized_query(raw_query: Option<&str>) -> Option<String> {
    let raw = raw_query?.trim();
    if raw.is_empty() {
        return None;
    }

    let mut value = raw.chars().take(MAX_QUERY_CHARS).collect::<String>();
    if value.chars().filter(|ch| *ch == '"').count() % 2 == 1 {
        value = value.replace('"', " ");
    }

    let mut normalized = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch == '"'
            || ch == '_'
            || ch == '@'
            || ch == '#'
            || ch.is_alphanumeric()
            || ch.is_whitespace()
            || !ch.is_ascii()
        {
            normalized.push(ch);
        } else {
            normalized.push(' ');
        }
    }

    let normalized = normalized
        .split_whitespace()
        .map(|token| match token {
            "AND" => "and",
            "OR" => "or",
            "NOT" => "not",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ");

    (!normalized.is_empty()).then_some(normalized)
}

fn stored_i64(doc: &TantivyDocument, field: Field) -> AppResult<i64> {
    stored_i64_opt(doc, field).ok_or_else(|| AppError::search("indexed document is missing id"))
}

fn stored_i64_opt(doc: &TantivyDocument, field: Field) -> Option<i64> {
    doc.get_first(field)
        .and_then(|value| value.as_value().as_i64())
}

fn memory_bytes(memory_mb: usize) -> usize {
    memory_mb.saturating_mul(1024).saturating_mul(1024)
}

fn search_error(error: impl std::fmt::Display) -> AppError {
    AppError::search(error.to_string())
}

#[derive(Debug)]
struct TweetIndexDocument {
    id: i64,
    author_id: i64,
    body: String,
    relation: String,
    published_at: OffsetDateTime,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

async fn fetch_tweet_documents(
    pool: &PgPool,
    tweet_ids: &[i64],
) -> AppResult<HashMap<i64, TweetIndexDocument>> {
    if tweet_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            t.id,
            t.author_id,
            COALESCE((t.note_text).body, (t.legacy_text).body) AS body,
            CASE
                WHEN t.repost_id IS NOT NULL THEN 'repost'
                WHEN t.quote_tweet_id IS NOT NULL THEN 'quote'
                WHEN t.reply_to_tweet_id IS NOT NULL THEN 'reply'
                ELSE 'original'
            END AS relation,
            t.published_at,
            t.created_at,
            t.updated_at
        FROM tweet.tweet AS t
        WHERE t.id = ANY($1::BIGINT[])
        "#,
    )
    .bind(tweet_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let id = row.get("id");
            (
                id,
                TweetIndexDocument {
                    id,
                    author_id: row.get("author_id"),
                    body: row.get("body"),
                    relation: row.get("relation"),
                    published_at: row.get("published_at"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                },
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn test_search_state(temp_dir: &TempDir) -> SearchState {
        let config = SearchSection {
            enabled: true,
            index_dir: temp_dir.path().to_path_buf(),
            worker_count: 1,
            queue_batch_size: 200,
            writer_memory_mb: 128,
            commit_interval_seconds: 5,
            stale_timeout_seconds: 300,
            max_attempts: 8,
        };

        SearchState {
            inner: Arc::new(SearchRuntime {
                tweets: open_tweet_index(&config).unwrap(),
            }),
        }
    }

    #[tokio::test]
    async fn tweet_search_matches_body_filters_and_time_sort() {
        let temp_dir = TempDir::new().unwrap();
        let state = test_search_state(&temp_dir);
        let index = &state.inner.tweets;
        {
            let mut writer = index.lock_writer().unwrap();
            writer
                .add_document(doc!(
                    index.fields.id => 2001i64,
                    index.fields.author_id => 9001i64,
                    index.fields.body => "人工智能 搜索 排序",
                    index.fields.relation => "original",
                    index.fields.published_at => 10i64,
                    index.fields.created_at => 30i64,
                    index.fields.updated_at => 40i64,
                ))
                .unwrap();
            writer
                .add_document(doc!(
                    index.fields.id => 2002i64,
                    index.fields.author_id => 9001i64,
                    index.fields.body => "人工智能 搜索 新帖子",
                    index.fields.relation => "reply",
                    index.fields.published_at => 20i64,
                    index.fields.created_at => 20i64,
                    index.fields.updated_at => 50i64,
                ))
                .unwrap();
            writer.commit().unwrap();
        }

        let hits = state
            .search_tweets(
                Some("人工智能"),
                &TweetSearchFilters {
                    tweet_ids: Vec::new(),
                    author_ids: Vec::new(),
                    author_id: Some(9001),
                    relation: Some("all".to_owned()),
                },
                TweetSearchSort::PublishedAt,
                10,
                0,
            )
            .await
            .unwrap();
        assert_eq!(
            hits.iter().map(|hit| hit.id).collect::<Vec<_>>(),
            vec![2002, 2001]
        );

        let reply_hits = state
            .search_tweets(
                Some("人工智能"),
                &TweetSearchFilters {
                    tweet_ids: Vec::new(),
                    author_ids: Vec::new(),
                    author_id: Some(9001),
                    relation: Some("reply".to_owned()),
                },
                TweetSearchSort::PublishedAt,
                10,
                0,
            )
            .await
            .unwrap();
        assert_eq!(
            reply_hits.iter().map(|hit| hit.id).collect::<Vec<_>>(),
            vec![2002]
        );

        let created_hits = state
            .search_tweets(
                Some("人工智能"),
                &TweetSearchFilters {
                    tweet_ids: Vec::new(),
                    author_ids: Vec::new(),
                    author_id: Some(9001),
                    relation: Some("all".to_owned()),
                },
                TweetSearchSort::CreatedAt,
                10,
                0,
            )
            .await
            .unwrap();
        assert_eq!(
            created_hits.iter().map(|hit| hit.id).collect::<Vec<_>>(),
            vec![2001, 2002]
        );

        let updated_hits = state
            .search_tweets(
                Some("人工智能"),
                &TweetSearchFilters {
                    tweet_ids: Vec::new(),
                    author_ids: Vec::new(),
                    author_id: Some(9001),
                    relation: Some("all".to_owned()),
                },
                TweetSearchSort::UpdatedAt,
                10,
                0,
            )
            .await
            .unwrap();
        assert_eq!(
            updated_hits.iter().map(|hit| hit.id).collect::<Vec<_>>(),
            vec![2002, 2001]
        );
    }

    #[tokio::test]
    async fn tweet_search_keeps_ids_as_exact_filters() {
        let temp_dir = TempDir::new().unwrap();
        let state = test_search_state(&temp_dir);
        let index = &state.inner.tweets;
        {
            let mut writer = index.lock_writer().unwrap();
            writer
                .add_document(doc!(
                    index.fields.id => 2001i64,
                    index.fields.author_id => 9001i64,
                    index.fields.body => "alpha only",
                    index.fields.relation => "original",
                    index.fields.published_at => 10i64,
                    index.fields.created_at => 10i64,
                    index.fields.updated_at => 10i64,
                ))
                .unwrap();
            writer
                .add_document(doc!(
                    index.fields.id => 2002i64,
                    index.fields.author_id => 9002i64,
                    index.fields.body => "2001 alpha",
                    index.fields.relation => "original",
                    index.fields.published_at => 20i64,
                    index.fields.created_at => 20i64,
                    index.fields.updated_at => 20i64,
                ))
                .unwrap();
            writer.commit().unwrap();
        }

        let body_hits = state
            .search_tweets(
                Some("2001"),
                &TweetSearchFilters::default(),
                TweetSearchSort::PublishedAt,
                10,
                0,
            )
            .await
            .unwrap();
        assert_eq!(
            body_hits.iter().map(|hit| hit.id).collect::<Vec<_>>(),
            vec![2002]
        );

        let exact_hits = state
            .search_tweets(
                Some("alpha"),
                &TweetSearchFilters {
                    tweet_ids: vec![2001],
                    author_ids: vec![9001],
                    author_id: None,
                    relation: Some("all".to_owned()),
                },
                TweetSearchSort::PublishedAt,
                10,
                0,
            )
            .await
            .unwrap();
        assert_eq!(
            exact_hits.iter().map(|hit| hit.id).collect::<Vec<_>>(),
            vec![2001]
        );
    }

    #[test]
    fn startup_backfill_decision_detects_count_mismatch() {
        assert!(!should_backfill_index(0, 0));
        assert!(!should_backfill_index(10, 10));
        assert!(should_backfill_index(0, 10));
        assert!(should_backfill_index(12, 10));
    }

    #[test]
    fn parses_tweet_search_sort() {
        assert_eq!(
            TweetSearchSort::parse(None).unwrap(),
            TweetSearchSort::Relevance
        );
        assert_eq!(
            TweetSearchSort::parse(Some("publishedAt")).unwrap(),
            TweetSearchSort::PublishedAt
        );
        assert_eq!(
            TweetSearchSort::parse(Some("createdAt")).unwrap(),
            TweetSearchSort::CreatedAt
        );
        assert_eq!(
            TweetSearchSort::parse(Some("updatedAt")).unwrap(),
            TweetSearchSort::UpdatedAt
        );
        assert!(TweetSearchSort::parse(Some("time")).is_err());
    }

    #[test]
    fn document_count_reports_committed_tweets() {
        let temp_dir = TempDir::new().unwrap();
        let state = test_search_state(&temp_dir);
        let index = &state.inner.tweets;
        {
            let mut writer = index.lock_writer().unwrap();
            writer
                .add_document(doc!(
                    index.fields.id => 2001i64,
                    index.fields.author_id => 9001i64,
                    index.fields.body => "人工智能 搜索",
                    index.fields.relation => "original",
                    index.fields.published_at => 20i64,
                    index.fields.created_at => 20i64,
                    index.fields.updated_at => 20i64,
                ))
                .unwrap();
            writer.commit().unwrap();
        }

        assert_eq!(state.tweet_document_count().unwrap(), 1);
    }

    #[test]
    fn normalizes_query_for_simplified_query_parser() {
        assert_eq!(
            normalized_query(Some(r#"field:value +(测试) OR rust"#)).unwrap(),
            "field value 测试 or rust"
        );
    }
}

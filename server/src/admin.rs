use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    auth::{self, ActiveSession},
    db,
    error::{AppError, AppResult},
    state::AppState,
};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 100;
const CURSOR_VERSION: u8 = 1;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub q: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UserListQuery {
    #[serde(flatten)]
    pub list: ListQuery,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserCursor {
    v: u8,
    q: Option<String>,
    status: String,
    #[serde(with = "cursor_datetime")]
    created_at: OffsetDateTime,
    id: Uuid,
}

mod cursor_datetime {
    use super::*;
    use serde::de::{self, SeqAccess, Visitor};
    use std::fmt;
    use time::{Date, PrimitiveDateTime, Time, UtcOffset};

    pub fn serialize<S>(value: &OffsetDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let formatted = value.format(&Rfc3339).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&formatted)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OffsetDateTimeVisitor;

        impl<'de> Visitor<'de> for OffsetDateTimeVisitor {
            type Value = OffsetDateTime;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an RFC3339 datetime string or legacy datetime tuple")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                OffsetDateTime::parse(value, &Rfc3339).map_err(E::custom)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let year = seq
                    .next_element::<i32>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let ordinal = seq
                    .next_element::<u16>()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let hour = seq
                    .next_element::<u8>()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let minute = seq
                    .next_element::<u8>()?
                    .ok_or_else(|| de::Error::invalid_length(3, &self))?;
                let second = seq
                    .next_element::<u8>()?
                    .ok_or_else(|| de::Error::invalid_length(4, &self))?;
                let nanosecond = seq
                    .next_element::<u32>()?
                    .ok_or_else(|| de::Error::invalid_length(5, &self))?;
                let offset_hours = seq
                    .next_element::<i8>()?
                    .ok_or_else(|| de::Error::invalid_length(6, &self))?;
                let offset_minutes = seq
                    .next_element::<i8>()?
                    .ok_or_else(|| de::Error::invalid_length(7, &self))?;
                let offset_seconds = seq
                    .next_element::<i8>()?
                    .ok_or_else(|| de::Error::invalid_length(8, &self))?;

                let date = Date::from_ordinal_date(year, ordinal).map_err(de::Error::custom)?;
                let time = Time::from_hms_nano(hour, minute, second, nanosecond)
                    .map_err(de::Error::custom)?;
                let offset = UtcOffset::from_hms(offset_hours, offset_minutes, offset_seconds)
                    .map_err(de::Error::custom)?;

                Ok(PrimitiveDateTime::new(date, time).assume_offset(offset))
            }
        }

        deserializer.deserialize_any(OffsetDateTimeVisitor)
    }
}

pub async fn list_users(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Query(query): Query<UserListQuery>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let limit = resolve_limit(query.list.limit);
    let q = normalize_query(query.list.q.as_deref());
    let status = normalize_user_status(query.status.as_deref())?;
    let cursor = decode_cursor::<UserCursor>(query.list.cursor.as_deref())?;

    if let Some(cursor) = cursor.as_ref() {
        ensure_cursor_version(cursor.v)?;
        ensure_filter_match(
            cursor.q.as_deref(),
            q.as_deref(),
            "cursor does not match query",
        )?;
        ensure_filter_match(
            Some(cursor.status.as_str()),
            Some(status.as_str()),
            "cursor does not match status filter",
        )?;
    }

    let search = like_pattern(q.as_deref());
    let use_cursor = cursor.is_some();
    let rows = sqlx::query(
        r#"
        SELECT id, username::text AS username, is_admin, disabled_at, created_at, updated_at
        FROM users
        WHERE (
                $1::text = 'all'
            OR ($1 = 'active' AND disabled_at IS NULL)
            OR ($1 = 'disabled' AND disabled_at IS NOT NULL)
        )
          AND (
                $2::text IS NULL
            OR username::text ILIKE $2
            OR id::text ILIKE $2
          )
          AND (
                NOT $3
            OR (created_at, id) < ($4, $5)
          )
        ORDER BY created_at DESC, id DESC
        LIMIT $6
        "#,
    )
    .bind(&status)
    .bind(search.as_deref())
    .bind(use_cursor)
    .bind(cursor.as_ref().map(|item| item.created_at))
    .bind(cursor.as_ref().map(|item| item.id))
    .bind(limit_plus_one(limit))
    .fetch_all(&state.db)
    .await?;

    let (items, next_cursor) = paginate_rows(rows, limit, |row| {
        let created_at: OffsetDateTime = row.get("created_at");
        let disabled_at: Option<OffsetDateTime> = row.get("disabled_at");
        let id: Uuid = row.get("id");
        (
            json!({
                "id": id,
                "username": row.get::<String, _>("username"),
                "isAdmin": row.get::<bool, _>("is_admin"),
                "disabled": disabled_at.is_some(),
                "disabledAt": format_time_opt(disabled_at),
                "createdAt": format_time(created_at),
                "updatedAt": format_time(row.get("updated_at")),
            }),
            UserCursor {
                v: CURSOR_VERSION,
                q: q.clone(),
                status: status.clone(),
                created_at,
                id,
            },
        )
    })?;

    Ok(Json(list_response(items, next_cursor)))
}

pub async fn get_user(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let _session = auth::require_admin_session(session)?;
    let row = sqlx::query(
        r#"
        SELECT
            u.id,
            u.username::text AS username,
            u.is_admin,
            u.disabled_at,
            u.disabled_by_user_id,
            u.created_at,
            u.updated_at,
            to_jsonb(u) AS record,
            COALESCE((
                SELECT jsonb_agg(to_jsonb(t))
                FROM (
                    SELECT selector, user_id, sso_subject_id, authorization_id, registration_state, expires_at, last_seen_at, created_at
                    FROM sessions
                    WHERE user_id = u.id
                    ORDER BY created_at DESC
                    LIMIT 20
                ) t
            ), '[]'::jsonb) AS sessions,
            COALESCE((
                SELECT jsonb_agg(to_jsonb(t))
                FROM (
                    SELECT authorization_id, sso_subject_id, user_id, status, last_checked_at, remote_expires_at, revoked_at, created_at, updated_at
                    FROM user_sso_authorizations
                    WHERE user_id = u.id
                    ORDER BY created_at DESC
                    LIMIT 20
                ) t
            ), '[]'::jsonb) AS authorizations,
            COALESCE((
                SELECT jsonb_agg(to_jsonb(t))
                FROM (
                    SELECT id, actor_user_id, event_type, resource_type, resource_id, details, created_at
                    FROM audit_events
                    WHERE resource_type = 'user'
                      AND resource_id = u.id::text
                    ORDER BY created_at DESC
                    LIMIT 20
                ) t
            ), '[]'::jsonb) AS audit_events
        FROM users u
        WHERE u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    let row = row.ok_or_else(|| AppError::not_found("user not found"))?;
    let disabled_at: Option<OffsetDateTime> = row.get("disabled_at");

    Ok(Json(detail_response(
        json!({
            "id": row.get::<Uuid, _>("id"),
            "username": row.get::<String, _>("username"),
            "isAdmin": row.get::<bool, _>("is_admin"),
            "disabled": disabled_at.is_some(),
            "disabledAt": format_time_opt(disabled_at),
            "disabledByUserId": row.get::<Option<Uuid>, _>("disabled_by_user_id"),
            "createdAt": format_time(row.get("created_at")),
            "updatedAt": format_time(row.get("updated_at")),
        }),
        row.get("record"),
        json!({
            "sessions": row.get::<Value, _>("sessions"),
            "authorizations": row.get::<Value, _>("authorizations"),
            "auditEvents": row.get::<Value, _>("audit_events"),
        }),
    )))
}

pub async fn disable_user(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let admin = auth::require_admin_session(session)?;
    if admin.record.user_id == Some(user_id) {
        return Err(AppError::bad_request(
            "cannot disable current admin account",
        ));
    }

    let mut tx = state.db.begin().await?;
    let existing = sqlx::query(
        r#"
        SELECT id, username::text AS username, is_admin, disabled_at, created_at, updated_at
        FROM users
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let existing = existing.ok_or_else(|| AppError::not_found("user not found"))?;
    let already_disabled: Option<OffsetDateTime> = existing.get("disabled_at");
    let now = OffsetDateTime::now_utc();

    if already_disabled.is_none() {
        sqlx::query(
            r#"
            UPDATE users
            SET disabled_at = $2,
                disabled_by_user_id = $3,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .bind(now)
        .bind(admin.record.user_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

        db::insert_audit_event_tx(
            &mut tx,
            admin.record.user_id,
            "user.disabled",
            "user",
            Some(user_id.to_string()),
            json!({
                "disabled_at": format_time(now),
            }),
        )
        .await?;
    }

    let updated = fetch_user_summary_json_tx(&mut tx, user_id).await?;
    tx.commit().await?;

    Ok(Json(json!({
        "ok": true,
        "user": updated,
    })))
}

pub async fn enable_user(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let admin = auth::require_admin_session(session)?;
    let mut tx = state.db.begin().await?;
    let existing = sqlx::query(
        r#"
        SELECT id, disabled_at
        FROM users
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let existing = existing.ok_or_else(|| AppError::not_found("user not found"))?;
    let disabled_at: Option<OffsetDateTime> = existing.get("disabled_at");

    if disabled_at.is_some() {
        sqlx::query(
            r#"
            UPDATE users
            SET disabled_at = NULL,
                disabled_by_user_id = NULL,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        db::insert_audit_event_tx(
            &mut tx,
            admin.record.user_id,
            "user.enabled",
            "user",
            Some(user_id.to_string()),
            json!({}),
        )
        .await?;
    }

    let updated = fetch_user_summary_json_tx(&mut tx, user_id).await?;
    tx.commit().await?;

    Ok(Json(json!({
        "ok": true,
        "user": updated,
    })))
}

async fn fetch_user_summary_json_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> AppResult<Value> {
    let row = sqlx::query(
        r#"
        SELECT id, username::text AS username, is_admin, disabled_at, created_at, updated_at
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    let disabled_at: Option<OffsetDateTime> = row.get("disabled_at");

    Ok(json!({
        "id": row.get::<Uuid, _>("id"),
        "username": row.get::<String, _>("username"),
        "isAdmin": row.get::<bool, _>("is_admin"),
        "disabled": disabled_at.is_some(),
        "disabledAt": format_time_opt(disabled_at),
        "createdAt": format_time(row.get("created_at")),
        "updatedAt": format_time(row.get("updated_at")),
    }))
}

fn list_response(items: Vec<Value>, next_cursor: Option<String>) -> Value {
    json!({
        "items": items,
        "nextCursor": next_cursor,
    })
}

fn detail_response(summary: Value, record: Value, related: Value) -> Value {
    json!({
        "summary": summary,
        "record": record,
        "related": related,
    })
}

fn resolve_limit(value: Option<usize>) -> usize {
    value.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

fn limit_plus_one(limit: usize) -> i64 {
    (limit.saturating_add(1)) as i64
}

fn normalize_query(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn like_pattern(value: Option<&str>) -> Option<String> {
    value.map(|value| format!("%{value}%"))
}

fn normalize_user_status(raw: Option<&str>) -> AppResult<String> {
    let value = raw.unwrap_or("all").trim().to_ascii_lowercase();
    match value.as_str() {
        "all" | "active" | "disabled" => Ok(value),
        _ => Err(AppError::bad_request(
            "status must be one of all, active, disabled",
        )),
    }
}

fn ensure_cursor_version(version: u8) -> AppResult<()> {
    if version == CURSOR_VERSION {
        Ok(())
    } else {
        Err(AppError::bad_request("unsupported cursor version"))
    }
}

fn ensure_filter_match(
    cursor_value: Option<&str>,
    request_value: Option<&str>,
    message: &str,
) -> AppResult<()> {
    if cursor_value == request_value {
        Ok(())
    } else {
        Err(AppError::bad_request(message))
    }
}

fn paginate_rows<T, C, F>(
    mut rows: Vec<sqlx::postgres::PgRow>,
    limit: usize,
    map: F,
) -> AppResult<(Vec<T>, Option<String>)>
where
    C: Serialize,
    F: Fn(sqlx::postgres::PgRow) -> (T, C),
{
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }

    let mut items = Vec::with_capacity(rows.len());
    let mut last_cursor = None;

    for row in rows {
        let (item, cursor) = map(row);
        items.push(item);
        last_cursor = Some(encode_cursor(&cursor)?);
    }

    Ok((items, has_more.then_some(last_cursor).flatten()))
}

fn encode_cursor<T: Serialize>(value: &T) -> AppResult<String> {
    let bytes = serde_json::to_vec(value)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor<T: DeserializeOwned>(raw: Option<&str>) -> AppResult<Option<T>> {
    let Some(raw) = raw else {
        return Ok(None);
    };

    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| AppError::bad_request("invalid cursor"))?;
    let cursor =
        serde_json::from_slice(&bytes).map_err(|_| AppError::bad_request("invalid cursor"))?;
    Ok(Some(cursor))
}

fn format_time(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| value.unix_timestamp().to_string())
}

fn format_time_opt(value: Option<OffsetDateTime>) -> Option<String> {
    value.map(format_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_cursor_payload() {
        let cursor = UserCursor {
            v: CURSOR_VERSION,
            q: Some("demo".to_owned()),
            status: "active".to_owned(),
            created_at: OffsetDateTime::now_utc(),
            id: Uuid::now_v7(),
        };

        let encoded = encode_cursor(&cursor).unwrap();
        let decoded = decode_cursor::<UserCursor>(Some(&encoded)).unwrap();
        assert_eq!(decoded.unwrap().status, "active");

        let raw = URL_SAFE_NO_PAD.decode(encoded).unwrap();
        let payload: Value = serde_json::from_slice(&raw).unwrap();
        assert!(payload.get("created_at").unwrap().is_string());
    }

    #[test]
    fn accepts_legacy_cursor_datetime_payload() {
        let legacy = serde_json::json!({
            "v": CURSOR_VERSION,
            "q": "demo",
            "status": "active",
            "created_at": [2026, 93, 9, 29, 21, 628560142, 0, 0, 0],
            "id": Uuid::nil(),
        });
        let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&legacy).unwrap());

        let decoded = decode_cursor::<UserCursor>(Some(&encoded))
            .unwrap()
            .unwrap();
        assert_eq!(decoded.status, "active");
        assert_eq!(
            decoded.created_at,
            time::macros::datetime!(2026-04-03 09:29:21.628560142 UTC)
        );
    }

    #[test]
    fn rejects_invalid_cursor_payload() {
        let error = decode_cursor::<UserCursor>(Some("not-a-valid-cursor")).unwrap_err();
        assert_eq!(error.to_string(), "invalid cursor");
    }

    #[test]
    fn rejects_unknown_user_status_filter() {
        let error = normalize_user_status(Some("paused")).unwrap_err();
        assert_eq!(
            error.to_string(),
            "status must be one of all, active, disabled"
        );
    }
}

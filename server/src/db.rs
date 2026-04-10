use sqlx::{PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct PendingSsoLogin {
    pub state: Uuid,
    pub code_verifier: String,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub selector: Uuid,
    pub verifier_hash: Vec<u8>,
    pub user_id: Option<Uuid>,
    pub username: Option<String>,
    pub user_is_admin: bool,
    pub user_disabled_at: Option<OffsetDateTime>,
    pub sso_subject_id: Uuid,
    pub authorization_id: Uuid,
    pub registration_state: String,
    pub expires_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
    pub authorization_status: String,
    pub authorization_last_checked_at: OffsetDateTime,
    pub authorization_remote_expires_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct SessionUser {
    pub id: Uuid,
    pub username: String,
}

#[derive(Debug, Clone)]
pub struct SubjectBinding {
    pub user: SessionUser,
    pub sso_subject_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct AuthorizationRecord {
    pub authorization_id: Uuid,
    pub sso_subject_id: Uuid,
    pub user_id: Option<Uuid>,
    pub status: String,
    pub last_checked_at: OffsetDateTime,
    pub remote_expires_at: Option<OffsetDateTime>,
    pub revoked_at: Option<OffsetDateTime>,
}

pub async fn healthcheck(pool: &PgPool) -> AppResult<()> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await?;
    Ok(())
}

pub async fn delete_expired_pending_sso_logins(
    pool: &PgPool,
    now: OffsetDateTime,
) -> AppResult<u64> {
    let result = sqlx::query("DELETE FROM iam.pending_sso_logins WHERE expires_at <= $1")
        .bind(now)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn insert_pending_sso_login(
    pool: &PgPool,
    state: Uuid,
    code_verifier: &str,
    expires_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO iam.pending_sso_logins (state, code_verifier, expires_at)
        VALUES ($1, $2, $3)
        ON CONFLICT (state) DO UPDATE
        SET code_verifier = EXCLUDED.code_verifier,
            expires_at = EXCLUDED.expires_at
        "#,
    )
    .bind(state)
    .bind(code_verifier)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn consume_pending_sso_login(
    pool: &PgPool,
    state: Uuid,
    now: OffsetDateTime,
) -> AppResult<Option<PendingSsoLogin>> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        r#"
        SELECT state, code_verifier, expires_at
        FROM iam.pending_sso_logins
        WHERE state = $1
        "#,
    )
    .bind(state)
    .fetch_optional(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM iam.pending_sso_logins WHERE state = $1")
        .bind(state)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(row.and_then(|row| {
        let expires_at: OffsetDateTime = row.get("expires_at");
        (expires_at > now).then(|| PendingSsoLogin {
            state: row.get("state"),
            code_verifier: row.get("code_verifier"),
            expires_at,
        })
    }))
}

pub async fn upsert_authorization(
    pool: &PgPool,
    authorization_id: Uuid,
    sso_subject_id: Uuid,
    user_id: Option<Uuid>,
    status: &str,
    last_checked_at: OffsetDateTime,
    remote_expires_at: Option<OffsetDateTime>,
    revoked_at: Option<OffsetDateTime>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO iam.user_sso_authorizations (
            authorization_id,
            sso_subject_id,
            user_id,
            status,
            last_checked_at,
            remote_expires_at,
            revoked_at,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        ON CONFLICT (authorization_id) DO UPDATE
        SET sso_subject_id = EXCLUDED.sso_subject_id,
            user_id = COALESCE(EXCLUDED.user_id, iam.user_sso_authorizations.user_id),
            status = EXCLUDED.status,
            last_checked_at = EXCLUDED.last_checked_at,
            remote_expires_at = EXCLUDED.remote_expires_at,
            revoked_at = EXCLUDED.revoked_at,
            updated_at = NOW()
        "#,
    )
    .bind(authorization_id)
    .bind(sso_subject_id)
    .bind(user_id)
    .bind(status)
    .bind(last_checked_at)
    .bind(remote_expires_at)
    .bind(revoked_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn find_subject_binding(
    pool: &PgPool,
    sso_subject_id: Uuid,
) -> AppResult<Option<SubjectBinding>> {
    let row = sqlx::query(
        r#"
        SELECT u.id AS user_id, u.username::text AS username, us.sso_subject_id
        FROM iam.user_sso_subjects us
        INNER JOIN iam.users u ON u.id = us.user_id
        WHERE us.sso_subject_id = $1
        "#,
    )
    .bind(sso_subject_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| SubjectBinding {
        user: SessionUser {
            id: row.get("user_id"),
            username: row.get("username"),
        },
        sso_subject_id: row.get("sso_subject_id"),
    }))
}

pub async fn create_session(
    pool: &PgPool,
    selector: Uuid,
    verifier_hash: Vec<u8>,
    user_id: Option<Uuid>,
    sso_subject_id: Uuid,
    authorization_id: Uuid,
    registration_state: &str,
    expires_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO iam.sessions (
            selector,
            verifier_hash,
            user_id,
            sso_subject_id,
            authorization_id,
            registration_state,
            expires_at,
            last_seen_at,
            created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        "#,
    )
    .bind(selector)
    .bind(verifier_hash)
    .bind(user_id)
    .bind(sso_subject_id)
    .bind(authorization_id)
    .bind(registration_state)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn find_session(pool: &PgPool, selector: Uuid) -> AppResult<Option<SessionRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            s.selector,
            s.verifier_hash,
            s.user_id,
            u.username::text AS username,
            COALESCE(u.is_admin, FALSE) AS user_is_admin,
            u.disabled_at AS user_disabled_at,
            s.sso_subject_id,
            s.authorization_id,
            s.registration_state,
            s.expires_at,
            s.last_seen_at,
            s.created_at,
            a.status AS authorization_status,
            a.last_checked_at AS authorization_last_checked_at,
            a.remote_expires_at AS authorization_remote_expires_at
        FROM iam.sessions s
        LEFT JOIN iam.users u ON u.id = s.user_id
        INNER JOIN iam.user_sso_authorizations a ON a.authorization_id = s.authorization_id
        WHERE s.selector = $1
        "#,
    )
    .bind(selector)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| SessionRecord {
        selector: row.get("selector"),
        verifier_hash: row.get("verifier_hash"),
        user_id: row.get("user_id"),
        username: row.get("username"),
        user_is_admin: row.get("user_is_admin"),
        user_disabled_at: row.get("user_disabled_at"),
        sso_subject_id: row.get("sso_subject_id"),
        authorization_id: row.get("authorization_id"),
        registration_state: row.get("registration_state"),
        expires_at: row.get("expires_at"),
        last_seen_at: row.get("last_seen_at"),
        created_at: row.get("created_at"),
        authorization_status: row.get("authorization_status"),
        authorization_last_checked_at: row.get("authorization_last_checked_at"),
        authorization_remote_expires_at: row.get("authorization_remote_expires_at"),
    }))
}

pub async fn touch_session(
    pool: &PgPool,
    selector: Uuid,
    last_seen_at: OffsetDateTime,
    expires_at: OffsetDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE iam.sessions
        SET last_seen_at = $2,
            expires_at = $3
        WHERE selector = $1
        "#,
    )
    .bind(selector)
    .bind(last_seen_at)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete_session(pool: &PgPool, selector: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM iam.sessions WHERE selector = $1")
        .bind(selector)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_sessions_by_authorization(
    pool: &PgPool,
    authorization_id: Uuid,
) -> AppResult<u64> {
    let result = sqlx::query("DELETE FROM iam.sessions WHERE authorization_id = $1")
        .bind(authorization_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn find_authorization(
    pool: &PgPool,
    authorization_id: Uuid,
) -> AppResult<Option<AuthorizationRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            authorization_id,
            sso_subject_id,
            user_id,
            status,
            last_checked_at,
            remote_expires_at,
            revoked_at
        FROM iam.user_sso_authorizations
        WHERE authorization_id = $1
        "#,
    )
    .bind(authorization_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| AuthorizationRecord {
        authorization_id: row.get("authorization_id"),
        sso_subject_id: row.get("sso_subject_id"),
        user_id: row.get("user_id"),
        status: row.get("status"),
        last_checked_at: row.get("last_checked_at"),
        remote_expires_at: row.get("remote_expires_at"),
        revoked_at: row.get("revoked_at"),
    }))
}

pub async fn update_authorization_status(
    pool: &PgPool,
    authorization_id: Uuid,
    status: &str,
    last_checked_at: OffsetDateTime,
    remote_expires_at: Option<OffsetDateTime>,
    revoked_at: Option<OffsetDateTime>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE iam.user_sso_authorizations
        SET status = $2,
            last_checked_at = $3,
            remote_expires_at = $4,
            revoked_at = $5,
            updated_at = NOW()
        WHERE authorization_id = $1
        "#,
    )
    .bind(authorization_id)
    .bind(status)
    .bind(last_checked_at)
    .bind(remote_expires_at)
    .bind(revoked_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn insert_audit_event(
    pool: &PgPool,
    actor_user_id: Option<Uuid>,
    event_type: &str,
    resource_type: &str,
    resource_id: Option<String>,
    details: serde_json::Value,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO audit.audit_events (id, actor_user_id, event_type, resource_type, resource_id, details)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(actor_user_id)
    .bind(event_type)
    .bind(resource_type)
    .bind(resource_id)
    .bind(details)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn insert_audit_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Option<Uuid>,
    event_type: &str,
    resource_type: &str,
    resource_id: Option<String>,
    details: serde_json::Value,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO audit.audit_events (id, actor_user_id, event_type, resource_type, resource_id, details)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(actor_user_id)
    .bind(event_type)
    .bind(resource_type)
    .bind(resource_id)
    .bind(details)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn bind_username_to_subject(
    pool: &PgPool,
    username: &str,
    sso_subject_id: Uuid,
    authorization_id: Uuid,
    selector: Uuid,
) -> AppResult<SessionUser> {
    let mut tx = pool.begin().await?;
    let now = OffsetDateTime::now_utc();
    let user_id = Uuid::now_v7();

    let user_insert = sqlx::query(
        r#"
        INSERT INTO iam.users (id, username, created_at, updated_at)
        VALUES ($1, $2, $3, $3)
        "#,
    )
    .bind(user_id)
    .bind(username)
    .bind(now)
    .execute(&mut *tx)
    .await;

    if let Err(error) = user_insert {
        tx.rollback().await?;
        if let sqlx::Error::Database(db_error) = &error {
            if db_error.constraint().is_some() {
                return Err(AppError::conflict("username is already taken"));
            }
        }
        return Err(error.into());
    }

    sqlx::query(
        r#"
        INSERT INTO iam.user_sso_subjects (id, user_id, sso_subject_id, created_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(sso_subject_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE iam.user_sso_authorizations
        SET user_id = $2,
            updated_at = NOW()
        WHERE authorization_id = $1
        "#,
    )
    .bind(authorization_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE iam.sessions
        SET user_id = $2,
            registration_state = 'active'
        WHERE selector = $1
        "#,
    )
    .bind(selector)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    insert_audit_event_tx(
        &mut tx,
        Some(user_id),
        "user.registered",
        "user",
        Some(user_id.to_string()),
        serde_json::json!({
            "username": username,
            "sso_subject_id": sso_subject_id,
        }),
    )
    .await?;

    tx.commit().await?;

    Ok(SessionUser {
        id: user_id,
        username: username.to_owned(),
    })
}

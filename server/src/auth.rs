use axum::{
    Json,
    extract::{Extension, Query, Request, State},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use http::Extensions;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{
    config::SessionSection,
    db,
    error::{AppError, AppResult},
    security::{
        TokenKind, issue_compound_secret, parse_compound_secret, pkce_s256, token_verifier_mac,
    },
    state::AppState,
};

#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub record: db::SessionRecord,
}

#[derive(Debug, Serialize)]
pub struct SessionMeResponse {
    pub authenticated: bool,
    pub registered: bool,
    pub username: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    pub account_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InternalSessionMeResponse {
    pub authenticated: bool,
    pub registered: bool,
    pub is_admin: bool,
    pub disabled: bool,
    pub user_id: Option<Uuid>,
    pub username: Option<String>,
    pub subject_id: Option<Uuid>,
    pub authorization_id: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterCompleteRequest {
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterCompleteResponse {
    pub user_id: Uuid,
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RevocationWebhookRequest {
    pub authorization_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct LogoutResponse {
    pub ok: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct SsoExchangeRequest<'a> {
    code: &'a str,
    code_verifier: &'a str,
}

#[derive(Debug, Deserialize)]
struct SsoExchangeResponse {
    subject_id: Uuid,
    authorization_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct AuthorizationStatusResponse {
    #[serde(rename = "subject_id")]
    _subject_id: Uuid,
    active: bool,
    status: String,
    #[serde(default, with = "time::serde::rfc3339::option")]
    expires_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    revoked_at: Option<OffsetDateTime>,
}

pub async fn session_cookie_middleware(
    State(state): State<AppState>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> AppResult<Response> {
    let cookie_action = resolve_request_session(&state, &jar, request.extensions_mut()).await?;
    let mut response = next.run(request).await;
    apply_cookie_action(&state, &mut response, cookie_action);
    Ok(response)
}

pub async fn account_login(
    State(state): State<AppState>,
    jar: CookieJar,
    session: Option<Extension<ActiveSession>>,
) -> AppResult<(CookieJar, Redirect)> {
    if session.is_some() {
        return Ok((jar, Redirect::temporary("/account")));
    }

    let (cookie, login_url) = begin_sso_login(&state).await?;
    Ok((jar.add(cookie), Redirect::temporary(&login_url)))
}

pub async fn sso_callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<CallbackQuery>,
) -> AppResult<(CookieJar, Redirect)> {
    let state_cookie_value = jar
        .get(&state.settings.config.session.pending_login_cookie_name)
        .map(|cookie| cookie.value().to_owned());
    let jar = clear_pending_cookie(&state, jar);

    if let Some(error) = query.error {
        return Ok((jar, Redirect::temporary(&format!("/account?error={error}"))));
    }

    let code = query
        .code
        .ok_or_else(|| AppError::bad_request("missing callback code"))?;
    let state_cookie =
        state_cookie_value.ok_or_else(|| AppError::bad_request("missing pending login state"))?;
    let state_id = Uuid::parse_str(&state_cookie)
        .map_err(|_| AppError::bad_request("invalid pending login state"))?;
    let now = OffsetDateTime::now_utc();
    let pending = db::consume_pending_sso_login(&state.db, state_id, now)
        .await?
        .ok_or_else(|| AppError::bad_request("pending login expired"))?;

    let exchange = exchange_code(&state, &code, &pending.code_verifier).await?;
    let existing = db::find_subject_binding(&state.db, exchange.subject_id).await?;
    let user_id = existing.as_ref().map(|binding| binding.user.id);
    let registration_state = if existing.is_some() {
        "active"
    } else {
        "pending"
    };

    db::upsert_authorization(
        &state.db,
        exchange.authorization_id,
        exchange.subject_id,
        user_id,
        "active",
        now,
        None,
        None,
    )
    .await?;

    let secret =
        issue_compound_secret(&state.settings.secrets.session_hmac_key, TokenKind::Session);
    let expires_at = session_expires_at(&state.settings.config.session, now, now);
    db::create_session(
        &state.db,
        secret.selector,
        secret.verifier_mac.clone(),
        user_id,
        exchange.subject_id,
        exchange.authorization_id,
        registration_state,
        expires_at,
    )
    .await?;

    db::insert_audit_event(
        &state.db,
        user_id,
        "auth.sso_callback",
        "session",
        Some(secret.selector.to_string()),
        serde_json::json!({
            "authorization_id": exchange.authorization_id,
            "sso_subject_id": exchange.subject_id,
            "registration_state": registration_state,
        }),
    )
    .await?;

    Ok((
        jar.add(session_cookie(
            &state.settings.config.session,
            &secret.compound_value(),
            expires_at - now,
        )),
        Redirect::temporary("/account"),
    ))
}

pub async fn session_me(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
) -> AppResult<Json<SessionMeResponse>> {
    Ok(Json(build_public_session_me_response(&state, session)))
}

pub async fn internal_session_me(
    _state: State<AppState>,
    session: Option<Extension<ActiveSession>>,
) -> AppResult<Json<InternalSessionMeResponse>> {
    Ok(Json(build_internal_session_me_response(session)))
}

pub async fn internal_register_complete(
    State(state): State<AppState>,
    session: Option<Extension<ActiveSession>>,
    Json(payload): Json<RegisterCompleteRequest>,
) -> AppResult<Json<RegisterCompleteResponse>> {
    let session = require_pending_registration(session)?;
    let username = normalize_username(&payload.username)?;
    let user = db::bind_username_to_subject(
        &state.db,
        &username,
        session.record.sso_subject_id,
        session.record.authorization_id,
        session.record.selector,
    )
    .await?;

    Ok(Json(RegisterCompleteResponse {
        user_id: user.id,
        username: user.username,
    }))
}

pub async fn internal_logout(
    State(state): State<AppState>,
    jar: CookieJar,
    session: Option<Extension<ActiveSession>>,
) -> AppResult<(CookieJar, Json<LogoutResponse>)> {
    if let Some(Extension(session)) = session {
        let _ = revoke_authorization(&state, session.record.authorization_id).await;
        db::delete_session(&state.db, session.record.selector).await?;
        db::insert_audit_event(
            &state.db,
            session.record.user_id,
            "session.logout",
            "session",
            Some(session.record.selector.to_string()),
            serde_json::json!({
                "authorization_id": session.record.authorization_id,
            }),
        )
        .await?;
    }

    Ok((
        clear_session_cookie(&state, jar),
        Json(LogoutResponse { ok: true }),
    ))
}

pub async fn revocation_webhook(
    State(state): State<AppState>,
    Json(payload): Json<RevocationWebhookRequest>,
) -> AppResult<impl IntoResponse> {
    let now = OffsetDateTime::now_utc();
    match check_authorization(&state, payload.authorization_id).await? {
        Some(status) if !status.active => {
            db::update_authorization_status(
                &state.db,
                payload.authorization_id,
                &status.status,
                now,
                status.expires_at,
                status.revoked_at.or(Some(now)),
            )
            .await?;
            db::delete_sessions_by_authorization(&state.db, payload.authorization_id).await?;
        }
        None => {
            db::update_authorization_status(
                &state.db,
                payload.authorization_id,
                "revoked",
                now,
                None,
                Some(now),
            )
            .await?;
            db::delete_sessions_by_authorization(&state.db, payload.authorization_id).await?;
        }
        _ => {}
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub fn require_registered_session(
    session: Option<Extension<ActiveSession>>,
) -> AppResult<ActiveSession> {
    let Extension(session) = session.ok_or_else(|| AppError::unauthorized("session required"))?;
    if session.record.registration_state != "active" {
        return Err(AppError::unauthorized("registration must be completed"));
    }
    Ok(session)
}

pub fn require_admin_session(
    session: Option<Extension<ActiveSession>>,
) -> AppResult<ActiveSession> {
    let session = require_registered_session(session)?;
    if !session.record.user_is_admin {
        return Err(AppError::forbidden("admin access required"));
    }
    Ok(session)
}

fn require_pending_registration(
    session: Option<Extension<ActiveSession>>,
) -> AppResult<ActiveSession> {
    let Extension(session) = session.ok_or_else(|| AppError::unauthorized("session required"))?;
    if session.record.registration_state != "pending" {
        return Err(AppError::bad_request("session is not pending registration"));
    }
    Ok(session)
}

async fn begin_sso_login(state: &AppState) -> AppResult<(Cookie<'static>, String)> {
    let now = OffsetDateTime::now_utc();
    let state_id = Uuid::now_v7();
    let code_verifier =
        issue_compound_secret(&state.settings.secrets.session_hmac_key, TokenKind::Session)
            .verifier;
    let expires_at =
        now + Duration::seconds(state.settings.config.session.pending_login_ttl_seconds);

    db::delete_expired_pending_sso_logins(&state.db, now).await?;
    db::insert_pending_sso_login(&state.db, state_id, &code_verifier, expires_at).await?;
    register_revocation_webhook(state).await;

    let cookie = pending_cookie(
        &state.settings.config.session,
        state_id.to_string(),
        expires_at - now,
    );
    let login_url = format!(
        "{}/sso/authorize?client_id={}&code_challenge={}",
        state.settings.config.sso.issuer.trim_end_matches('/'),
        state.settings.config.sso.client_id,
        pkce_s256(&code_verifier),
    );

    Ok((cookie, login_url))
}

enum CookieAction {
    None,
    Renew {
        value: String,
        expires_at: OffsetDateTime,
    },
    Clear,
}

async fn resolve_request_session(
    state: &AppState,
    jar: &CookieJar,
    extensions: &mut Extensions,
) -> AppResult<CookieAction> {
    let Some(cookie) = jar.get(&state.settings.config.session.cookie_name) else {
        return Ok(CookieAction::None);
    };
    let Some((selector, verifier)) = parse_compound_secret(cookie.value()) else {
        return Ok(CookieAction::Clear);
    };

    let now = OffsetDateTime::now_utc();
    let Some(mut session) = db::find_session(&state.db, selector).await? else {
        return Ok(CookieAction::Clear);
    };
    let expected = token_verifier_mac(
        &state.settings.secrets.session_hmac_key,
        TokenKind::Session,
        selector,
        &verifier,
    );

    if session.verifier_hash != expected {
        return Ok(CookieAction::Clear);
    }

    if session.user_disabled_at.is_some() {
        db::delete_session(&state.db, selector).await?;
        return Ok(CookieAction::Clear);
    }

    if session.expires_at <= now || session.authorization_status != "active" {
        db::delete_session(&state.db, selector).await?;
        return Ok(CookieAction::Clear);
    }

    refresh_authorization_if_needed(state, &session).await?;
    session = match db::find_session(&state.db, selector).await? {
        Some(record) => record,
        None => return Ok(CookieAction::Clear),
    };

    if session.user_disabled_at.is_some() {
        db::delete_session(&state.db, selector).await?;
        return Ok(CookieAction::Clear);
    }

    if session.authorization_status != "active" {
        db::delete_session(&state.db, selector).await?;
        return Ok(CookieAction::Clear);
    }

    if state.settings.config.session.auto_renew {
        let expires_at =
            session_expires_at(&state.settings.config.session, session.created_at, now);
        db::touch_session(&state.db, selector, now, expires_at).await?;
        session.expires_at = expires_at;
    }

    extensions.insert(ActiveSession {
        record: session.clone(),
    });

    Ok(if state.settings.config.session.auto_renew {
        CookieAction::Renew {
            value: cookie.value().to_owned(),
            expires_at: session.expires_at,
        }
    } else {
        CookieAction::None
    })
}

fn apply_cookie_action(state: &AppState, response: &mut Response, action: CookieAction) {
    use axum::http::{HeaderValue, header::SET_COOKIE};

    let cookie = match action {
        CookieAction::None => return,
        CookieAction::Renew { value, expires_at } => session_cookie(
            &state.settings.config.session,
            &value,
            expires_at - OffsetDateTime::now_utc(),
        ),
        CookieAction::Clear => removal_cookie(
            &state.settings.config.session.cookie_name,
            state.settings.config.session.cookie_secure,
        ),
    };

    if let Ok(value) = HeaderValue::from_str(&cookie.to_string()) {
        response.headers_mut().append(SET_COOKIE, value);
    }
}

async fn refresh_authorization_if_needed(
    state: &AppState,
    session: &db::SessionRecord,
) -> AppResult<()> {
    let ttl = Duration::seconds(state.settings.config.sso.authorization_cache_ttl_seconds);
    if session.authorization_last_checked_at + ttl > OffsetDateTime::now_utc() {
        return Ok(());
    }

    match check_authorization(state, session.authorization_id).await? {
        Some(status) => {
            let now = OffsetDateTime::now_utc();
            let new_status = if status.active {
                "active"
            } else {
                status.status.as_str()
            };
            let revoked_at = if status.active {
                None
            } else {
                status.revoked_at.or(Some(now))
            };
            db::update_authorization_status(
                &state.db,
                session.authorization_id,
                new_status,
                now,
                status.expires_at,
                revoked_at,
            )
            .await?;
            if !status.active {
                db::delete_sessions_by_authorization(&state.db, session.authorization_id).await?;
            }
        }
        None => {
            let now = OffsetDateTime::now_utc();
            db::update_authorization_status(
                &state.db,
                session.authorization_id,
                "revoked",
                now,
                None,
                Some(now),
            )
            .await?;
            db::delete_sessions_by_authorization(&state.db, session.authorization_id).await?;
        }
    }

    Ok(())
}

async fn exchange_code(
    state: &AppState,
    code: &str,
    code_verifier: &str,
) -> AppResult<SsoExchangeResponse> {
    let response = state
        .auth_http_client
        .post(format!(
            "{}/sso/exchange",
            state.settings.config.sso.issuer.trim_end_matches('/')
        ))
        .header(
            AUTHORIZATION,
            format!("Bearer {}", state.settings.secrets.app_token),
        )
        .header(CONTENT_TYPE, "application/json")
        .json(&SsoExchangeRequest {
            code,
            code_verifier,
        })
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(AppError::upstream(format!(
            "sso exchange failed with status {}",
            response.status()
        )));
    }

    Ok(response.json().await?)
}

async fn check_authorization(
    state: &AppState,
    authorization_id: Uuid,
) -> AppResult<Option<AuthorizationStatusResponse>> {
    let response = state
        .auth_http_client
        .get(format!(
            "{}/sso/authorizations/{}",
            state.settings.config.sso.issuer.trim_end_matches('/'),
            authorization_id
        ))
        .header(
            AUTHORIZATION,
            format!("Bearer {}", state.settings.secrets.app_token),
        )
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(AppError::upstream(format!(
            "authorization check failed with status {}",
            response.status()
        )));
    }

    Ok(Some(response.json().await?))
}

async fn revoke_authorization(state: &AppState, authorization_id: Uuid) -> AppResult<()> {
    let response = state
        .auth_http_client
        .delete(format!(
            "{}/sso/authorizations/{}",
            state.settings.config.sso.issuer.trim_end_matches('/'),
            authorization_id
        ))
        .header(
            AUTHORIZATION,
            format!("Bearer {}", state.settings.secrets.app_token),
        )
        .send()
        .await?;

    if !response.status().is_success() && response.status() != reqwest::StatusCode::NOT_FOUND {
        return Err(AppError::upstream(format!(
            "authorization revoke failed with status {}",
            response.status()
        )));
    }

    Ok(())
}

async fn register_revocation_webhook(state: &AppState) {
    let callback_url = format!(
        "{}/integrations/sso/webhooks/revocations",
        state.settings.config.app.base_url.trim_end_matches('/')
    );
    match state
        .auth_http_client
        .put(format!(
            "{}/sso/authorizations/webhook",
            state.settings.config.sso.issuer.trim_end_matches('/')
        ))
        .header(
            AUTHORIZATION,
            format!("Bearer {}", state.settings.secrets.app_token),
        )
        .header(CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({ "callback_url": callback_url }))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {}
        Ok(response) => tracing::warn!(
            "revocation webhook registration failed: {}",
            response.status()
        ),
        Err(error) => tracing::warn!("revocation webhook registration failed: {error}"),
    }
}

fn normalize_username(raw: &str) -> AppResult<String> {
    let username = raw.trim();
    if !(3..=32).contains(&username.len()) {
        return Err(AppError::bad_request(
            "username length must be between 3 and 32 characters",
        ));
    }
    if !username
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(AppError::bad_request(
            "username may only contain letters, digits, and underscores",
        ));
    }
    Ok(username.to_owned())
}

fn build_public_session_me_response(
    state: &AppState,
    session: Option<Extension<ActiveSession>>,
) -> SessionMeResponse {
    build_public_session_response(
        &state.settings.config.app.base_url,
        session.as_ref().map(|extension| &extension.0.record),
    )
}

fn build_public_session_response(
    base_url: &str,
    session: Option<&db::SessionRecord>,
) -> SessionMeResponse {
    let authenticated = session.is_some();
    let registered = session
        .map(|record| record.registration_state == "active")
        .unwrap_or(false);

    SessionMeResponse {
        authenticated,
        registered,
        username: session.and_then(|record| record.username.clone()),
        expires_at: session.map(|record| record.expires_at),
        account_url: (!registered).then(|| account_management_url(base_url)),
    }
}

fn build_internal_session_me_response(
    session: Option<Extension<ActiveSession>>,
) -> InternalSessionMeResponse {
    let Some(Extension(session)) = session else {
        return InternalSessionMeResponse {
            authenticated: false,
            registered: false,
            is_admin: false,
            disabled: false,
            user_id: None,
            username: None,
            subject_id: None,
            authorization_id: None,
            expires_at: None,
        };
    };

    InternalSessionMeResponse {
        authenticated: true,
        registered: session.record.registration_state == "active",
        is_admin: session.record.user_is_admin,
        disabled: session.record.user_disabled_at.is_some(),
        user_id: session.record.user_id,
        username: session.record.username.clone(),
        subject_id: Some(session.record.sso_subject_id),
        authorization_id: Some(session.record.authorization_id),
        expires_at: Some(session.record.expires_at),
    }
}

fn account_management_url(base_url: &str) -> String {
    format!("{}/account", base_url.trim_end_matches('/'))
}

fn session_expires_at(
    config: &SessionSection,
    created_at: OffsetDateTime,
    now: OffsetDateTime,
) -> OffsetDateTime {
    let mut expires_at = now + Duration::hours(config.ttl_hours);
    if config.absolute_ttl_hours > 0 {
        let absolute = created_at + Duration::hours(config.absolute_ttl_hours);
        if expires_at > absolute {
            expires_at = absolute;
        }
    }
    expires_at
}

fn session_cookie(config: &SessionSection, value: &str, max_age: Duration) -> Cookie<'static> {
    cookie(
        &config.cookie_name,
        config.cookie_secure,
        value.to_owned(),
        max_age,
    )
}

fn pending_cookie(config: &SessionSection, value: String, max_age: Duration) -> Cookie<'static> {
    cookie(
        &config.pending_login_cookie_name,
        config.cookie_secure,
        value,
        max_age,
    )
}

fn removal_cookie(name: &str, secure: bool) -> Cookie<'static> {
    cookie(name, secure, String::new(), Duration::seconds(0))
}

fn cookie(name: &str, secure: bool, value: String, max_age: Duration) -> Cookie<'static> {
    Cookie::build((name.to_owned(), value))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(max_age)
        .build()
}

fn clear_session_cookie(state: &AppState, jar: CookieJar) -> CookieJar {
    jar.remove(removal_cookie(
        &state.settings.config.session.cookie_name,
        state.settings.config.session.cookie_secure,
    ))
}

fn clear_pending_cookie(state: &AppState, jar: CookieJar) -> CookieJar {
    jar.remove(removal_cookie(
        &state.settings.config.session.pending_login_cookie_name,
        state.settings.config.session.cookie_secure,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_session(registration_state: &str, username: Option<&str>) -> db::SessionRecord {
        let now = OffsetDateTime::now_utc();
        db::SessionRecord {
            selector: Uuid::now_v7(),
            verifier_hash: vec![1, 2, 3],
            user_id: username.map(|_| Uuid::now_v7()),
            username: username.map(ToOwned::to_owned),
            user_is_admin: false,
            user_disabled_at: None,
            sso_subject_id: Uuid::now_v7(),
            authorization_id: Uuid::now_v7(),
            registration_state: registration_state.to_owned(),
            expires_at: now + Duration::hours(1),
            last_seen_at: now,
            created_at: now,
            authorization_status: "active".to_owned(),
            authorization_last_checked_at: now,
            authorization_remote_expires_at: None,
        }
    }

    #[test]
    fn public_session_returns_account_url_when_anonymous() {
        let response = build_public_session_response("http://127.0.0.1:3001", None);
        assert!(!response.authenticated);
        assert!(!response.registered);
        assert_eq!(response.username, None);
        assert_eq!(
            response.account_url.as_deref(),
            Some("http://127.0.0.1:3001/account")
        );
    }

    #[test]
    fn public_session_returns_account_url_when_registration_is_pending() {
        let session = sample_session("pending", None);
        let response = build_public_session_response("http://127.0.0.1:3001/", Some(&session));
        assert!(response.authenticated);
        assert!(!response.registered);
        assert_eq!(
            response.account_url.as_deref(),
            Some("http://127.0.0.1:3001/account")
        );
        assert_eq!(response.expires_at, Some(session.expires_at));
    }

    #[test]
    fn public_session_hides_account_url_when_registered() {
        let session = sample_session("active", Some("demo_user"));
        let response = build_public_session_response("http://127.0.0.1:3001", Some(&session));
        assert!(response.authenticated);
        assert!(response.registered);
        assert_eq!(response.username.as_deref(), Some("demo_user"));
        assert_eq!(response.account_url, None);
    }

    #[test]
    fn require_registered_session_rejects_pending_registration() {
        let session = ActiveSession {
            record: sample_session("pending", None),
        };
        let error = require_registered_session(Some(Extension(session))).unwrap_err();
        assert!(matches!(error, AppError::Unauthorized(_)));
        assert_eq!(error.to_string(), "registration must be completed");
    }

    #[test]
    fn require_admin_session_rejects_non_admin_user() {
        let session = ActiveSession {
            record: sample_session("active", Some("demo_user")),
        };
        let error = require_admin_session(Some(Extension(session))).unwrap_err();
        assert!(matches!(error, AppError::Forbidden(_)));
        assert_eq!(error.to_string(), "admin access required");
    }

    #[test]
    fn internal_session_reports_admin_state() {
        let mut record = sample_session("active", Some("demo_user"));
        record.user_is_admin = true;
        let response =
            build_internal_session_me_response(Some(Extension(ActiveSession { record })));
        assert!(response.authenticated);
        assert!(response.registered);
        assert!(response.is_admin);
        assert!(!response.disabled);
    }
}

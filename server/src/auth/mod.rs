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

mod cookies;
mod responses;
mod session;
mod sso;

use self::{cookies::*, responses::*, sso::*};

pub use self::session::*;
pub use self::sso::*;

#[cfg(test)]
use self::session::should_auto_renew_session;

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

    #[test]
    fn batch_tweet_apis_skip_session_auto_renew() {
        assert!(!should_auto_renew_session("/api/v1/tweet/submit"));
        assert!(!should_auto_renew_session("/api/v1/tweet/query"));
        assert!(should_auto_renew_session("/api/v1/session"));
    }
}

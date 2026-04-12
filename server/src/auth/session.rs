use super::*;

pub async fn session_cookie_middleware(
    State(state): State<AppState>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> AppResult<Response> {
    let auto_renew_session = should_auto_renew_session(request.uri().path());
    let cookie_action =
        resolve_request_session(&state, &jar, request.extensions_mut(), auto_renew_session).await?;
    let mut response = next.run(request).await;
    apply_cookie_action(&state, &mut response, cookie_action);
    Ok(response)
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

enum AuthorizationRefreshOutcome {
    Unchanged,
    Active {
        last_checked_at: OffsetDateTime,
        remote_expires_at: Option<OffsetDateTime>,
    },
    Revoked,
}

async fn resolve_request_session(
    state: &AppState,
    jar: &CookieJar,
    extensions: &mut Extensions,
    auto_renew_session: bool,
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

    match refresh_authorization_if_needed(state, &session).await? {
        AuthorizationRefreshOutcome::Unchanged => {}
        AuthorizationRefreshOutcome::Active {
            last_checked_at,
            remote_expires_at,
        } => {
            session.authorization_status = "active".to_owned();
            session.authorization_last_checked_at = last_checked_at;
            session.authorization_remote_expires_at = remote_expires_at;
        }
        AuthorizationRefreshOutcome::Revoked => return Ok(CookieAction::Clear),
    }

    if state.settings.config.session.auto_renew && auto_renew_session {
        let expires_at =
            session_expires_at(&state.settings.config.session, session.created_at, now);
        db::touch_session(&state.db, selector, now, expires_at).await?;
        session.expires_at = expires_at;
    }

    extensions.insert(ActiveSession {
        record: session.clone(),
    });

    Ok(
        if state.settings.config.session.auto_renew && auto_renew_session {
            CookieAction::Renew {
                value: cookie.value().to_owned(),
                expires_at: session.expires_at,
            }
        } else {
            CookieAction::None
        },
    )
}

pub(super) fn should_auto_renew_session(path: &str) -> bool {
    !matches!(path, "/api/v1/tweet/submit" | "/api/v1/tweet/query")
}

async fn refresh_authorization_if_needed(
    state: &AppState,
    session: &db::SessionRecord,
) -> AppResult<AuthorizationRefreshOutcome> {
    let ttl = Duration::seconds(state.settings.config.sso.authorization_cache_ttl_seconds);
    if session.authorization_last_checked_at + ttl > OffsetDateTime::now_utc() {
        return Ok(AuthorizationRefreshOutcome::Unchanged);
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
                return Ok(AuthorizationRefreshOutcome::Revoked);
            }
            Ok(AuthorizationRefreshOutcome::Active {
                last_checked_at: now,
                remote_expires_at: status.expires_at,
            })
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
            Ok(AuthorizationRefreshOutcome::Revoked)
        }
    }
}

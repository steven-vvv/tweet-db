use super::*;

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

pub(super) async fn check_authorization(
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

pub(super) async fn revoke_authorization(
    state: &AppState,
    authorization_id: Uuid,
) -> AppResult<()> {
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

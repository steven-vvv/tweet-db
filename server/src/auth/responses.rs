use super::*;

pub(super) fn normalize_username(raw: &str) -> AppResult<String> {
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

pub(super) fn build_public_session_me_response(
    state: &AppState,
    session: Option<Extension<ActiveSession>>,
) -> SessionMeResponse {
    build_public_session_response(
        &state.settings.config.app.base_url,
        session.as_ref().map(|extension| &extension.0.record),
    )
}

pub(super) fn build_public_session_response(
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

pub(super) fn build_internal_session_me_response(
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

pub(super) fn account_management_url(base_url: &str) -> String {
    format!("{}/account", base_url.trim_end_matches('/'))
}

pub(super) fn session_expires_at(
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

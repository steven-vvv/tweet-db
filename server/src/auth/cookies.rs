use super::*;

pub(super) enum CookieAction {
    None,
    Renew {
        value: String,
        expires_at: OffsetDateTime,
    },
    Clear,
}

pub(super) fn apply_cookie_action(state: &AppState, response: &mut Response, action: CookieAction) {
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

pub(super) fn session_cookie(
    config: &SessionSection,
    value: &str,
    max_age: Duration,
) -> Cookie<'static> {
    cookie(
        &config.cookie_name,
        config.cookie_secure,
        value.to_owned(),
        max_age,
    )
}

pub(super) fn pending_cookie(
    config: &SessionSection,
    value: String,
    max_age: Duration,
) -> Cookie<'static> {
    cookie(
        &config.pending_login_cookie_name,
        config.cookie_secure,
        value,
        max_age,
    )
}

pub(super) fn removal_cookie(name: &str, secure: bool) -> Cookie<'static> {
    cookie(name, secure, String::new(), Duration::seconds(0))
}

pub(super) fn cookie(
    name: &str,
    secure: bool,
    value: String,
    max_age: Duration,
) -> Cookie<'static> {
    Cookie::build((name.to_owned(), value))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(max_age)
        .build()
}

pub(super) fn clear_session_cookie(state: &AppState, jar: CookieJar) -> CookieJar {
    jar.remove(removal_cookie(
        &state.settings.config.session.cookie_name,
        state.settings.config.session.cookie_secure,
    ))
}

pub(super) fn clear_pending_cookie(state: &AppState, jar: CookieJar) -> CookieJar {
    jar.remove(removal_cookie(
        &state.settings.config.session.pending_login_cookie_name,
        state.settings.config.session.cookie_secure,
    ))
}

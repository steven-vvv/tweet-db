CREATE TABLE users (
    id UUID PRIMARY KEY,
    username CITEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE user_sso_subjects (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    sso_subject_id UUID NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, sso_subject_id)
);

CREATE TABLE user_sso_authorizations (
    authorization_id UUID PRIMARY KEY,
    sso_subject_id UUID NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'revoked', 'expired')),
    last_checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    remote_expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_user_sso_authorizations_user_id
ON user_sso_authorizations (user_id, created_at DESC);

CREATE TABLE pending_sso_logins (
    state UUID PRIMARY KEY,
    code_verifier TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pending_sso_logins_expires_at
ON pending_sso_logins (expires_at);

CREATE TABLE sessions (
    selector UUID PRIMARY KEY,
    verifier_hash BYTEA NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    sso_subject_id UUID NOT NULL,
    authorization_id UUID NOT NULL REFERENCES user_sso_authorizations(authorization_id) ON DELETE CASCADE,
    registration_state TEXT NOT NULL CHECK (registration_state IN ('pending', 'active')),
    expires_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_sessions_user_id_expires_at
ON sessions (user_id, expires_at DESC);

CREATE INDEX idx_sessions_authorization_id
ON sessions (authorization_id);

CREATE TABLE audit_events (
    id UUID PRIMARY KEY,
    actor_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    event_type TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_events_created_at
ON audit_events (created_at DESC);


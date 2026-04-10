CREATE TABLE iam.users (
    id UUID PRIMARY KEY,
    username public.citext NOT NULL UNIQUE,
    is_admin BOOLEAN NOT NULL DEFAULT FALSE,
    disabled_at TIMESTAMPTZ,
    disabled_by_user_id UUID REFERENCES iam.users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_created_at
ON iam.users (created_at DESC, id DESC);

CREATE INDEX idx_users_disabled_created_at
ON iam.users (disabled_at, created_at DESC, id DESC);

CREATE TABLE iam.user_sso_subjects (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES iam.users(id) ON DELETE CASCADE,
    sso_subject_id UUID NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, sso_subject_id)
);

CREATE TABLE iam.user_sso_authorizations (
    authorization_id UUID PRIMARY KEY,
    sso_subject_id UUID NOT NULL,
    user_id UUID REFERENCES iam.users(id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'revoked', 'expired')),
    last_checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    remote_expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_user_sso_authorizations_user_id
ON iam.user_sso_authorizations (user_id, created_at DESC);

CREATE TABLE iam.pending_sso_logins (
    state UUID PRIMARY KEY,
    code_verifier TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pending_sso_logins_expires_at
ON iam.pending_sso_logins (expires_at);

CREATE TABLE iam.sessions (
    selector UUID PRIMARY KEY,
    verifier_hash BYTEA NOT NULL,
    user_id UUID REFERENCES iam.users(id) ON DELETE CASCADE,
    sso_subject_id UUID NOT NULL,
    authorization_id UUID NOT NULL REFERENCES iam.user_sso_authorizations(authorization_id) ON DELETE CASCADE,
    registration_state TEXT NOT NULL CHECK (registration_state IN ('pending', 'active')),
    expires_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_sessions_user_id_expires_at
ON iam.sessions (user_id, expires_at DESC);

CREATE INDEX idx_sessions_authorization_id
ON iam.sessions (authorization_id);

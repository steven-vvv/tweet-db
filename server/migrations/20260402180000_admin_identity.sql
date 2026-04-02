ALTER TABLE users
ADD COLUMN is_admin BOOLEAN NOT NULL DEFAULT FALSE,
ADD COLUMN disabled_at TIMESTAMPTZ,
ADD COLUMN disabled_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX idx_users_created_at
ON users (created_at DESC, id DESC);

CREATE INDEX idx_users_disabled_created_at
ON users (disabled_at, created_at DESC, id DESC);

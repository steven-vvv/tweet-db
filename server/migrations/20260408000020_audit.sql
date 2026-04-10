CREATE TABLE audit.audit_events (
    id UUID PRIMARY KEY,
    actor_user_id UUID REFERENCES iam.users(id) ON DELETE SET NULL,
    event_type TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_events_created_at
ON audit.audit_events (created_at DESC);

CREATE INDEX idx_audit_events_resource_created_at
ON audit.audit_events (resource_type, resource_id, created_at DESC, id DESC);

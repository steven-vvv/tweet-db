CREATE EXTENSION IF NOT EXISTS citext WITH SCHEMA public;

CREATE SCHEMA IF NOT EXISTS tweet;
COMMENT ON SCHEMA tweet IS 'Tweet v2 core domain: tweets, Twitter actors, media metadata, dictionaries, and convenience views.';

CREATE SCHEMA IF NOT EXISTS iam;
COMMENT ON SCHEMA iam IS 'Application identity and access management domain: local users, SSO bindings, authorizations, and sessions.';

CREATE SCHEMA IF NOT EXISTS media;
COMMENT ON SCHEMA media IS 'Reserved for future local asset, object storage, and transfer worker subsystems.';

CREATE SCHEMA IF NOT EXISTS vector;
COMMENT ON SCHEMA vector IS 'Reserved for future vector indexing, embedding storage, and retrieval subsystems.';

CREATE SCHEMA IF NOT EXISTS audit;
COMMENT ON SCHEMA audit IS 'Audit and operational logging domain for administrative actions and future governance records.';

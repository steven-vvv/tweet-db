ALTER TABLE actor_profile_versions
ADD COLUMN source_created_at TIMESTAMPTZ;

ALTER TABLE posts
ADD COLUMN source_created_at TIMESTAMPTZ;

CREATE OR REPLACE FUNCTION parse_legacy_source_created_at(raw_value TEXT)
RETURNS TIMESTAMPTZ
LANGUAGE plpgsql
AS $$
DECLARE
    normalized TEXT := BTRIM(raw_value);
BEGIN
    IF raw_value IS NULL OR normalized = '' THEN
        RETURN NULL;
    END IF;

    BEGIN
        RETURN normalized::TIMESTAMPTZ;
    EXCEPTION
        WHEN OTHERS THEN
            NULL;
    END;

    IF normalized ~ '^[A-Z][a-z]{2} [A-Z][a-z]{2} [0-9]{1,2} [0-9]{2}:[0-9]{2}:[0-9]{2} [+-][0-9]{4} [0-9]{4}$' THEN
        BEGIN
            RETURN to_timestamp(
                normalized,
                'Dy Mon FMDD HH24:MI:SS TZHTZM YYYY'
            );
        EXCEPTION
            WHEN OTHERS THEN
                RETURN NULL;
        END;
    END IF;

    RETURN NULL;
END;
$$;

UPDATE actor_profile_versions
SET source_created_at = parse_legacy_source_created_at(source_created_at_raw)
WHERE source_created_at IS NULL;

UPDATE posts
SET source_created_at = parse_legacy_source_created_at(source_created_at_raw)
WHERE source_created_at IS NULL;

DROP FUNCTION parse_legacy_source_created_at(TEXT);

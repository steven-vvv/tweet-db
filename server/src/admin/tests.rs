use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

#[test]
fn round_trips_cursor_payload() {
    let cursor = UserCursor {
        v: CURSOR_VERSION,
        q: Some("demo".to_owned()),
        status: "active".to_owned(),
        created_at: OffsetDateTime::now_utc(),
        id: Uuid::now_v7(),
    };

    let encoded = encode_cursor(&cursor).unwrap();
    let decoded = decode_cursor::<UserCursor>(Some(&encoded)).unwrap();
    assert_eq!(decoded.unwrap().status, "active");

    let raw = URL_SAFE_NO_PAD.decode(encoded).unwrap();
    let payload: Value = serde_json::from_slice(&raw).unwrap();
    assert!(payload.get("created_at").unwrap().is_string());
}

#[test]
fn accepts_legacy_cursor_datetime_payload() {
    let legacy = serde_json::json!({
        "v": CURSOR_VERSION,
        "q": "demo",
        "status": "active",
        "created_at": [2026, 93, 9, 29, 21, 628560142, 0, 0, 0],
        "id": Uuid::nil(),
    });
    let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&legacy).unwrap());

    let decoded = decode_cursor::<UserCursor>(Some(&encoded))
        .unwrap()
        .unwrap();
    assert_eq!(decoded.status, "active");
    assert_eq!(
        decoded.created_at,
        time::macros::datetime!(2026-04-03 09:29:21.628560142 UTC)
    );
}

#[test]
fn rejects_invalid_cursor_payload() {
    let error = decode_cursor::<UserCursor>(Some("not-a-valid-cursor")).unwrap_err();
    assert_eq!(error.to_string(), "invalid cursor");
}

#[test]
fn rejects_unknown_user_status_filter() {
    let error = normalize_user_status(Some("paused")).unwrap_err();
    assert_eq!(
        error.to_string(),
        "status must be one of all, active, disabled"
    );
}

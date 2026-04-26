use super::*;
use serde_json::json;

#[test]
fn submit_request_accepts_three_root_lists() {
    let request: SubmitTweetRequest = serde_json::from_value(json!({
        "users": [],
        "tweets": [],
        "media": []
    }))
    .unwrap();

    assert_eq!(request.users.len(), 0);
    assert_eq!(request.tweets.len(), 0);
    assert_eq!(request.media.len(), 0);
}

#[test]
fn user_registered_at_accepts_legacy_created_at_alias() {
    let request: SubmitTweetRequest = serde_json::from_value(json!({
        "users": [{
            "id": "123",
            "createdAt": "2026-04-10T00:00:00Z"
        }]
    }))
    .unwrap();

    assert!(request.users[0].registered_at.is_some());
}

#[test]
fn summary_counts_partial_results() {
    let response = SubmitTweetResponse {
        users: vec![SubmitObjectResult {
            id: Some("1".to_owned()),
            status: SubmitObjectStatus::Partial,
            operations: Vec::new(),
            error: None,
        }],
        ..Default::default()
    };

    let summary = summarize_results(&response);

    assert_eq!(summary.total, 1);
    assert_eq!(summary.partial, 1);
}

#[test]
fn collect_last_valid_indices_prefers_last_duplicate() {
    let request: SubmitTweetRequest = serde_json::from_value(json!({
        "users": [
            {"id": "1"},
            {"id": "2"},
            {"id": "1"}
        ]
    }))
    .unwrap();

    let indices = collect_last_valid_i64_indices(&request.users, "user.id", |user| &user.id);

    assert_eq!(indices.get(&1), Some(&2));
    assert_eq!(indices.get(&2), Some(&1));
}

#[test]
fn dedupe_media_refs_keeps_first_occurrence() {
    let mut refs = vec![
        TweetMediaRef {
            tweet_id: 1,
            media_id: 10,
            display_order: 0,
        },
        TweetMediaRef {
            tweet_id: 1,
            media_id: 10,
            display_order: 1,
        },
        TweetMediaRef {
            tweet_id: 1,
            media_id: 11,
            display_order: 2,
        },
    ];

    dedupe_media_refs(&mut refs);

    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].media_id, 10);
    assert_eq!(refs[0].display_order, 0);
    assert_eq!(refs[1].media_id, 11);
}

#[test]
fn x_monkey_remote_db_submit_fixture_matches_request_contract() {
    let request: SubmitTweetRequest = serde_json::from_str(include_str!(
        "../../test-fixtures/x_monkey_remote_db_submit_payload.json"
    ))
    .unwrap();

    assert_eq!(request.users.len(), 1);
    assert_eq!(request.tweets.len(), 1);
    assert_eq!(request.media.len(), 1);
    assert_eq!(
        request.tweets[0].edit.as_ref().unwrap().version_ids,
        vec![
            "1912345678901234500".to_owned(),
            "1912345678901234567".to_owned()
        ]
    );
    assert_eq!(request.media[0].availability.as_deref(), Some("Available"));
}

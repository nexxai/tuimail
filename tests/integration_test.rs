use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;

use tuimail::database::{CachedMessage, Database};
use tuimail::state::AppState;
use tuimail::types::Label;

#[tokio::test]
async fn test_no_api_loop_with_fresh_cache() {
    // Setup: Create a real database for this test
    let temp_db_path = format!("sqlite:test_integration_{}.db", Utc::now().timestamp());
    let db = Arc::new(Database::new(&temp_db_path).await.unwrap());

    // Add a label to the database
    let label = Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    };
    db.upsert_label(&label).await.unwrap();

    // Add fresh messages to the cache (just synced)
    let fresh_message = CachedMessage {
        id: "fresh_msg_1".to_string(),
        thread_id: Some("thread_1".to_string()),
        label_ids: vec!["INBOX".to_string()],
        snippet: Some("Fresh message from cache".to_string()),
        subject: Some("Cache Test".to_string()),
        from_addr: Some("cache@example.com".to_string()),
        to_addr: Some("user@example.com".to_string()),
        date_str: Some("Tue, 10 Jun 2025 22:00:00 -0600".to_string()),
        body_text: Some("This message was loaded from cache, not API".to_string()),
        body_html: None,
        received_date: Utc::now(),
        internal_date: Utc::now(),
        is_unread: true,
        is_starred: false,
        cache_timestamp: Utc::now(),
    };

    db.upsert_message(&fresh_message).await.unwrap();

    // Mark as recently synced (fresh cache)
    db.update_sync_state("INBOX", Some("12345")).await.unwrap();

    // Create app state with database
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());
    state.set_database(db.clone());
    state.labels = vec![label];
    state.selected_label = 0;

    let state_arc = Arc::new(RwLock::new(state));

    // Test: Load messages using the fixed spawn_message_fetch_with_cache
    // This should load from cache and NOT call the API
    {
        let mut state_guard = state_arc.write().await;
        state_guard.set_loading_messages(true);
    }

    // Load messages from cache (simulating the fixed behavior)
    let label_id = {
        let state_guard = state_arc.read().await;
        state_guard
            .get_current_label()
            .unwrap()
            .id
            .as_ref()
            .unwrap()
            .clone()
    };

    {
        let mut state_guard = state_arc.write().await;
        // This should load from cache
        let cache_result = state_guard.load_messages_from_cache(&label_id).await;
        assert!(cache_result.is_ok());

        // Check if cache is stale (it shouldn't be)
        let is_stale = state_guard.is_cache_stale(&label_id).await;
        assert!(!is_stale, "Cache should be fresh, not stale");

        state_guard.set_loading_messages(false);
    }

    // Verify: Messages were loaded from cache
    {
        let state_guard = state_arc.read().await;
        assert_eq!(state_guard.messages.len(), 1);
        assert_eq!(state_guard.messages[0].id, Some("fresh_msg_1".to_string()));
        assert_eq!(
            state_guard.messages[0].snippet,
            Some("Fresh message from cache".to_string())
        );

        // Verify headers were loaded from cache
        assert!(state_guard.message_headers.contains_key("fresh_msg_1"));
        let (subject, from) = state_guard.message_headers.get("fresh_msg_1").unwrap();
        assert_eq!(subject, "Cache Test");
        assert_eq!(from, "cache@example.com");

        // Verify body was loaded from cache
        assert!(state_guard.message_bodies.contains_key("fresh_msg_1"));
        let body = state_guard.message_bodies.get("fresh_msg_1").unwrap();
        assert_eq!(body, "This message was loaded from cache, not API");
    }

    // Cleanup
    std::fs::remove_file(temp_db_path.trim_start_matches("sqlite:")).ok();
}

#[tokio::test]
async fn test_cache_staleness_triggers_api_call() {
    // Setup: Create a real database for this test
    let temp_db_path = format!("sqlite:test_stale_{}.db", Utc::now().timestamp());
    let db = Arc::new(Database::new(&temp_db_path).await.unwrap());

    // Add a label to the database
    let label = Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    };
    db.upsert_label(&label).await.unwrap();

    // Add old messages to cache (but don't update sync state, making it appear stale)
    let old_message = CachedMessage {
        id: "old_msg_1".to_string(),
        thread_id: Some("thread_1".to_string()),
        label_ids: vec!["INBOX".to_string()],
        snippet: Some("Old message from cache".to_string()),
        subject: Some("Stale Cache Test".to_string()),
        from_addr: Some("old@example.com".to_string()),
        to_addr: Some("user@example.com".to_string()),
        date_str: Some("Tue, 10 Jun 2025 20:00:00 -0600".to_string()),
        body_text: Some("This message is from stale cache".to_string()),
        body_html: None,
        received_date: Utc::now() - chrono::Duration::hours(2),
        internal_date: Utc::now() - chrono::Duration::hours(2),
        is_unread: true,
        is_starred: false,
        cache_timestamp: Utc::now() - chrono::Duration::hours(2),
    };

    db.upsert_message(&old_message).await.unwrap();

    // Don't update sync state, making cache appear stale

    // Create app state with database
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());
    state.set_database(db.clone());
    state.labels = vec![label];
    state.selected_label = 0;

    let state_arc = Arc::new(RwLock::new(state));

    // Test: Check cache staleness
    let label_id = {
        let state_guard = state_arc.read().await;
        state_guard
            .get_current_label()
            .unwrap()
            .id
            .as_ref()
            .unwrap()
            .clone()
    };

    {
        let state_guard = state_arc.read().await;
        // This should be stale since no sync state exists
        let is_stale = state_guard.is_cache_stale(&label_id).await;
        assert!(is_stale, "Cache should be stale when no sync state exists");
    }

    // In a real scenario, this would trigger an API call
    // For this test, we just verify that the staleness detection works

    // Cleanup
    std::fs::remove_file(temp_db_path.trim_start_matches("sqlite:")).ok();
}

#[tokio::test]
async fn test_frontend_displays_cached_data_immediately() {
    // Setup: Create a real database for this test
    let temp_db_path = format!("sqlite:test_frontend_{}.db", Utc::now().timestamp());
    let db = Arc::new(Database::new(&temp_db_path).await.unwrap());

    // Add a label to the database first
    let label = Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    };
    db.upsert_label(&label).await.unwrap();

    // Add multiple messages with rich content
    let messages = vec![
        CachedMessage {
            id: "msg_1".to_string(),
            thread_id: Some("thread_1".to_string()),
            label_ids: vec!["INBOX".to_string()],
            snippet: Some("Important business proposal".to_string()),
            subject: Some("Business Opportunity".to_string()),
            from_addr: Some("business@company.com".to_string()),
            to_addr: Some("me@example.com".to_string()),
            date_str: Some("Tue, 10 Jun 2025 09:00:00 -0600".to_string()),
            body_text: Some("We have an exciting business opportunity for you...".to_string()),
            body_html: Some(
                "<p>We have an exciting business opportunity for you...</p>".to_string(),
            ),
            received_date: Utc::now() - chrono::Duration::hours(1),
            internal_date: Utc::now() - chrono::Duration::hours(1),
            is_unread: true,
            is_starred: true,
            cache_timestamp: Utc::now(),
        },
        CachedMessage {
            id: "msg_2".to_string(),
            thread_id: Some("thread_2".to_string()),
            label_ids: vec!["INBOX".to_string()],
            snippet: Some("Meeting reminder for tomorrow".to_string()),
            subject: Some("Tomorrow's Meeting".to_string()),
            from_addr: Some("calendar@office.com".to_string()),
            to_addr: Some("me@example.com".to_string()),
            date_str: Some("Tue, 10 Jun 2025 14:30:00 -0600".to_string()),
            body_text: Some("Don't forget about our meeting tomorrow at 2 PM".to_string()),
            body_html: None,
            received_date: Utc::now() - chrono::Duration::minutes(30),
            internal_date: Utc::now() - chrono::Duration::minutes(30),
            is_unread: false,
            is_starred: false,
            cache_timestamp: Utc::now(),
        },
    ];

    for msg in &messages {
        db.upsert_message(msg).await.unwrap();
    }

    // Mark as recently synced
    db.update_sync_state("INBOX", Some("54321")).await.unwrap();

    // Create app state
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());
    state.set_database(db.clone());
    state.labels = vec![Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    }];
    state.selected_label = 0;

    // Test: Load messages from cache
    let cache_result = state.load_messages_from_cache("INBOX").await;
    assert!(cache_result.is_ok());

    // Verify: Frontend has immediate access to all cached data
    assert_eq!(state.messages.len(), 2);

    // Check first message (sorted by internal_date DESC, so msg_2 comes first as it's more recent)
    assert_eq!(state.messages[0].id, Some("msg_2".to_string()));
    assert_eq!(
        state.messages[0].snippet,
        Some("Meeting reminder for tomorrow".to_string())
    );

    // Check second message
    assert_eq!(state.messages[1].id, Some("msg_1".to_string()));
    assert_eq!(
        state.messages[1].snippet,
        Some("Important business proposal".to_string())
    );

    // Check cached headers for msg_1
    assert!(state.message_headers.contains_key("msg_1"));
    let (subject, from) = state.message_headers.get("msg_1").unwrap();
    assert_eq!(subject, "Business Opportunity");
    assert_eq!(from, "business@company.com");

    // Check cached body for msg_1
    assert!(state.message_bodies.contains_key("msg_1"));
    let body = state.message_bodies.get("msg_1").unwrap();
    assert_eq!(body, "We have an exciting business opportunity for you...");

    // Simulate user navigation (selecting first message which is msg_2)
    state.selected_message = 0;
    state.update_message_state();
    state.update_current_message_display_headers();

    // Verify: Headers are updated from cache for msg_2
    assert!(state.current_message_display_headers.is_some());
    let headers = state.current_message_display_headers.as_ref().unwrap();
    assert_eq!(headers.subject, "Tomorrow's Meeting");
    assert_eq!(headers.from, "calendar@office.com");

    // Verify: Body is available from cache for msg_2
    let body2 = state.message_bodies.get("msg_2").unwrap();
    assert_eq!(body2, "Don't forget about our meeting tomorrow at 2 PM");

    // Cleanup
    std::fs::remove_file(temp_db_path.trim_start_matches("sqlite:")).ok();
}

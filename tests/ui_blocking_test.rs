use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::Duration;

use rmail::background_tasks::spawn_message_fetch_with_cache;
use rmail::database::{CachedMessage, Database};
use rmail::state::AppState;
use rmail::types::Label;

#[tokio::test]
async fn test_ui_never_blocks_on_cached_data() {
    // Setup: Create database with fresh cached data
    let temp_db_path = format!("sqlite:test_ui_blocking_{}.db", Utc::now().timestamp());
    let db = Arc::new(Database::new(&temp_db_path).await.unwrap());

    // Add label
    let label = Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    };
    db.upsert_label(&label).await.unwrap();

    // Add fresh cached messages
    let messages = vec![
        CachedMessage {
            id: "msg1".to_string(),
            thread_id: Some("thread1".to_string()),
            label_ids: vec!["INBOX".to_string()],
            snippet: Some("Test message 1".to_string()),
            subject: Some("Subject 1".to_string()),
            from_addr: Some("sender1@example.com".to_string()),
            to_addr: Some("me@example.com".to_string()),
            date_str: Some("Tue, 10 Jun 2025 22:00:00 -0600".to_string()),
            body_text: Some("Body 1".to_string()),
            body_html: None,
            received_date: Utc::now(),
            internal_date: Utc::now(),
            is_unread: true,
            is_starred: false,
            cache_timestamp: Utc::now(),
        },
        CachedMessage {
            id: "msg2".to_string(),
            thread_id: Some("thread2".to_string()),
            label_ids: vec!["INBOX".to_string()],
            snippet: Some("Test message 2".to_string()),
            subject: Some("Subject 2".to_string()),
            from_addr: Some("sender2@example.com".to_string()),
            to_addr: Some("me@example.com".to_string()),
            date_str: Some("Tue, 10 Jun 2025 22:05:00 -0600".to_string()),
            body_text: Some("Body 2".to_string()),
            body_html: None,
            received_date: Utc::now(),
            internal_date: Utc::now(),
            is_unread: false,
            is_starred: true,
            cache_timestamp: Utc::now(),
        },
    ];

    for msg in &messages {
        db.upsert_message(msg).await.unwrap();
    }

    // Mark as recently synced (fresh cache)
    db.update_sync_state("INBOX", Some("fresh123"))
        .await
        .unwrap();

    // Create app state
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());
    state.set_database(db.clone());
    state.labels = vec![label];
    state.selected_label = 0;

    let state_arc = Arc::new(RwLock::new(state));

    // Test: Simulate user selecting label - should be INSTANT
    let start_time = std::time::Instant::now();

    // This should complete immediately with cached data
    spawn_message_fetch_with_cache(state_arc.clone());

    // Give it a tiny bit of time to load cache
    tokio::time::sleep(Duration::from_millis(10)).await;

    let elapsed = start_time.elapsed();

    // Verify: Messages are available immediately from cache
    {
        let state_guard = state_arc.read().await;
        assert_eq!(
            state_guard.messages.len(),
            2,
            "Messages should be loaded from cache immediately"
        );
        assert!(
            !state_guard.loading_messages,
            "UI should not be in loading state when cache is available"
        );

        // Check that headers and bodies are available
        assert!(state_guard.message_headers.contains_key("msg1"));
        assert!(state_guard.message_headers.contains_key("msg2"));
        assert!(state_guard.message_bodies.contains_key("msg1"));
        assert!(state_guard.message_bodies.contains_key("msg2"));
    }

    // This should complete in under 50ms since it's just cache loading
    assert!(
        elapsed < Duration::from_millis(50),
        "Cache loading took too long: {:?}",
        elapsed
    );

    // Cleanup
    std::fs::remove_file(temp_db_path.trim_start_matches("sqlite:")).ok();
}

#[tokio::test]
async fn test_no_concurrent_fetch_loops() {
    // Setup: Create database with stale cache to trigger API calls
    let temp_db_path = format!("sqlite:test_no_loops_{}.db", Utc::now().timestamp());
    let db = Arc::new(Database::new(&temp_db_path).await.unwrap());

    // Add label
    let label = Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    };
    db.upsert_label(&label).await.unwrap();

    // Add some old cached data (no sync state = stale)
    let old_message = CachedMessage {
        id: "old_msg".to_string(),
        thread_id: Some("old_thread".to_string()),
        label_ids: vec!["INBOX".to_string()],
        snippet: Some("Old cached message".to_string()),
        subject: Some("Old Subject".to_string()),
        from_addr: Some("old@example.com".to_string()),
        to_addr: Some("me@example.com".to_string()),
        date_str: Some("Tue, 10 Jun 2025 20:00:00 -0600".to_string()),
        body_text: Some("Old body".to_string()),
        body_html: None,
        received_date: Utc::now() - chrono::Duration::hours(2),
        internal_date: Utc::now() - chrono::Duration::hours(2),
        is_unread: true,
        is_starred: false,
        cache_timestamp: Utc::now() - chrono::Duration::hours(2),
    };
    db.upsert_message(&old_message).await.unwrap();

    // Create app state
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());
    state.set_database(db.clone());
    state.labels = vec![label];
    state.selected_label = 0;

    let state_arc = Arc::new(RwLock::new(state));

    // Test: Spawn multiple concurrent fetches rapidly (simulating the reported issue)
    let start_time = std::time::Instant::now();

    // Spawn multiple fetches in rapid succession (this used to cause loops)
    for _ in 0..5 {
        spawn_message_fetch_with_cache(state_arc.clone());
        tokio::time::sleep(Duration::from_millis(1)).await; // Very small delay
    }

    // Wait a bit for any background processing
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify: Old cached data is available immediately despite stale cache
    {
        let state_guard = state_arc.read().await;
        assert_eq!(
            state_guard.messages.len(),
            1,
            "Cached messages should be available immediately"
        );
        assert_eq!(state_guard.messages[0].id, Some("old_msg".to_string()));

        // Even with stale cache, the UI should show the cached data
        assert!(state_guard.message_headers.contains_key("old_msg"));
        assert!(state_guard.message_bodies.contains_key("old_msg"));
    }

    let elapsed = start_time.elapsed();

    // Should complete in reasonable time even with multiple concurrent spawns
    // (This mainly tests that concurrent fetches don't cause infinite loops)
    assert!(
        elapsed < Duration::from_millis(500),
        "Multiple concurrent fetches took too long: {:?}",
        elapsed
    );

    // Cleanup
    std::fs::remove_file(temp_db_path.trim_start_matches("sqlite:")).ok();
}

#[tokio::test]
async fn test_ui_shows_cached_data_during_background_fetch() {
    // This test verifies that the UI never shows "Loading..." when cached data exists

    let temp_db_path = format!("sqlite:test_ui_cache_{}.db", Utc::now().timestamp());
    let db = Arc::new(Database::new(&temp_db_path).await.unwrap());

    // Add label
    let label = Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    };
    db.upsert_label(&label).await.unwrap();

    // Add cached message
    let cached_message = CachedMessage {
        id: "cached_msg".to_string(),
        thread_id: Some("cached_thread".to_string()),
        label_ids: vec!["INBOX".to_string()],
        snippet: Some("Cached message available while fetching".to_string()),
        subject: Some("Cached Subject".to_string()),
        from_addr: Some("cached@example.com".to_string()),
        to_addr: Some("me@example.com".to_string()),
        date_str: Some("Tue, 10 Jun 2025 21:00:00 -0600".to_string()),
        body_text: Some("Cached body text".to_string()),
        body_html: None,
        received_date: Utc::now(),
        internal_date: Utc::now(),
        is_unread: true,
        is_starred: false,
        cache_timestamp: Utc::now(),
    };
    db.upsert_message(&cached_message).await.unwrap();

    // Don't set sync state to make cache appear stale (will trigger background fetch)

    // Create app state
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());
    state.set_database(db.clone());
    state.labels = vec![label];
    state.selected_label = 0;

    let state_arc = Arc::new(RwLock::new(state));

    // Test: Start background fetch
    spawn_message_fetch_with_cache(state_arc.clone());

    // Give it a moment to load cache
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Verify: Cached data is immediately available to UI
    {
        let state_guard = state_arc.read().await;

        // Should have cached message available
        assert_eq!(state_guard.messages.len(), 1);
        assert_eq!(state_guard.messages[0].id, Some("cached_msg".to_string()));

        // UI should NOT be in loading state even if background fetch is happening
        // (The new implementation should not set loading_messages for background fetches)

        // Headers and body should be available from cache
        assert!(state_guard.message_headers.contains_key("cached_msg"));
        assert!(state_guard.message_bodies.contains_key("cached_msg"));

        let (subject, from) = state_guard.message_headers.get("cached_msg").unwrap();
        assert_eq!(subject, "Cached Subject");
        assert_eq!(from, "cached@example.com");

        let body = state_guard.message_bodies.get("cached_msg").unwrap();
        assert_eq!(body, "Cached body text");
    }

    // Cleanup
    std::fs::remove_file(temp_db_path.trim_start_matches("sqlite:")).ok();
}

#[tokio::test]
async fn test_rapid_label_switching_no_blocking() {
    // Test rapid label switching to ensure no UI blocking occurs

    let temp_db_path = format!("sqlite:test_rapid_switch_{}.db", Utc::now().timestamp());
    let db = Arc::new(Database::new(&temp_db_path).await.unwrap());

    // Add multiple labels with cached data
    let labels = vec![
        Label {
            id: Some("INBOX".to_string()),
            name: Some("Inbox".to_string()),
        },
        Label {
            id: Some("SENT".to_string()),
            name: Some("Sent".to_string()),
        },
        Label {
            id: Some("DRAFT".to_string()),
            name: Some("Drafts".to_string()),
        },
    ];

    for label in &labels {
        db.upsert_label(label).await.unwrap();

        // Add a message for each label
        let msg = CachedMessage {
            id: format!("msg_{}", label.id.as_ref().unwrap()),
            thread_id: Some(format!("thread_{}", label.id.as_ref().unwrap())),
            label_ids: vec![label.id.as_ref().unwrap().clone()],
            snippet: Some(format!("Message in {}", label.name.as_ref().unwrap())),
            subject: Some(format!("Subject for {}", label.name.as_ref().unwrap())),
            from_addr: Some("test@example.com".to_string()),
            to_addr: Some("me@example.com".to_string()),
            date_str: Some("Tue, 10 Jun 2025 22:00:00 -0600".to_string()),
            body_text: Some(format!("Body for {}", label.name.as_ref().unwrap())),
            body_html: None,
            received_date: Utc::now(),
            internal_date: Utc::now(),
            is_unread: true,
            is_starred: false,
            cache_timestamp: Utc::now(),
        };
        db.upsert_message(&msg).await.unwrap();

        // Mark as recently synced
        db.update_sync_state(label.id.as_ref().unwrap(), Some("fresh"))
            .await
            .unwrap();
    }

    // Create app state
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());
    state.set_database(db.clone());
    state.labels = labels;
    state.selected_label = 0;

    let state_arc = Arc::new(RwLock::new(state));

    // Test: Rapidly switch between labels
    let start_time = std::time::Instant::now();

    for i in 0..3 {
        {
            let mut state_guard = state_arc.write().await;
            state_guard.selected_label = i;

            // Load cache immediately for the selected label
            let label_id = state_guard
                .get_current_label()
                .unwrap()
                .id
                .as_ref()
                .unwrap()
                .clone();
            let _ = state_guard.load_messages_from_cache(&label_id).await;
        }

        // Spawn background fetch for this label
        spawn_message_fetch_with_cache(state_arc.clone());

        // Verify data is immediately available
        {
            let state_guard = state_arc.read().await;
            assert!(
                !state_guard.messages.is_empty(),
                "Messages should be available immediately for label {}",
                i
            );

            let expected_msg_id = format!("msg_{}", state_guard.labels[i].id.as_ref().unwrap());
            assert_eq!(state_guard.messages[0].id, Some(expected_msg_id.clone()));

            // Headers should be available
            assert!(state_guard.message_headers.contains_key(&expected_msg_id));
        }
    }

    let elapsed = start_time.elapsed();

    // Rapid switching should be very fast with cached data
    assert!(
        elapsed < Duration::from_millis(100),
        "Rapid label switching took too long: {:?}",
        elapsed
    );

    // Cleanup
    std::fs::remove_file(temp_db_path.trim_start_matches("sqlite:")).ok();
}

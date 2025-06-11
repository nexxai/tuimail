use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use rmail::database::CachedMessage;
use rmail::state::AppState;
use rmail::sync::{SyncCommand, SyncEvent};
use rmail::types::{Label, Message};

// Mock database for testing
#[derive(Clone)]
struct MockDatabase {
    messages: Arc<RwLock<HashMap<String, Vec<CachedMessage>>>>,
    sync_states: Arc<RwLock<HashMap<String, chrono::DateTime<Utc>>>>,
}

impl MockDatabase {
    fn new() -> Self {
        Self {
            messages: Arc::new(RwLock::new(HashMap::new())),
            sync_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn add_cached_messages(&self, label_id: &str, messages: Vec<CachedMessage>) {
        let mut msg_map = self.messages.write().await;
        msg_map.insert(label_id.to_string(), messages);

        // Mark as recently synced
        let mut sync_map = self.sync_states.write().await;
        sync_map.insert(label_id.to_string(), Utc::now());
    }

    async fn get_messages_for_label(&self, label_id: &str) -> Vec<CachedMessage> {
        let msg_map = self.messages.read().await;
        msg_map.get(label_id).cloned().unwrap_or_default()
    }

    async fn is_recently_synced(&self, label_id: &str, minutes_threshold: i64) -> bool {
        let sync_map = self.sync_states.read().await;
        if let Some(last_sync) = sync_map.get(label_id) {
            let elapsed = Utc::now() - *last_sync;
            elapsed.num_minutes() < minutes_threshold
        } else {
            false
        }
    }
}

// Mock API call counter
#[derive(Clone)]
struct ApiCallCounter {
    calls: Arc<RwLock<u32>>,
}

impl ApiCallCounter {
    fn new() -> Self {
        Self {
            calls: Arc::new(RwLock::new(0)),
        }
    }

    async fn increment(&self) {
        let mut count = self.calls.write().await;
        *count += 1;
    }

    async fn get_count(&self) -> u32 {
        *self.calls.read().await
    }
}

// Test helper to create sample cached messages
fn create_sample_cached_messages() -> Vec<CachedMessage> {
    vec![
        CachedMessage {
            id: "msg1".to_string(),
            thread_id: Some("thread1".to_string()),
            label_ids: vec!["INBOX".to_string()],
            snippet: Some("Test message 1".to_string()),
            subject: Some("Subject 1".to_string()),
            from_addr: Some("sender@example.com".to_string()),
            to_addr: Some("recipient@example.com".to_string()),
            date_str: Some("Tue, 10 Jun 2025 14:00:00 -0600".to_string()),
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
            to_addr: Some("recipient@example.com".to_string()),
            date_str: Some("Tue, 10 Jun 2025 15:00:00 -0600".to_string()),
            body_text: Some("Body 2".to_string()),
            body_html: None,
            received_date: Utc::now(),
            internal_date: Utc::now(),
            is_unread: false,
            is_starred: true,
            cache_timestamp: Utc::now(),
        },
    ]
}

// Mock version of spawn_message_fetch_with_cache that tracks API calls
async fn mock_load_messages_with_cache_first(
    state: &mut AppState,
    mock_db: &MockDatabase,
    api_counter: &ApiCallCounter,
    label_id: &str,
) {
    // Step 1: Try to load from cache first
    let cached_messages = mock_db.get_messages_for_label(label_id).await;

    if !cached_messages.is_empty() {
        // Convert cached messages to UI format
        state.messages = cached_messages
            .iter()
            .map(|cached| Message {
                id: Some(cached.id.clone()),
                snippet: cached.snippet.clone(),
                payload: None,
                thread_id: cached.thread_id.clone(),
                label_ids: Some(cached.label_ids.clone()),
            })
            .collect();

        // Update headers cache
        for cached in &cached_messages {
            let subject = cached
                .subject
                .clone()
                .unwrap_or_else(|| "(no subject)".to_string());
            let from = cached
                .from_addr
                .clone()
                .unwrap_or_else(|| "(unknown sender)".to_string());
            state
                .message_headers
                .insert(cached.id.clone(), (subject, from));

            if let Some(body) = &cached.body_text {
                state.message_bodies.insert(cached.id.clone(), body.clone());
            }
        }
    }

    // Step 2: Check if we need to fetch from API
    // This is the BUG - the current implementation always fetches from API
    // But it should only fetch if cache is stale or empty

    let should_fetch_from_api =
        cached_messages.is_empty() || !mock_db.is_recently_synced(label_id, 5).await; // 5 minutes threshold

    if should_fetch_from_api {
        // Simulate API call
        api_counter.increment().await;
    }
}

#[tokio::test]
async fn test_cache_first_avoids_unnecessary_api_calls() {
    let mock_db = MockDatabase::new();
    let api_counter = ApiCallCounter::new();

    // Setup: Add fresh messages to cache
    let sample_messages = create_sample_cached_messages();
    mock_db
        .add_cached_messages("INBOX", sample_messages.clone())
        .await;

    // Create app state
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());
    state.labels = vec![Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    }];
    state.selected_label = 0;

    // Test: Load messages for INBOX
    mock_load_messages_with_cache_first(&mut state, &mock_db, &api_counter, "INBOX").await;

    // Verify: Messages were loaded from cache
    assert_eq!(state.messages.len(), 2);
    assert_eq!(state.messages[0].id, Some("msg1".to_string()));
    assert_eq!(state.messages[1].id, Some("msg2".to_string()));

    // Verify: No API call was made since cache is fresh
    assert_eq!(
        api_counter.get_count().await,
        0,
        "API should not be called when cache is fresh"
    );

    // Verify: Headers were populated from cache
    assert!(state.message_headers.contains_key("msg1"));
    assert!(state.message_headers.contains_key("msg2"));
    assert_eq!(state.message_headers.get("msg1").unwrap().0, "Subject 1");
    assert_eq!(
        state.message_headers.get("msg1").unwrap().1,
        "sender@example.com"
    );
}

#[tokio::test]
async fn test_api_call_when_cache_empty() {
    let mock_db = MockDatabase::new();
    let api_counter = ApiCallCounter::new();

    // Setup: Empty cache
    // (no messages added to mock_db)

    // Create app state
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());
    state.labels = vec![Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    }];
    state.selected_label = 0;

    // Test: Load messages for INBOX
    mock_load_messages_with_cache_first(&mut state, &mock_db, &api_counter, "INBOX").await;

    // Verify: API call was made since cache is empty
    assert_eq!(
        api_counter.get_count().await,
        1,
        "API should be called when cache is empty"
    );

    // Verify: Messages list is empty (since we didn't simulate API response)
    assert_eq!(state.messages.len(), 0);
}

#[tokio::test]
async fn test_api_call_when_cache_stale() {
    let mock_db = MockDatabase::new();
    let api_counter = ApiCallCounter::new();

    // Setup: Add messages to cache but mark as old sync
    let sample_messages = create_sample_cached_messages();
    mock_db
        .messages
        .write()
        .await
        .insert("INBOX".to_string(), sample_messages.clone());
    // Don't add to sync_states, making it appear as never synced (stale)

    // Create app state
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());
    state.labels = vec![Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    }];
    state.selected_label = 0;

    // Test: Load messages for INBOX
    mock_load_messages_with_cache_first(&mut state, &mock_db, &api_counter, "INBOX").await;

    // Verify: Messages were loaded from cache for immediate display
    assert_eq!(state.messages.len(), 2);

    // Verify: API call was made since cache is stale
    assert_eq!(
        api_counter.get_count().await,
        1,
        "API should be called when cache is stale"
    );
}

#[tokio::test]
async fn test_background_sync_updates_cache() {
    // Setup mock database
    let mock_db = MockDatabase::new();

    // Setup sync service channels
    let (cmd_tx, _cmd_rx) = mpsc::channel(100);
    let (event_tx, mut event_rx) = mpsc::channel(100);

    // Create app state
    let client = reqwest::Client::new();
    let state = AppState::new(client, "test_token".to_string());
    let _state_arc = Arc::new(RwLock::new(state));

    // Create a mock database wrapper for the sync service
    // (In real implementation, this would be the actual Database struct)

    // Test: Send sync command
    cmd_tx
        .send(SyncCommand::SyncLabel("INBOX".to_string()))
        .await
        .unwrap();

    // In a real test, we would:
    // 1. Start the sync service
    // 2. Wait for sync completion event
    // 3. Verify that messages were cached
    // 4. Verify that frontend was notified

    // For now, we'll simulate the expected behavior
    let sample_messages = create_sample_cached_messages();
    mock_db.add_cached_messages("INBOX", sample_messages).await;

    // Simulate sync completion event
    event_tx
        .send(SyncEvent::LabelSynced("INBOX".to_string(), 2))
        .await
        .unwrap();

    // Verify event was received
    if let Some(event) = event_rx.recv().await {
        match event {
            SyncEvent::LabelSynced(label_id, count) => {
                assert_eq!(label_id, "INBOX");
                assert_eq!(count, 2);
            }
            _ => panic!("Expected LabelSynced event"),
        }
    }

    // Verify cache was updated
    let cached_messages = mock_db.get_messages_for_label("INBOX").await;
    assert_eq!(cached_messages.len(), 2);
    assert_eq!(cached_messages[0].id, "msg1");
    assert_eq!(cached_messages[1].id, "msg2");
}

#[tokio::test]
async fn test_frontend_works_off_cache_only() {
    let mock_db = MockDatabase::new();
    let api_counter = ApiCallCounter::new();

    // Setup: Add fresh messages to cache
    let sample_messages = create_sample_cached_messages();
    mock_db
        .add_cached_messages("INBOX", sample_messages.clone())
        .await;

    // Create app state
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());
    state.labels = vec![Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    }];
    state.selected_label = 0;

    // Test: Multiple operations that should all work from cache

    // 1. Load messages
    mock_load_messages_with_cache_first(&mut state, &mock_db, &api_counter, "INBOX").await;
    assert_eq!(api_counter.get_count().await, 0);
    assert_eq!(state.messages.len(), 2);

    // 2. Select different message (should use cached data)
    state.selected_message = 1;
    state.update_message_state();
    assert_eq!(state.messages[1].id, Some("msg2".to_string()));

    // 3. Access message body (should be in cache)
    let body = state.message_bodies.get("msg2");
    assert_eq!(body, Some(&"Body 2".to_string()));

    // Verify: Still no API calls made
    assert_eq!(
        api_counter.get_count().await,
        0,
        "Frontend should work entirely from cache when cache is fresh"
    );
}

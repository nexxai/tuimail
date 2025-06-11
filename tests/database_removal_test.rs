use rmail::database::Database;
use rmail::state::AppState;
use rmail::types::Label;
use std::fs;
use std::sync::Arc;

#[tokio::test]
async fn test_app_initialization_with_missing_database() {
    // Remove database if it exists
    let _ = fs::remove_file("test_rmail.db");

    // Verify database doesn't exist
    assert!(!std::path::Path::new("test_rmail.db").exists());

    // Create database - this should create a new empty database
    let db_result = Database::new("sqlite:test_rmail.db").await;
    assert!(
        db_result.is_ok(),
        "Database creation should succeed even when file doesn't exist"
    );

    let db = db_result.unwrap();

    // Verify tables are created and empty
    let labels = db.get_labels().await.unwrap();
    assert!(labels.is_empty(), "Labels should be empty in new database");

    let messages = db.get_messages_for_label("INBOX", 10, 0).await.unwrap();
    assert!(
        messages.is_empty(),
        "Messages should be empty in new database"
    );

    // Cleanup
    let _ = fs::remove_file("test_rmail.db");
}

#[tokio::test]
async fn test_empty_cache_behavior() {
    // Create a test database
    let _ = fs::remove_file("test_empty_cache.db");
    let db = Arc::new(Database::new("sqlite:test_empty_cache.db").await.unwrap());

    // Create app state with empty database
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "fake_token".to_string());
    state.set_database(db.clone());

    // Add a test label to state (simulating API fetch)
    state.labels = vec![Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    }];
    state.selected_label = 0;
    state.update_label_state();

    // Try to load messages from empty cache
    let result = state.load_messages_from_cache("INBOX").await;
    assert!(result.is_ok(), "Loading from empty cache should succeed");
    assert!(
        state.messages.is_empty(),
        "Messages should be empty when cache is empty"
    );

    // Verify cache staleness check
    let is_stale = state.is_cache_stale("INBOX").await;
    assert!(is_stale, "Empty cache should be considered stale");

    // Cleanup
    let _ = fs::remove_file("test_empty_cache.db");
}

#[tokio::test]
async fn test_message_fetch_error_handling() {
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "invalid_token".to_string());

    // Add a test label
    state.labels = vec![Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    }];
    state.selected_label = 0;
    state.update_label_state();

    // Test fetching with invalid token should not crash
    // This will fail the API call but shouldn't panic
    rmail::gmail_api::fetch_messages_for_label(&mut state).await;

    // The state should remain in a consistent state even after API failure
    assert!(
        state.messages.is_empty(),
        "Messages should remain empty after API failure"
    );
    assert_eq!(
        state.selected_message, 0,
        "Selected message should remain 0"
    );
}

#[tokio::test]
async fn test_ui_state_consistency_after_database_removal() {
    // This test simulates the full scenario: database removal and app behavior
    let _ = fs::remove_file("test_ui_consistency.db");

    // Create database and populate it
    let db = Arc::new(
        Database::new("sqlite:test_ui_consistency.db")
            .await
            .unwrap(),
    );

    // Add test data
    let test_label = Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    };
    db.upsert_label(&test_label).await.unwrap();

    // Create app state and load from cache
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "fake_token".to_string());
    state.set_database(db.clone());

    // Load labels from cache
    let result = state.load_labels_from_cache().await;
    assert!(result.is_ok(), "Loading labels from cache should succeed");
    // Note: order_labels() adds ALL MAIL automatically, so we expect 2 labels
    assert_eq!(
        state.labels.len(),
        2,
        "Should have loaded 1 label from cache plus auto-added ALL MAIL"
    );

    // Now simulate database removal and recreation
    drop(db); // Release database handle
    let _ = fs::remove_file("test_ui_consistency.db");

    // Create new empty database
    let new_db = Arc::new(
        Database::new("sqlite:test_ui_consistency.db")
            .await
            .unwrap(),
    );
    state.set_database(new_db);

    // Try to load from now-empty cache
    let result = state.load_labels_from_cache().await;
    assert!(result.is_ok(), "Loading from empty cache should succeed");

    // The issue: labels will be empty now, but app should handle this gracefully
    // After the fix, the app should detect this and refetch from API or show appropriate state

    // Cleanup
    let _ = fs::remove_file("test_ui_consistency.db");
}

#[tokio::test]
async fn test_fetch_messages_for_label_with_empty_response() {
    // Create app state with invalid token to simulate API failure
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "invalid_token".to_string());

    // Add a test label
    state.labels = vec![Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    }];
    state.selected_label = 0;

    // This should return None due to authentication failure
    // The function should handle this gracefully
    rmail::gmail_api::fetch_messages_for_label(&mut state).await;

    // Verify state remains consistent
    assert!(
        state.messages.is_empty(),
        "Messages should be empty after API failure"
    );
    assert_eq!(state.selected_message, 0, "Selected message should be 0");
    assert!(
        state.message_headers.is_empty(),
        "Message headers should be empty"
    );
}

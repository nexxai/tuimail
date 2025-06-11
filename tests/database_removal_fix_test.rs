use rmail::database::Database;
use rmail::gmail_api::fetch_messages_for_label;
use rmail::state::AppState;
use rmail::types::Label;
use std::fs;
use std::sync::Arc;

#[tokio::test]
async fn test_error_message_on_api_failure_after_database_removal() {
    // Remove database if it exists to simulate the user's scenario
    let _ = fs::remove_file("test_error_handling.db");

    // Create a new empty database (simulating what happens after db removal)
    let db = Arc::new(
        Database::new("sqlite:test_error_handling.db")
            .await
            .unwrap(),
    );

    // Create app state with empty database and invalid token
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "invalid_token_will_fail".to_string());
    state.set_database(db.clone());

    // Add a test label to state (simulating successful labels API call)
    state.labels = vec![Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    }];
    state.selected_label = 0;
    state.update_label_state();

    // Try to load messages from empty cache - should succeed but result in empty list
    let result = state.load_messages_from_cache("INBOX").await;
    assert!(result.is_ok(), "Loading from empty cache should succeed");
    assert!(
        state.messages.is_empty(),
        "Messages should be empty when cache is empty"
    );

    // No error message should be set yet
    assert!(
        state.error_message.is_none(),
        "No error message should be set from empty cache"
    );

    // Now try to fetch from API with invalid token - this should fail and set an error message
    fetch_messages_for_label(&mut state).await;

    // After the API failure, an error message should be set
    assert!(
        state.error_message.is_some(),
        "Error message should be set after API failure"
    );
    let error_msg = state.error_message.as_ref().unwrap();
    assert!(
        error_msg.contains("Failed to fetch messages"),
        "Error message should mention fetch failure"
    );

    // Messages should still be empty (no corrupted state)
    assert!(
        state.messages.is_empty(),
        "Messages should remain empty after API failure"
    );
    assert_eq!(
        state.selected_message, 0,
        "Selected message should remain 0"
    );
    assert!(
        state.message_headers.is_empty(),
        "Message headers should remain empty"
    );

    // Cleanup
    let _ = fs::remove_file("test_error_handling.db");
}

#[tokio::test]
async fn test_error_clearing_behavior() {
    // Create app state with an error message
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "token".to_string());

    // Set an error message
    state.set_error_message("Test error message".to_string());
    assert!(state.error_message.is_some(), "Error message should be set");

    // Clear the error message
    state.clear_error_message();
    assert!(
        state.error_message.is_none(),
        "Error message should be cleared"
    );
}

#[tokio::test]
async fn test_database_recreated_after_removal() {
    let db_path = "test_db_recreation.db";

    // Remove database file if it exists
    let _ = fs::remove_file(db_path);

    // Verify the file doesn't exist
    assert!(
        !std::path::Path::new(db_path).exists(),
        "Database file should not exist"
    );

    // Create database - this should recreate the file
    let db = Database::new(&format!("sqlite:{}", db_path)).await.unwrap();

    // Verify the file now exists
    assert!(
        std::path::Path::new(db_path).exists(),
        "Database file should be created"
    );

    // Verify the database is functional
    let labels = db.get_labels().await.unwrap();
    assert!(labels.is_empty(), "New database should have empty labels");

    let messages = db.get_messages_for_label("INBOX", 10, 0).await.unwrap();
    assert!(
        messages.is_empty(),
        "New database should have empty messages"
    );

    // Cleanup
    let _ = fs::remove_file(db_path);
}

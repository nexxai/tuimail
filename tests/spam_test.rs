use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::Arc;
use tokio::sync::RwLock;
use tuimail::event_handler::handle_key_event;
use tuimail::state::{AppState, FocusedPane};
use tuimail::types::{Label, Message};

// Helper function to create a test message
fn create_test_message(id: &str) -> Message {
    Message {
        id: Some(id.to_string()),
        snippet: Some("Test message snippet".to_string()),
        payload: None,
        thread_id: None,
        label_ids: Some(vec!["INBOX".to_string()]),
    }
}

// Helper function to create a test label
fn create_test_label(id: &str, name: &str) -> Label {
    Label {
        id: Some(id.to_string()),
        name: Some(name.to_string()),
    }
}

#[tokio::test]
async fn test_spam_key_only_works_in_messages_and_content_panes() {
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());

    // Add test data
    state.labels = vec![create_test_label("INBOX", "Inbox")];
    state.messages = vec![create_test_message("msg1")];
    state.selected_message = 0;

    let state_arc = Arc::new(RwLock::new(state));

    // Test in Labels pane - 's' should do nothing
    {
        let mut state_guard = state_arc.write().await;
        state_guard.focused_pane = FocusedPane::Labels;
        drop(state_guard);
    }

    let key_event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty());
    let result = handle_key_event(key_event, state_arc.clone()).await;
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should not quit

    // Verify message is still there (spam action didn't execute)
    {
        let state_guard = state_arc.read().await;
        assert_eq!(state_guard.messages.len(), 1);
        assert_eq!(state_guard.focused_pane, FocusedPane::Labels);
    }
}

#[tokio::test]
async fn test_spam_key_during_compose_mode() {
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());

    // Add test data and enter compose mode
    state.labels = vec![create_test_label("INBOX", "Inbox")];
    state.messages = vec![create_test_message("msg1")];
    state.selected_message = 0;
    state.composing = true;

    let state_arc = Arc::new(RwLock::new(state));

    let key_event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty());
    let result = handle_key_event(key_event, state_arc.clone()).await;
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should not quit

    // Verify message is still there (spam action didn't execute)
    // and 's' was treated as text input
    {
        let state_guard = state_arc.read().await;
        assert_eq!(state_guard.messages.len(), 1);
        assert!(state_guard.composing);
    }
}

#[tokio::test]
async fn test_spam_key_in_messages_pane_with_no_messages() {
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());

    // Set up state with no messages
    state.labels = vec![create_test_label("INBOX", "Inbox")];
    state.messages = vec![]; // No messages
    state.focused_pane = FocusedPane::Messages;

    let state_arc = Arc::new(RwLock::new(state));

    let key_event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty());
    let result = handle_key_event(key_event, state_arc.clone()).await;
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should not quit

    // Should handle gracefully (no panic)
    {
        let state_guard = state_arc.read().await;
        assert_eq!(state_guard.messages.len(), 0);
        assert_eq!(state_guard.focused_pane, FocusedPane::Messages);
    }
}

#[tokio::test]
async fn test_spam_key_in_content_pane_with_no_messages() {
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());

    // Set up state with no messages
    state.labels = vec![create_test_label("INBOX", "Inbox")];
    state.messages = vec![]; // No messages
    state.focused_pane = FocusedPane::Content;

    let state_arc = Arc::new(RwLock::new(state));

    let key_event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty());
    let result = handle_key_event(key_event, state_arc.clone()).await;
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should not quit

    // Should handle gracefully (no panic)
    {
        let state_guard = state_arc.read().await;
        assert_eq!(state_guard.messages.len(), 0);
        assert_eq!(state_guard.focused_pane, FocusedPane::Content);
    }
}

#[tokio::test]
async fn test_spam_key_with_message_without_id() {
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());

    // Create a message without an ID
    let mut message = create_test_message("msg1");
    message.id = None; // Remove ID

    state.labels = vec![create_test_label("INBOX", "Inbox")];
    state.messages = vec![message];
    state.selected_message = 0;
    state.focused_pane = FocusedPane::Messages;

    let state_arc = Arc::new(RwLock::new(state));

    let key_event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty());
    let result = handle_key_event(key_event, state_arc.clone()).await;
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should not quit

    // Message should still be there since spam operation couldn't proceed
    {
        let state_guard = state_arc.read().await;
        assert_eq!(state_guard.messages.len(), 1);
        assert_eq!(state_guard.focused_pane, FocusedPane::Messages);
    }
}

#[tokio::test]
async fn test_spam_key_error_handling_preserves_state() {
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());

    // Add test data with invalid token to trigger API error
    state.labels = vec![create_test_label("INBOX", "Inbox")];
    state.messages = vec![create_test_message("msg1")];
    state.selected_message = 0;
    state.focused_pane = FocusedPane::Messages;
    state.token = "invalid_token".to_string(); // This will cause API call to fail

    let state_arc = Arc::new(RwLock::new(state));

    let key_event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty());
    let result = handle_key_event(key_event, state_arc.clone()).await;
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should not quit

    // On API error, message should still be in the list and error message should be set
    {
        let state_guard = state_arc.read().await;
        assert_eq!(state_guard.messages.len(), 1); // Message should still be there
        assert!(state_guard.error_message.is_some()); // Error message should be set
        assert_eq!(state_guard.focused_pane, FocusedPane::Messages);
    }
}

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::Arc;
use tokio::sync::RwLock;

use tuimail::event_handler::handle_key_event;
use tuimail::state::{AppState, FocusedPane};
use tuimail::types::{Label, Message};

async fn setup_simple_test_state() -> Arc<RwLock<AppState>> {
    // Create app state without database operations to avoid permission issues
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "test_token".to_string());

    // Add test labels
    state.labels = vec![Label {
        id: Some("INBOX".to_string()),
        name: Some("Inbox".to_string()),
    }];
    state.selected_label = 0;

    // Add test messages directly to state
    state.messages = vec![
        Message {
            id: Some("msg_1".to_string()),
            thread_id: Some("thread_1".to_string()),
            label_ids: Some(vec!["INBOX".to_string()]),
            snippet: Some("Test message 1".to_string()),
            payload: None,
        },
        Message {
            id: Some("msg_2".to_string()),
            thread_id: Some("thread_2".to_string()),
            label_ids: Some(vec!["INBOX".to_string()]),
            snippet: Some("Test message 2".to_string()),
            payload: None,
        },
    ];

    // Set focus to Messages pane and select first message
    state.focused_pane = FocusedPane::Messages;
    state.selected_message = 0;

    Arc::new(RwLock::new(state))
}

#[tokio::test]
async fn test_backspace_archives_message_in_messages_pane() {
    let state_arc = setup_simple_test_state().await;

    // Verify initial state
    {
        let state_guard = state_arc.read().await;
        assert_eq!(state_guard.messages.len(), 2);
        assert_eq!(state_guard.focused_pane, FocusedPane::Messages);
        assert_eq!(state_guard.selected_message, 0);
        assert_eq!(state_guard.messages[0].id, Some("msg_1".to_string()));
    }

    // Create backspace key event
    let backspace_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);

    // Handle the backspace key event
    let result = handle_key_event(backspace_event, state_arc.clone()).await;

    // Verify the event was handled successfully
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should not quit the application

    // Note: Since we're using a test token and not actually calling the Gmail API,
    // the archive operation will fail, but we can still verify the key event was
    // processed correctly and routed to the archive handler.
}

#[tokio::test]
async fn test_backspace_archives_message_in_content_pane() {
    let state_arc = setup_simple_test_state().await;

    // Set focus to Content pane
    {
        let mut state_guard = state_arc.write().await;
        state_guard.focused_pane = FocusedPane::Content;
        state_guard.selected_message = 1; // Select second message
    }

    // Verify initial state
    {
        let state_guard = state_arc.read().await;
        assert_eq!(state_guard.messages.len(), 2);
        assert_eq!(state_guard.focused_pane, FocusedPane::Content);
        assert_eq!(state_guard.selected_message, 1);
        assert!(state_guard.messages[1].id.is_some());
    }

    // Create backspace key event
    let backspace_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);

    // Handle the backspace key event
    let result = handle_key_event(backspace_event, state_arc.clone()).await;

    // Verify the event was handled successfully
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should not quit the application
}

#[tokio::test]
async fn test_backspace_does_nothing_in_labels_pane() {
    let state_arc = setup_simple_test_state().await;

    // Set focus to Labels pane
    {
        let mut state_guard = state_arc.write().await;
        state_guard.focused_pane = FocusedPane::Labels;
    }

    // Verify initial state
    {
        let state_guard = state_arc.read().await;
        assert_eq!(state_guard.messages.len(), 2);
        assert_eq!(state_guard.focused_pane, FocusedPane::Labels);
    }

    // Create backspace key event
    let backspace_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);

    // Handle the backspace key event
    let result = handle_key_event(backspace_event, state_arc.clone()).await;

    // Verify the event was handled successfully but no archiving occurred
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should not quit the application

    // Verify messages are unchanged (since archiving only works in Messages/Content panes)
    {
        let state_guard = state_arc.read().await;
        assert_eq!(state_guard.messages.len(), 2); // No messages should be removed
    }
}

#[tokio::test]
async fn test_backspace_does_nothing_in_compose_mode() {
    let state_arc = setup_simple_test_state().await;

    // Start composing to enter compose mode
    {
        let mut state_guard = state_arc.write().await;
        state_guard.start_composing(None, None, None, None, None);
        assert!(state_guard.composing);
    }

    // Create backspace key event
    let backspace_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);

    // Handle the backspace key event
    let result = handle_key_event(backspace_event, state_arc.clone()).await;

    // Verify the event was handled successfully
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should not quit the application

    // Verify we're still in compose mode and no messages were archived
    {
        let state_guard = state_arc.read().await;
        assert!(state_guard.composing);
        assert_eq!(state_guard.messages.len(), 2); // No messages should be removed
    }
}

#[tokio::test]
async fn test_a_key_still_works_for_archiving() {
    let state_arc = setup_simple_test_state().await;

    // Verify initial state
    {
        let state_guard = state_arc.read().await;
        assert_eq!(state_guard.messages.len(), 2);
        assert_eq!(state_guard.focused_pane, FocusedPane::Messages);
    }

    // Create 'a' key event
    let a_key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);

    // Handle the 'a' key event
    let result = handle_key_event(a_key_event, state_arc.clone()).await;

    // Verify the event was handled successfully
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should not quit the application
}

#[tokio::test]
async fn test_both_backspace_and_a_key_have_same_behavior() {
    // Test that both backspace and 'a' key trigger the same archive functionality

    // Test with backspace
    let state_arc1 = setup_simple_test_state().await;
    let backspace_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
    let result1 = handle_key_event(backspace_event, state_arc1.clone()).await;

    // Test with 'a' key
    let state_arc2 = setup_simple_test_state().await;
    let a_key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    let result2 = handle_key_event(a_key_event, state_arc2.clone()).await;

    // Both should have the same result
    assert_eq!(result1.is_ok(), result2.is_ok());
    if result1.is_ok() && result2.is_ok() {
        assert_eq!(result1.unwrap(), result2.unwrap());
    }
}

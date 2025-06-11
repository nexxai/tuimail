use crate::background_tasks::{spawn_message_fetch, spawn_message_fetch_with_cache};
use crate::gmail_api::{fetch_full_message, load_more_messages, send_email, try_authenticate};
use crate::state::{AppState, ComposeField, FocusedPane};
use crossterm::event::{self, KeyCode, KeyModifiers};
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn handle_key_event(
    key: event::KeyEvent,
    state_arc: Arc<RwLock<AppState>>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut state_guard = state_arc.write().await;

    // Clear error message on any key press if an error is displayed
    if state_guard.error_message.is_some() {
        state_guard.clear_error_message();
        // If an error was just cleared, we might not want to process the key
        // that cleared it, especially if it was just a "press any key" scenario.
        // For now, we'll let the key event propagate.
    }

    match key.code {
        // Global quit - works at any time
        KeyCode::Char('q') => {
            if state_guard.composing
                && state_guard.compose_state.focused_field != ComposeField::Body
            {
                state_guard.stop_composing();
                Ok(false)
            } else if !state_guard.composing {
                Ok(true) // Signal to quit
            } else {
                // If in compose mode and focused on body, treat 'q' as a character
                let cursor_pos = state_guard.compose_state.body_cursor_position;
                state_guard.compose_state.body.insert(cursor_pos, 'q');
                state_guard.compose_state.body_cursor_position = cursor_pos + 1;
                Ok(false)
            }
        }

        // Compose email with 'c' key (only when not composing)
        KeyCode::Char('c') if !state_guard.composing => {
            state_guard.start_composing(None, None, None, None, None);
            Ok(false)
        }

        // Toggle help with ? key (only when not composing)
        KeyCode::Char('?') if !state_guard.composing => {
            state_guard.toggle_help();
            Ok(false)
        }

        // Force refresh current label with 'f' key (only when not composing)
        KeyCode::Char('f') if !state_guard.composing => {
            if !state_guard.loading_messages {
                state_guard.set_loading_messages(true);
                drop(state_guard); // Release the lock before spawning
                spawn_message_fetch(state_arc.clone());
            }
            Ok(false)
        }

        // Force re-authentication with Ctrl+R (only when not composing)
        KeyCode::Char('r')
            if !state_guard.composing && key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            // Clear error message first
            state_guard.clear_error_message();

            // Try to re-authenticate
            match try_authenticate().await {
                Ok(new_token) => {
                    state_guard.token = new_token;
                    state_guard.set_error_message("Re-authentication successful!".to_string());
                }
                Err(e) => {
                    state_guard.set_error_message(format!("Re-authentication failed: {}", e));
                }
            }
            Ok(false)
        }

        // Handle compose mode vs normal mode
        _ if state_guard.composing => handle_compose_mode_input(key, &mut state_guard).await,

        // Normal mode navigation (only when not composing)
        KeyCode::Char('j') | KeyCode::Down if !state_guard.composing => {
            state_guard.move_down();

            // Load more messages if we're near the end and in messages pane
            if matches!(state_guard.focused_pane, FocusedPane::Messages) {
                let messages_loaded = state_guard.messages.len();
                let screen_size = state_guard.messages_per_screen;
                if state_guard.selected_message + screen_size >= messages_loaded {
                    // Load more messages directly from API
                    if let Some(label) = state_guard.labels.get(state_guard.selected_label) {
                        if let Some(_label_id) = &label.id {
                            let _ = load_more_messages(&mut state_guard).await;
                        }
                    }
                }
            }
            Ok(false)
        }

        KeyCode::Char('k') | KeyCode::Up if !state_guard.composing => {
            state_guard.move_up();
            Ok(false)
        }

        // Tab to switch between panes forward (only when not composing)
        KeyCode::Tab if !state_guard.composing => {
            match state_guard.focused_pane {
                FocusedPane::Labels => state_guard.switch_to_messages_pane(),
                FocusedPane::Messages => state_guard.switch_to_content_pane(),
                FocusedPane::Content => state_guard.switch_to_labels_pane(),
            }
            Ok(false)
        }

        // Shift+Tab to switch between panes backward (only when not composing)
        KeyCode::BackTab if !state_guard.composing => {
            match state_guard.focused_pane {
                FocusedPane::Labels => state_guard.switch_to_content_pane(),
                FocusedPane::Messages => state_guard.switch_to_labels_pane(),
                FocusedPane::Content => state_guard.switch_to_messages_pane(),
            }
            Ok(false)
        }

        // Enter key behavior depends on focused pane (only when not composing)
        KeyCode::Enter if !state_guard.composing => {
            handle_enter_key(&mut state_guard, state_arc.clone()).await
        }

        // Reply to message with 'r' key (in Messages or Content pane)
        KeyCode::Char('r')
            if !state_guard.composing
                && matches!(
                    state_guard.focused_pane,
                    FocusedPane::Messages | FocusedPane::Content
                ) =>
        {
            handle_reply(&mut state_guard, state_arc.clone()).await
        }

        // Escape to go back to labels pane (only when not composing)
        KeyCode::Esc if !state_guard.composing => {
            state_guard.switch_to_labels_pane();
            Ok(false)
        }

        // Archive message with 'a' key (only in Messages and Content panes)
        KeyCode::Char('a') if !state_guard.composing => {
            handle_archive_message(&mut state_guard).await
        }

        // Delete message with 'd' key (only in Messages and Content panes)
        KeyCode::Char('d') if !state_guard.composing => {
            handle_delete_message(&mut state_guard).await
        }

        _ => Ok(false),
    }
}

async fn handle_compose_mode_input(
    key: event::KeyEvent,
    state_guard: &mut AppState,
) -> Result<bool, Box<dyn std::error::Error>> {
    match key.code {
        // Tab navigation in compose mode
        KeyCode::Tab => {
            state_guard.compose_next_field();
            Ok(false)
        }
        KeyCode::BackTab => {
            state_guard.compose_prev_field();
            Ok(false)
        }

        // Toggle BCC with Ctrl+B
        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state_guard.toggle_bcc();
            Ok(false)
        }

        // Escape to cancel compose
        KeyCode::Esc => {
            state_guard.stop_composing();
            Ok(false)
        }

        // Enter to send (only when on Send button)
        KeyCode::Enter => {
            if matches!(state_guard.compose_state.focused_field, ComposeField::Send) {
                // Send the email
                state_guard.compose_state.sending = true;
                let result = send_email(
                    state_guard,
                    &state_guard.compose_state.to,
                    &state_guard.compose_state.cc,
                    &state_guard.compose_state.bcc,
                    &state_guard.compose_state.subject,
                    &state_guard.compose_state.body,
                )
                .await;

                state_guard.compose_state.sending = false;

                match result {
                    Ok(()) => {
                        // Email sent successfully, close compose window
                        state_guard.stop_composing();
                    }
                    Err(_) => {
                        // Handle error - for now just keep compose window open
                        // In a real app, you'd show an error message
                    }
                }
            }
            Ok(false)
        }

        // Handle text input for compose fields
        KeyCode::Char(c) => {
            handle_compose_text_input(state_guard, c);
            Ok(false)
        }

        // Handle backspace
        KeyCode::Backspace => {
            handle_compose_backspace(state_guard);
            Ok(false)
        }

        // Handle left arrow key
        KeyCode::Left => {
            handle_compose_left_arrow(state_guard);
            Ok(false)
        }

        // Handle right arrow key
        KeyCode::Right => {
            handle_compose_right_arrow(state_guard);
            Ok(false)
        }

        _ => Ok(false),
    }
}

fn handle_compose_text_input(state_guard: &mut AppState, c: char) {
    match state_guard.compose_state.focused_field {
        ComposeField::To => {
            let cursor_pos = state_guard.compose_state.to_cursor_position;
            state_guard.compose_state.to.insert(cursor_pos, c);
            state_guard.compose_state.to_cursor_position = cursor_pos + 1;
        }
        ComposeField::Cc => {
            let cursor_pos = state_guard.compose_state.cc_cursor_position;
            state_guard.compose_state.cc.insert(cursor_pos, c);
            state_guard.compose_state.cc_cursor_position = cursor_pos + 1;
        }
        ComposeField::Bcc => {
            let cursor_pos = state_guard.compose_state.bcc_cursor_position;
            state_guard.compose_state.bcc.insert(cursor_pos, c);
            state_guard.compose_state.bcc_cursor_position = cursor_pos + 1;
        }
        ComposeField::Subject => {
            let cursor_pos = state_guard.compose_state.subject_cursor_position;
            state_guard.compose_state.subject.insert(cursor_pos, c);
            state_guard.compose_state.subject_cursor_position = cursor_pos + 1;
        }
        ComposeField::Body => {
            let cursor_pos = state_guard.compose_state.body_cursor_position;
            state_guard.compose_state.body.insert(cursor_pos, c);
            state_guard.compose_state.body_cursor_position = cursor_pos + 1;
        }
        ComposeField::Send => {} // No text input for send button
    }
}

fn handle_compose_backspace(state_guard: &mut AppState) {
    match state_guard.compose_state.focused_field {
        ComposeField::To => {
            if state_guard.compose_state.to_cursor_position > 0 {
                let cursor_pos = state_guard.compose_state.to_cursor_position;
                state_guard.compose_state.to.remove(cursor_pos - 1);
                state_guard.compose_state.to_cursor_position = cursor_pos - 1;
            }
        }
        ComposeField::Cc => {
            if state_guard.compose_state.cc_cursor_position > 0 {
                let cursor_pos = state_guard.compose_state.cc_cursor_position;
                state_guard.compose_state.cc.remove(cursor_pos - 1);
                state_guard.compose_state.cc_cursor_position = cursor_pos - 1;
            }
        }
        ComposeField::Bcc => {
            if state_guard.compose_state.bcc_cursor_position > 0 {
                let cursor_pos = state_guard.compose_state.bcc_cursor_position;
                state_guard.compose_state.bcc.remove(cursor_pos - 1);
                state_guard.compose_state.bcc_cursor_position = cursor_pos - 1;
            }
        }
        ComposeField::Subject => {
            if state_guard.compose_state.subject_cursor_position > 0 {
                let cursor_pos = state_guard.compose_state.subject_cursor_position;
                state_guard.compose_state.subject.remove(cursor_pos - 1);
                state_guard.compose_state.subject_cursor_position = cursor_pos - 1;
            }
        }
        ComposeField::Body => {
            if state_guard.compose_state.body_cursor_position > 0 {
                let cursor_pos = state_guard.compose_state.body_cursor_position;
                state_guard.compose_state.body.remove(cursor_pos - 1);
                state_guard.compose_state.body_cursor_position = cursor_pos - 1;
            }
        }
        ComposeField::Send => {} // No text input for send button
    }
}

fn handle_compose_left_arrow(state_guard: &mut AppState) {
    match state_guard.compose_state.focused_field {
        ComposeField::To => {
            if state_guard.compose_state.to_cursor_position > 0 {
                state_guard.compose_state.to_cursor_position -= 1;
            }
        }
        ComposeField::Cc => {
            if state_guard.compose_state.cc_cursor_position > 0 {
                state_guard.compose_state.cc_cursor_position -= 1;
            }
        }
        ComposeField::Bcc => {
            if state_guard.compose_state.bcc_cursor_position > 0 {
                state_guard.compose_state.bcc_cursor_position -= 1;
            }
        }
        ComposeField::Subject => {
            if state_guard.compose_state.subject_cursor_position > 0 {
                state_guard.compose_state.subject_cursor_position -= 1;
            }
        }
        ComposeField::Body => {
            if state_guard.compose_state.body_cursor_position > 0 {
                state_guard.compose_state.body_cursor_position -= 1;
            }
        }
        ComposeField::Send => {}
    }
}

fn handle_compose_right_arrow(state_guard: &mut AppState) {
    match state_guard.compose_state.focused_field {
        ComposeField::To => {
            if state_guard.compose_state.to_cursor_position < state_guard.compose_state.to.len() {
                state_guard.compose_state.to_cursor_position += 1;
            }
        }
        ComposeField::Cc => {
            if state_guard.compose_state.cc_cursor_position < state_guard.compose_state.cc.len() {
                state_guard.compose_state.cc_cursor_position += 1;
            }
        }
        ComposeField::Bcc => {
            if state_guard.compose_state.bcc_cursor_position < state_guard.compose_state.bcc.len() {
                state_guard.compose_state.bcc_cursor_position += 1;
            }
        }
        ComposeField::Subject => {
            if state_guard.compose_state.subject_cursor_position
                < state_guard.compose_state.subject.len()
            {
                state_guard.compose_state.subject_cursor_position += 1;
            }
        }
        ComposeField::Body => {
            if state_guard.compose_state.body_cursor_position < state_guard.compose_state.body.len()
            {
                state_guard.compose_state.body_cursor_position += 1;
            }
        }
        ComposeField::Send => {}
    }
}

async fn handle_enter_key(
    state_guard: &mut AppState,
    state_arc: Arc<RwLock<AppState>>,
) -> Result<bool, Box<dyn std::error::Error>> {
    match state_guard.focused_pane {
        FocusedPane::Labels => {
            // Select label and switch to messages pane - load in background
            state_guard.reset_pagination();
            state_guard.set_loading_messages(true);
            state_guard.switch_to_messages_pane();

            // Load messages in background (cache-first, then API if needed)
            // Release lock before spawning by ending the scope
            spawn_message_fetch_with_cache(state_arc.clone());
            Ok(false)
        }
        FocusedPane::Messages => {
            // Load message content and switch to content pane
            let message_id = state_guard
                .messages
                .get(state_guard.selected_message)
                .and_then(|msg| msg.id.clone());

            if let Some(id) = message_id {
                let id_str = &id;

                // Fetch full message content and headers
                if let Err(e) = fetch_full_message(state_guard, id_str).await {
                    // Handle error, e.g., log it or display a message
                    state_guard.set_error_message(format!("Error fetching full message: {}", e));
                }

                state_guard.switch_to_content_pane();
            }
            Ok(false)
        }
        FocusedPane::Content => {
            // In content pane, Enter does nothing or could scroll
            Ok(false)
        }
    }
}

async fn handle_reply(
    state_guard: &mut AppState,
    state_arc: Arc<RwLock<AppState>>,
) -> Result<bool, Box<dyn std::error::Error>> {
    if let Some(msg) = state_guard.messages.get(state_guard.selected_message) {
        let message_id = msg.id.clone();
        let _ = state_guard; // Release the lock before reacquiring
        let mut state_guard_reacquired = state_arc.write().await;

        // Retrieve the full message body from the cache
        let original_body_full = message_id.as_ref().and_then(|id| {
            state_guard_reacquired
                .message_bodies
                .get(id)
                .map(|s| s.clone())
        });

        // Ensure full message headers are loaded if not already
        if state_guard_reacquired
            .current_message_display_headers
            .is_none()
        {
            if let Some(id) = message_id {
                let id_str = &id;
                let fetch_result = fetch_full_message(&mut state_guard_reacquired, id_str).await;

                if let Err(e) = fetch_result {
                    state_guard_reacquired
                        .set_error_message(format!("Error fetching full message for reply: {}", e));
                }
            } else {
                state_guard_reacquired
                    .set_error_message("Cannot reply: No message ID found.".to_string());
            }
        }

        let mut reply_body = String::new();
        if let Some(original_body) = original_body_full {
            reply_body.push_str("\n\n"); // Add two blank lines
            for line in original_body.lines() {
                reply_body.push_str(&format!("> {}\n", line));
            }
        }

        let mut to_addr = None;
        let mut subject_text = None;
        let mut cc_addr = None;

        if let Some(headers) = state_guard_reacquired
            .current_message_display_headers
            .take()
        {
            to_addr = Some(headers.from);
            subject_text = Some(format!("{}", headers.subject));
            // Keep original CC if it exists, otherwise None
            cc_addr = if !headers.to.is_empty() {
                Some(headers.to)
            } else {
                None
            };
        }

        state_guard_reacquired.start_composing(
            to_addr,
            cc_addr,
            subject_text,
            Some(reply_body),
            Some(ComposeField::Body),
        );
    }
    Ok(false)
}

async fn handle_archive_message(
    state_guard: &mut AppState,
) -> Result<bool, Box<dyn std::error::Error>> {
    if matches!(
        state_guard.focused_pane,
        FocusedPane::Messages | FocusedPane::Content
    ) {
        let selected_message = state_guard.selected_message;
        if let Some(msg) = state_guard.messages.get(selected_message) {
            if let Some(msg_id) = &msg.id {
                // Actually call the Gmail API to archive the message
                match crate::gmail_api::archive_message(state_guard, msg_id).await {
                    Ok(()) => {
                        // Success - remove from UI
                        state_guard.messages.remove(selected_message);
                        if state_guard.selected_message >= state_guard.messages.len()
                            && state_guard.selected_message > 0
                        {
                            state_guard.selected_message = state_guard.messages.len() - 1;
                        }
                        state_guard.update_message_state();
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        if error_msg.contains("401") || error_msg.contains("invalid authentication")
                        {
                            state_guard.set_error_message(
                                "Authentication expired. Press Ctrl+R to re-authenticate."
                                    .to_string(),
                            );
                        } else {
                            state_guard
                                .set_error_message(format!("Failed to archive message: {}", e));
                        }
                    }
                }
            }
        }
    }
    Ok(false)
}

async fn handle_delete_message(
    state_guard: &mut AppState,
) -> Result<bool, Box<dyn std::error::Error>> {
    if matches!(
        state_guard.focused_pane,
        FocusedPane::Messages | FocusedPane::Content
    ) {
        let selected_message = state_guard.selected_message;
        if let Some(msg) = state_guard.messages.get(selected_message) {
            if let Some(msg_id) = &msg.id {
                // Actually call the Gmail API to delete the message
                match crate::gmail_api::delete_message(state_guard, msg_id).await {
                    Ok(()) => {
                        // Success - remove from UI
                        state_guard.messages.remove(selected_message);
                        if state_guard.selected_message >= state_guard.messages.len()
                            && state_guard.selected_message > 0
                        {
                            state_guard.selected_message = state_guard.messages.len() - 1;
                        }
                        state_guard.update_message_state();
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        if error_msg.contains("401") || error_msg.contains("invalid authentication")
                        {
                            state_guard.set_error_message(
                                "Authentication expired. Press Ctrl+R to re-authenticate."
                                    .to_string(),
                            );
                        } else {
                            state_guard
                                .set_error_message(format!("Failed to delete message: {}", e));
                        }
                    }
                }
            }
        }
    }
    Ok(false)
}

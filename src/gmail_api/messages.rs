use crate::state::AppState;
use crate::types::{Message, MessagesResponse};
use chrono::DateTime;
use chrono::Utc;

pub async fn fetch_messages_for_label(state: &mut AppState) {
    // If not cached, fetch initial batch from API

    // Load 2 screenfuls initially (one visible + one buffer)
    let initial_batch_size = state.messages_per_screen * 2;
    match fetch_messages_for_label_index_paginated(
        state,
        state.selected_label,
        0,
        initial_batch_size,
    )
    .await
    {
        Some(messages) => {
            // Capture the ID of the currently selected message before updating the list
            let current_selected_message_id = state
                .messages
                .get(state.selected_message)
                .and_then(|msg| msg.id.clone());

            state.messages = messages.clone();

            // After updating state.messages, try to find the previously selected message
            if let Some(prev_id) = current_selected_message_id {
                if let Some(new_index) = state
                    .messages
                    .iter()
                    .position(|m| m.id.as_deref() == Some(&prev_id))
                {
                    state.selected_message = new_index;
                } else {
                    // If the previously selected message is no longer in the list, reset to 0
                    state.selected_message = 0;
                }
            } else {
                // If no message was selected, or messages list was empty, default to 0
                state.selected_message = 0;
            }
            state.update_message_state();

            // Extract headers and save to both in-memory cache and database
            for message in &messages {
                if let Some(msg_id) = &message.id {
                    // Extract subject, from, and date from headers if available
                    let mut subject = None;
                    let mut from_addr = None;
                    let mut date_str = None;

                    if let Some(payload) = &message.payload {
                        if let Some(headers) = &payload.headers {
                            subject = headers
                                .iter()
                                .find(|h| h.name.as_deref() == Some("Subject"))
                                .and_then(|h| h.value.clone());

                            from_addr = headers
                                .iter()
                                .find(|h| h.name.as_deref() == Some("From"))
                                .and_then(|h| h.value.clone());

                            date_str = headers
                                .iter()
                                .find(|h| h.name.as_deref() == Some("Date"))
                                .and_then(|h| h.value.clone());
                        }
                    }

                    // Cache headers in memory for immediate display
                    if let (Some(subj), Some(from)) = (&subject, &from_addr) {
                        state
                            .message_headers
                            .insert(msg_id.clone(), (subj.clone(), from.clone()));
                    }

                    // Cache date separately for formatting
                    if let Some(date) = &date_str {
                        state
                            .message_bodies
                            .insert(format!("{}_date", msg_id), date.clone());
                    }

                    // Save to database cache if available
                    if let (Some(db), Some(label)) =
                        (&state.database, state.labels.get(state.selected_label))
                    {
                        if let Some(current_label_id) = &label.id {
                            // For specific labels, only associate with the current label being viewed
                            // For ALLMAIL, use all the message's labels
                            let label_ids = if current_label_id.to_uppercase() == "ALLMAIL" {
                                message.label_ids.clone().unwrap_or_default()
                            } else {
                                vec![current_label_id.clone()]
                            };

                            let cached_message = crate::database::CachedMessage {
                                id: msg_id.clone(),
                                thread_id: message.thread_id.clone(),
                                label_ids,
                                snippet: message.snippet.clone(),
                                subject,
                                from_addr,
                                to_addr: None,
                                date_str: date_str.clone(),
                                body_text: None,
                                body_html: None,
                                received_date: chrono::Utc::now(), // This can still be the current time of caching
                                internal_date: date_str
                                    .clone()
                                    .as_ref()
                                    .and_then(|s| {
                                        DateTime::parse_from_rfc2822(s)
                                            .ok()
                                            .map(|dt| dt.with_timezone(&Utc))
                                    })
                                    .unwrap_or_else(chrono::Utc::now), // Use parsed date or current UTC
                                is_unread: false,  // Placeholder
                                is_starred: false, // Placeholder
                                cache_timestamp: chrono::Utc::now(),
                            };
                            let _ = db.upsert_message(&cached_message).await;
                        }
                    }
                }
            }

            // Update sync state to mark this label as recently synced
            if let (Some(db), Some(label)) =
                (&state.database, state.labels.get(state.selected_label))
            {
                if let Some(label_id) = &label.id {
                    let _ = db.update_sync_state(label_id, None).await;
                }
            }

            // Also save to in-memory cache for compatibility
            state.cache_messages_for_label(state.selected_label, messages);
        }
        None => {
            // Failed to fetch messages from API - this could be due to authentication issues
            // or network problems. Set an error message for the UI to display.
            state.set_error_message(
                "Failed to fetch messages. Please check your internet connection and try again."
                    .to_string(),
            );
        }
    }
}

// Helper function to fetch full message content and headers
pub async fn fetch_full_message(
    state: &mut AppState,
    msg_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let message_url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full",
        msg_id
    );

    let response = state
        .client
        .get(&message_url)
        .bearer_auth(&state.token)
        .send()
        .await?;

    if response.status().is_success() {
        let message: Message = response.json().await?;

        // Extract body content
        let body_text = crate::email_content::extract_plain_text_body(
            &message
                .payload
                .as_ref()
                .unwrap_or(&crate::types::MessagePart::default()),
        )
        .unwrap_or_default();
        let body_html = crate::email_content::extract_html_body(
            &message
                .payload
                .as_ref()
                .unwrap_or(&crate::types::MessagePart::default()),
        );

        // Extract headers for display
        let mut subject = "(no subject)".to_string();
        let mut from = "(unknown sender)".to_string();
        let mut to = "(unknown recipient)".to_string();
        let mut date = "(unknown date)".to_string();

        if let Some(payload) = message.payload.as_ref() {
            if let Some(headers) = &payload.headers {
                subject = headers
                    .iter()
                    .find(|h| h.name.as_deref() == Some("Subject"))
                    .and_then(|h| h.value.clone())
                    .unwrap_or(subject);

                from = headers
                    .iter()
                    .find(|h| h.name.as_deref() == Some("From"))
                    .and_then(|h| h.value.clone())
                    .unwrap_or(from);

                to = headers
                    .iter()
                    .find(|h| h.name.as_deref() == Some("To"))
                    .and_then(|h| h.value.clone())
                    .unwrap_or(to);

                date = headers
                    .iter()
                    .find(|h| h.name.as_deref() == Some("Date"))
                    .and_then(|h| h.value.clone())
                    .unwrap_or(date);
            }
        }

        // Update state with full message body and display headers
        state
            .message_bodies
            .insert(msg_id.to_string(), body_text.clone());

        // Store the original date string for formatting in UI
        state
            .message_bodies
            .insert(format!("{}_date", msg_id), date.clone());

        state.current_message_display_headers = Some(crate::types::MessageHeadersDisplay {
            subject,
            from,
            to,
            date: date.clone(), // Use the original date string here
        });

        // Update database cache if available
        if let Some(db) = &state.database {
            let cached_message = crate::database::CachedMessage {
                id: msg_id.to_string(),
                thread_id: message.thread_id.clone(),
                label_ids: message.label_ids.clone().unwrap_or_default(),
                snippet: message.snippet.clone(),
                subject: state
                    .current_message_display_headers
                    .as_ref()
                    .map(|h| h.subject.clone()),
                from_addr: state
                    .current_message_display_headers
                    .as_ref()
                    .map(|h| h.from.clone()),
                to_addr: state
                    .current_message_display_headers
                    .as_ref()
                    .map(|h| h.to.clone()),
                date_str: Some(date.clone()), // Store the original RFC 2822 date string
                body_text: Some(body_text.clone()),
                body_html: body_html,
                received_date: chrono::Utc::now(),
                internal_date: chrono::Utc::now(), // This will be updated from the actual date header if parsed
                is_unread: false,
                is_starred: false,
                cache_timestamp: chrono::Utc::now(),
            };
            let _ = db.upsert_message(&cached_message).await;
        }

        Ok(())
    } else {
        Err(format!("Failed to fetch full message: {}", response.status()).into())
    }
}

// Load more messages when scrolling near the end
pub async fn load_more_messages(state: &mut AppState) {
    let current_count = state.messages.len();
    let batch_size = state.messages_per_screen;

    if let Some(more_messages) = fetch_messages_for_label_index_paginated(
        state,
        state.selected_label,
        current_count,
        batch_size,
    )
    .await
    {
        if !more_messages.is_empty() {
            state.messages.extend(more_messages.clone());
            // Update cache with new messages
            if let Some(label) = state.labels.get(state.selected_label) {
                if let Some(label_id) = &label.id {
                    state
                        .label_messages_cache
                        .insert(label_id.clone(), state.messages.clone());
                }
            }
        }
    }
}

// Helper function to fetch messages for a specific label index with pagination
async fn fetch_messages_for_label_index_paginated(
    state: &AppState,
    label_index: usize,
    offset: usize,
    limit: usize,
) -> Option<Vec<Message>> {
    if let Some(label) = state.labels.get(label_index) {
        let label_id = label.id.as_deref().unwrap_or("");

        // For "All Mail", don't include labelIds parameter to get all messages
        let messages_url = if label_id.to_uppercase() == "ALLMAIL" {
            format!(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages?maxResults={}&orderBy=date_desc",
                limit + offset
            )
        } else {
            format!(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages?labelIds={}&maxResults={}&orderBy=date_desc",
                label_id,
                limit + offset
            )
        };

        match state
            .client
            .get(&messages_url)
            .bearer_auth(&state.token)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(messages_data) = response.json::<MessagesResponse>().await {
                        let message_refs = messages_data.messages.unwrap_or_default();
                        let mut messages = Vec::new();

                        // Skip messages we already have (offset) and take only what we need
                        for msg_ref in message_refs.iter().skip(offset).take(limit) {
                            if let Some(id) = &msg_ref.id {
                                // Use metadata format to get headers (subject, from) immediately
                                let message_url = format!(
                                    "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=metadata",
                                    id
                                );

                                if let Ok(msg_response) = state
                                    .client
                                    .get(&message_url)
                                    .bearer_auth(&state.token)
                                    .send()
                                    .await
                                {
                                    if msg_response.status().is_success() {
                                        if let Ok(message) = msg_response.json::<Message>().await {
                                            messages.push(message);
                                        }
                                    }
                                }
                            }
                        }
                        return Some(messages);
                    }
                }
            }
            Err(_e) => {
                return None;
            }
        }
    }
    None
}

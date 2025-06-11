use crate::database::Database;
use crate::types::{Label, Message};
use ratatui::widgets::ListState;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, PartialEq, Clone)]
pub enum FocusedPane {
    Labels,
    Messages,
    Content,
}

#[derive(Debug, PartialEq)]
pub enum ComposeField {
    To,
    Cc,
    Bcc,
    Subject,
    Body,
    Send,
}

pub struct ComposeState {
    pub to: String,
    pub to_cursor_position: usize,
    pub cc: String,
    pub cc_cursor_position: usize,
    pub bcc: String,
    pub bcc_cursor_position: usize,
    pub subject: String,
    pub subject_cursor_position: usize,
    pub body: String,
    pub body_cursor_position: usize,
    pub focused_field: ComposeField,
    pub show_bcc: bool,
    pub sending: bool,
}

impl ComposeState {
    pub fn new() -> Self {
        Self {
            to: String::new(),
            to_cursor_position: 0,
            cc: String::new(),
            cc_cursor_position: 0,
            bcc: String::new(),
            bcc_cursor_position: 0,
            subject: String::new(),
            subject_cursor_position: 0,
            body: String::new(),
            body_cursor_position: 0,
            focused_field: ComposeField::To,
            show_bcc: false,
            sending: false,
        }
    }

    pub fn clear(&mut self) {
        self.to.clear();
        self.to_cursor_position = 0;
        self.cc.clear();
        self.cc_cursor_position = 0;
        self.bcc.clear();
        self.bcc_cursor_position = 0;
        self.subject.clear();
        self.subject_cursor_position = 0;
        self.body.clear();
        self.body_cursor_position = 0;
        self.focused_field = ComposeField::To;
        self.show_bcc = false;
        self.sending = false;
    }
}

pub struct AppState {
    pub focused_pane: FocusedPane,
    pub show_help: bool,
    pub loading_messages: bool,
    pub composing: bool,
    pub compose_state: ComposeState,
    pub labels: Vec<Label>,
    pub selected_label: usize,
    pub label_state: ListState,
    pub messages: Vec<Message>,
    pub selected_message: usize,
    pub message_state: ListState,
    pub message_bodies: HashMap<String, String>,
    pub message_headers: HashMap<String, (String, String)>, // msg_id -> (subject, from)
    pub current_message_display_headers: Option<crate::types::MessageHeadersDisplay>,
    pub client: reqwest::Client,
    pub token: String,
    // Cache for preloaded messages by label ID
    pub label_messages_cache: HashMap<String, Vec<Message>>,
    // Track which labels have been loaded
    pub loaded_labels: HashSet<String>,
    // Pagination tracking
    pub messages_per_screen: usize,
    pub current_page: usize,
    // Screen dimensions
    pub screen_height: u16,
    // Content pane scrolling
    pub content_scroll_offset: usize,
    // Database integration
    pub database: Option<Arc<Database>>,
    // Local cache mode
    pub use_local_cache: bool,
    // Error message for display
    pub error_message: Option<String>,
}

impl AppState {
    pub fn new(client: reqwest::Client, token: String) -> Self {
        let mut label_state = ListState::default();
        label_state.select(Some(0));
        let mut message_state = ListState::default();
        message_state.select(Some(0));
        Self {
            focused_pane: FocusedPane::Labels,
            show_help: false,
            loading_messages: false,
            composing: false,
            compose_state: ComposeState::new(),
            labels: vec![],
            selected_label: 0,
            label_state,
            messages: vec![],
            selected_message: 0,
            message_state,
            message_bodies: HashMap::new(),
            message_headers: HashMap::new(),
            current_message_display_headers: None,
            client,
            token,
            label_messages_cache: HashMap::new(),
            loaded_labels: HashSet::new(),
            messages_per_screen: 10, // Default, will be updated based on screen size
            current_page: 0,
            screen_height: 24, // Default, will be updated
            content_scroll_offset: 0,
            database: None,
            use_local_cache: false,
            error_message: None, // Initialize error message as None
        }
    }

    // Set an error message to be displayed
    pub fn set_error_message(&mut self, message: String) {
        self.error_message = Some(message);
    }

    // Clear the current error message
    pub fn clear_error_message(&mut self) {
        self.error_message = None;
    }

    pub fn update_label_state(&mut self) {
        self.label_state.select(Some(self.selected_label));
    }

    pub fn update_message_state(&mut self) {
        self.message_state.select(Some(self.selected_message));
    }

    // Get messages for a label from cache or current messages
    #[allow(dead_code)]
    pub fn get_messages_for_label(&self, label_index: usize) -> Vec<Message> {
        if let Some(label) = self.labels.get(label_index) {
            if let Some(label_id) = &label.id {
                if let Some(cached_messages) = self.label_messages_cache.get(label_id) {
                    return cached_messages.clone();
                }
            }
        }
        if label_index == self.selected_label {
            return self.messages.clone();
        }
        vec![]
    }

    // Check if a label has been loaded
    #[allow(dead_code)]
    pub fn is_label_loaded(&self, label_index: usize) -> bool {
        if let Some(label) = self.labels.get(label_index) {
            if let Some(label_id) = &label.id {
                return self.loaded_labels.contains(label_id);
            }
        }
        false
    }

    // Cache messages for a label
    pub fn cache_messages_for_label(&mut self, label_index: usize, messages: Vec<Message>) {
        if let Some(label) = self.labels.get(label_index) {
            if let Some(label_id) = &label.id {
                self.label_messages_cache.insert(label_id.clone(), messages);
                self.loaded_labels.insert(label_id.clone());
            }
        }
    }

    // Update screen dimensions and calculate messages per screen
    pub fn update_screen_size(&mut self, height: u16) {
        self.screen_height = height;
        // Reserve space for UI elements (borders, headers, etc.)
        // Middle pane gets 40% of width, so we calculate based on height
        let available_height = height.saturating_sub(4); // Reserve for borders and status
        self.messages_per_screen = (available_height as usize).max(5); // Minimum 5 messages
    }

    // Filter out Chat labels and system labels (case-insensitive)
    pub fn filter_labels(&mut self) {
        self.labels.retain(|label| {
            if let Some(name) = &label.name {
                let name_lower = name.to_lowercase();
                // Only filter out obvious chat labels, not all labels containing "chat"
                !name_lower.starts_with("chat/") && name_lower != "chat"
            } else {
                true
            }
        });
    }

    // Reset pagination when changing labels
    pub fn reset_pagination(&mut self) {
        self.current_page = 0;
        self.selected_message = 0;
        self.update_message_state();
        // Reset content scroll when changing messages
        self.content_scroll_offset = 0;
    }

    // Order labels with priority order
    pub fn order_labels(&mut self) {
        let priority_order = vec![
            "INBOX",
            "IMPORTANT",
            "STARRED",
            "SENT",
            "DRAFT",
            "ALLMAIL",
            "SPAM",
            "TRASH",
        ];

        let mut priority_labels = Vec::new();
        let mut other_labels = Vec::new();

        // First, collect priority labels in order
        for priority_id in &priority_order {
            if let Some(pos) = self
                .labels
                .iter()
                .position(|l| l.id.as_deref().unwrap_or("").to_uppercase() == *priority_id)
            {
                priority_labels.push(self.labels.remove(pos));
            }
        }

        // Add "All Mail" if it doesn't exist but we have other labels
        if !priority_labels
            .iter()
            .any(|l| l.id.as_deref().unwrap_or("").to_uppercase() == "ALLMAIL")
            && (!priority_labels.is_empty() || !self.labels.is_empty())
        {
            let all_mail_label = Label {
                id: Some("ALLMAIL".to_string()),
                name: Some("ALL MAIL".to_string()),
            };
            priority_labels.push(all_mail_label);
        }

        // Add remaining labels (user labels, categories, etc.)
        other_labels.append(&mut self.labels);

        // Combine priority labels first, then others
        self.labels = priority_labels;
        self.labels.append(&mut other_labels);
    }

    // Navigation methods for pane-based movement
    pub fn move_up(&mut self) {
        match self.focused_pane {
            FocusedPane::Labels => {
                if self.selected_label > 0 {
                    self.selected_label -= 1;
                    self.update_label_state();
                }
            }
            FocusedPane::Messages => {
                if self.selected_message > 0 {
                    self.selected_message -= 1;
                    self.update_message_state();
                    self.update_current_message_display_headers(); // Update headers on selection change
                }
            }
            FocusedPane::Content => {
                // Scroll up in content pane
                if self.content_scroll_offset > 0 {
                    self.content_scroll_offset -= 1;
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.focused_pane {
            FocusedPane::Labels => {
                if self.selected_label + 1 < self.labels.len() {
                    self.selected_label += 1;
                    self.update_label_state();
                }
            }
            FocusedPane::Messages => {
                if self.selected_message + 1 < self.messages.len() {
                    self.selected_message += 1;
                    self.update_message_state();
                    self.update_current_message_display_headers(); // Update headers on selection change
                }
            }
            FocusedPane::Content => {
                // Scroll down in content pane
                self.content_scroll_offset += 1;
            }
        }
    }

    // Helper to update current_message_display_headers based on selected_message
    pub fn update_current_message_display_headers(&mut self) {
        self.current_message_display_headers = None; // Clear previous headers

        if let Some(current_msg) = self.messages.get(self.selected_message) {
            if let Some(msg_id) = &current_msg.id {
                if let Some((subject, from)) = self.message_headers.get(msg_id) {
                    let date_key = format!("{}_date", msg_id);
                    let date = self
                        .message_bodies
                        .get(&date_key)
                        .cloned()
                        .unwrap_or_else(|| "(unknown date)".to_string());

                    // For 'To' address, we don't cache it in message_headers,
                    // so we'll use a placeholder or fetch it if needed.
                    // For now, use a placeholder.
                    let to = "(unknown recipient)".to_string();

                    self.current_message_display_headers =
                        Some(crate::types::MessageHeadersDisplay {
                            subject: subject.clone(),
                            from: from.clone(),
                            to, // Placeholder for 'To'
                            date,
                        });
                }
            }
        }
    }

    pub fn switch_to_messages_pane(&mut self) {
        self.focused_pane = FocusedPane::Messages;
    }

    pub fn switch_to_labels_pane(&mut self) {
        self.focused_pane = FocusedPane::Labels;
    }

    pub fn switch_to_content_pane(&mut self) {
        self.focused_pane = FocusedPane::Content;
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn set_loading_messages(&mut self, loading: bool) {
        self.loading_messages = loading;
        if loading {
            self.messages.clear();
            self.message_bodies.clear(); // Clear message bodies cache
            self.message_headers.clear(); // Clear message headers cache
            self.current_message_display_headers = None; // Clear display headers
        }
    }

    // Compose email methods
    pub fn start_composing(
        &mut self,
        to: Option<String>,
        cc: Option<String>,
        subject: Option<String>,
        body: Option<String>,
        initial_focus: Option<ComposeField>,
    ) {
        self.composing = true;
        self.compose_state.clear(); // Clear existing state first

        if let Some(t) = to {
            self.compose_state.to = t;
            self.compose_state.to_cursor_position = self.compose_state.to.len();
        }
        if let Some(c) = cc {
            self.compose_state.cc = c;
            self.compose_state.cc_cursor_position = self.compose_state.cc.len();
        }
        if let Some(s) = subject {
            self.compose_state.subject = s;
            self.compose_state.subject_cursor_position = self.compose_state.subject.len();
        }
        if let Some(b) = body {
            self.compose_state.body = b;
            // When replying, the body should start with the cursor at the beginning
            self.compose_state.body_cursor_position = 0;
        }
        self.compose_state.focused_field = initial_focus.unwrap_or(ComposeField::To);
    }

    pub fn stop_composing(&mut self) {
        self.composing = false;
        self.compose_state.clear();
    }

    pub fn compose_next_field(&mut self) {
        use ComposeField::*;
        self.compose_state.focused_field = match self.compose_state.focused_field {
            To => Cc,
            Cc => {
                if self.compose_state.show_bcc {
                    Bcc
                } else {
                    Subject
                }
            }
            Bcc => Subject,
            Subject => Body,
            Body => Send,
            Send => To,
        };
    }

    pub fn compose_prev_field(&mut self) {
        use ComposeField::*;
        self.compose_state.focused_field = match self.compose_state.focused_field {
            To => Send,
            Cc => To,
            Bcc => Cc,
            Subject => {
                if self.compose_state.show_bcc {
                    Bcc
                } else {
                    Cc
                }
            }
            Body => Subject,
            Send => Body,
        };
    }

    pub fn toggle_bcc(&mut self) {
        self.compose_state.show_bcc = !self.compose_state.show_bcc;
        if !self.compose_state.show_bcc && self.compose_state.focused_field == ComposeField::Bcc {
            self.compose_state.focused_field = ComposeField::Subject;
        }
    }

    // Database and sync integration methods
    pub fn set_database(&mut self, database: Arc<Database>) {
        self.database = Some(database);
        self.use_local_cache = true;
    }

    // Load messages from local cache
    pub async fn load_messages_from_cache(
        &mut self,
        label_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(db) = &self.database {
            let cached_messages = db
                .get_messages_for_label(label_id, self.messages_per_screen as i64, 0)
                .await?;

            // Capture the ID of the currently selected message before updating the list
            let current_selected_message_id = self
                .messages
                .get(self.selected_message)
                .and_then(|msg| msg.id.clone());

            // Convert cached messages to Message format for UI compatibility
            self.messages = cached_messages
                .iter()
                .map(|cached| Message {
                    id: Some(cached.id.clone()),
                    snippet: cached.snippet.clone(),
                    payload: None,
                    thread_id: cached.thread_id.clone(),
                    label_ids: Some(cached.label_ids.clone()),
                })
                .collect();

            // After updating self.messages, try to find the previously selected message
            if let Some(prev_id) = current_selected_message_id {
                if let Some(new_index) = self
                    .messages
                    .iter()
                    .position(|m| m.id.as_deref() == Some(&prev_id))
                {
                    self.selected_message = new_index;
                } else {
                    // If the previously selected message is no longer in the list, reset to 0
                    self.selected_message = 0;
                }
            } else {
                // If no message was selected, or messages list was empty, default to 0
                self.selected_message = 0;
            }
            self.update_message_state();

            // Update headers cache - use fallback values if headers are missing
            for cached in &cached_messages {
                let subject = cached
                    .subject
                    .clone()
                    .unwrap_or_else(|| "(no subject)".to_string());
                let from = cached
                    .from_addr
                    .clone()
                    .unwrap_or_else(|| "(unknown sender)".to_string());
                let to = cached
                    .to_addr
                    .clone()
                    .unwrap_or_else(|| "(unknown recipient)".to_string());
                let date = cached
                    .date_str
                    .clone()
                    .unwrap_or_else(|| "(unknown date)".to_string());

                self.message_headers
                    .insert(cached.id.clone(), (subject.clone(), from.clone()));

                // Store the date string in message_bodies with the specific key
                if let Some(date_s) = &cached.date_str {
                    self.message_bodies
                        .insert(format!("{}_date", cached.id), date_s.clone());
                }

                if let Some(body) = &cached.body_text {
                    self.message_bodies.insert(cached.id.clone(), body.clone());
                }

                // Set current message display headers if this is the selected message
                if let Some(current_msg) = self.messages.get(self.selected_message) {
                    if current_msg.id.as_deref() == Some(&cached.id) {
                        self.current_message_display_headers =
                            Some(crate::types::MessageHeadersDisplay {
                                subject,
                                from,
                                to,
                                date,
                            });
                    }
                }
            }
        }
        Ok(())
    }

    // Check if cache is stale for a given label (older than 5 minutes)
    pub async fn is_cache_stale(&self, _label_id: &str) -> bool {
        // Simplified cache staleness check - always consider cache potentially stale
        // In a real implementation, this could track last fetch times
        true
    }

    // Request sync for current label
    #[allow(dead_code)]
    pub async fn sync_current_label(&self) {
        // Note: Label synchronization is now handled by the notification system
        // This method is kept for compatibility but doesn't perform any action
    }

    pub fn get_current_label(&self) -> Option<&Label> {
        self.labels.get(self.selected_label)
    }

    // Load labels from cache
    pub async fn load_labels_from_cache(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(db) = &self.database {
            let cached_labels = db.get_labels().await?;

            // Convert cached labels to Label format
            self.labels = cached_labels
                .iter()
                .map(|cached| Label {
                    id: Some(cached.id.clone()),
                    name: Some(cached.name.clone()),
                })
                .collect();

            // Apply existing filtering and ordering
            self.filter_labels();
            self.order_labels();

            if !self.labels.is_empty() {
                self.selected_label = 0;
                self.update_label_state();
            }
        }
        Ok(())
    }
}

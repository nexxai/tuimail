use chrono::Utc;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};

use crate::database::{CachedMessage, Database};
use crate::gmail_api;
use crate::state::AppState;
use crate::types::Message;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum SyncCommand {
    SyncLabel(String),
    SyncAllLabels,
    ArchiveMessage(String),
    DeleteMessage(String),
    RefreshMessages(String),
    Shutdown,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum SyncEvent {
    LabelSynced(String, usize), // label_id, message_count
    MessageArchived(String),    // message_id
    MessageDeleted(String),     // message_id
    SyncError(String),          // error message
    CacheUpdated,
}

#[allow(dead_code)]
pub struct SyncService {
    db: Arc<Database>,
    command_rx: mpsc::Receiver<SyncCommand>,
    event_tx: mpsc::Sender<SyncEvent>,
    app_state: Arc<RwLock<AppState>>,
}

#[allow(dead_code)]
impl SyncService {
    pub fn new(
        db: Arc<Database>,
        command_rx: mpsc::Receiver<SyncCommand>,
        event_tx: mpsc::Sender<SyncEvent>,
        app_state: Arc<RwLock<AppState>>,
    ) -> Self {
        Self {
            db,
            command_rx,
            event_tx,
            app_state,
        }
    }

    pub async fn run(&mut self) {
        let mut sync_interval = interval(Duration::from_secs(300)); // Sync every 5 minutes

        loop {
            tokio::select! {
                // Handle incoming commands
                Some(command) = self.command_rx.recv() => {
                    match command {
                        SyncCommand::Shutdown => break,
                        _ => self.handle_command(command).await,
                    }
                }

                // Periodic sync
                _ = sync_interval.tick() => {
                    self.handle_command(SyncCommand::SyncAllLabels).await;
                }
            }
        }
    }

    async fn handle_command(&self, command: SyncCommand) {
        match command {
            SyncCommand::SyncLabel(label_id) => {
                self.sync_label(&label_id).await;
            }
            SyncCommand::SyncAllLabels => {
                self.sync_all_labels().await;
            }
            SyncCommand::ArchiveMessage(message_id) => {
                self.archive_message(&message_id).await;
            }
            SyncCommand::DeleteMessage(message_id) => {
                self.delete_message(&message_id).await;
            }
            SyncCommand::RefreshMessages(label_id) => {
                self.refresh_messages(&label_id).await;
            }
            SyncCommand::Shutdown => {}
        }
    }

    async fn sync_label(&self, label_id: &str) {
        let app_state = self.app_state.read().await;

        // Check if we need to sync this label
        if let Ok(sync_state) = self.db.get_sync_state(label_id).await {
            if let Some(state) = sync_state {
                let time_since_sync = Utc::now() - state.last_sync;
                if time_since_sync.num_minutes() < 5 {
                    // Skip if synced recently - send cache updated event to refresh UI with cached data
                    drop(app_state);
                    let _ = self.event_tx.send(SyncEvent::CacheUpdated).await;
                    return;
                }
            }
        }

        // Fetch messages from Gmail API
        let messages = match self
            .fetch_messages_from_api(&app_state, label_id, 100)
            .await
        {
            Ok(messages) => messages,
            Err(_) => {
                let error_msg = format!("Failed to sync {}", label_id);
                let _ = self
                    .event_tx
                    .send(SyncEvent::SyncError(error_msg.clone()))
                    .await;
                let mut app_state_guard = self.app_state.write().await;
                app_state_guard.set_error_message(error_msg);
                return;
            }
        };

        let mut cached_count = 0;
        for message in messages {
            if let Ok(cached_message) = self.convert_to_cached_message(&app_state, &message).await {
                if let Err(e) = self.db.upsert_message(&cached_message).await {
                    let error_msg = format!(
                        "Failed to cache message {}: {}",
                        message.id.as_deref().unwrap_or("unknown"),
                        e
                    );
                    let mut app_state_guard = self.app_state.write().await;
                    app_state_guard.set_error_message(error_msg);
                } else {
                    cached_count += 1;
                }
            }
        }

        // Update sync state
        if let Err(e) = self.db.update_sync_state(label_id, None).await {
            let error_msg = format!("Failed to update sync state for {}: {}", label_id, e);
            let mut app_state_guard = self.app_state.write().await;
            app_state_guard.set_error_message(error_msg);
        }

        // Notify UI
        let _ = self
            .event_tx
            .send(SyncEvent::LabelSynced(label_id.to_string(), cached_count))
            .await;
    }

    async fn sync_all_labels(&self) {
        let app_state = self.app_state.read().await;

        // First, sync labels themselves - handle result immediately to avoid Send issues
        let labels = {
            match gmail_api::fetch_labels(&app_state).await {
                Ok(labels) => labels,
                Err(_) => {
                    let error_msg = "Failed to fetch labels".to_string();
                    let _ = self
                        .event_tx
                        .send(SyncEvent::SyncError(error_msg.clone()))
                        .await;
                    let mut app_state_guard = self.app_state.write().await;
                    app_state_guard.set_error_message(error_msg);
                    return;
                }
            }
        };

        for label in &labels {
            if let Err(e) = self.db.upsert_label(label).await {
                let error_msg = format!(
                    "Failed to cache label {}: {}",
                    label.id.as_deref().unwrap_or("unknown"),
                    e
                );
                let mut app_state_guard = self.app_state.write().await;
                app_state_guard.set_error_message(error_msg);
            }
        }

        // Sync messages for important labels
        let priority_labels = ["INBOX", "IMPORTANT", "SENT", "DRAFT"];
        for label_id in &priority_labels {
            self.sync_label(label_id).await;
        }
    }

    async fn archive_message(&self, message_id: &str) {
        // Optimistically update local cache
        if let Err(e) = self.db.mark_message_archived(message_id).await {
            let error_msg = format!("Failed to mark message as archived locally: {}", e);
            let mut app_state_guard = self.app_state.write().await;
            app_state_guard.set_error_message(error_msg);
            return;
        }

        // Notify UI immediately
        let _ = self
            .event_tx
            .send(SyncEvent::MessageArchived(message_id.to_string()))
            .await;

        // Sync with Gmail API in background
        let app_state = self.app_state.read().await;
        let result = gmail_api::archive_message(&app_state, message_id).await;
        drop(app_state); // Release the read lock

        if let Err(e) = result {
            let error_msg = format!("Failed to archive message on server: {}", e);
            let mut app_state_guard = self.app_state.write().await;
            app_state_guard.set_error_message(error_msg);
            // TODO: Implement retry logic or conflict resolution
        }
    }

    async fn delete_message(&self, message_id: &str) {
        // Optimistically update local cache
        if let Err(e) = self.db.mark_message_deleted(message_id).await {
            let error_msg = format!("Failed to mark message as deleted locally: {}", e);
            let mut app_state_guard = self.app_state.write().await;
            app_state_guard.set_error_message(error_msg);
            return;
        }

        // Notify UI immediately
        let _ = self
            .event_tx
            .send(SyncEvent::MessageDeleted(message_id.to_string()))
            .await;

        // Sync with Gmail API in background
        let app_state = self.app_state.read().await;
        let result = gmail_api::delete_message(&app_state, message_id).await;
        drop(app_state); // Release the read lock

        if let Err(e) = result {
            let error_msg = format!("Failed to delete message on server: {}", e);
            let mut app_state_guard = self.app_state.write().await;
            app_state_guard.set_error_message(error_msg);
            // TODO: Implement retry logic or conflict resolution
        }
    }

    async fn refresh_messages(&self, label_id: &str) {
        // Force refresh by syncing the label
        self.sync_label(label_id).await;
        let _ = self.event_tx.send(SyncEvent::CacheUpdated).await;
    }

    async fn fetch_messages_from_api(
        &self,
        app_state: &AppState,
        label_id: &str,
        limit: usize,
    ) -> Result<Vec<Message>, String> {
        // Create a temporary mutable state for the API call
        let mut temp_state = AppState::new(app_state.client.clone(), app_state.token.clone());
        temp_state.labels = app_state.labels.clone();

        // Find the label index
        let label_index = temp_state
            .labels
            .iter()
            .position(|l| l.id.as_deref().unwrap_or("") == label_id)
            .unwrap_or(0);

        temp_state.selected_label = label_index;
        temp_state.messages_per_screen = limit;

        // Use existing API function
        gmail_api::fetch_messages_for_label(&mut temp_state).await;

        Ok(temp_state.messages)
    }

    async fn convert_to_cached_message(
        &self,
        _app_state: &AppState,
        message: &Message,
    ) -> Result<CachedMessage, String> {
        let message_id = message.id.as_deref().unwrap_or("");

        // Extract headers from message payload if available
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

        // Use snippet as body text if available
        let body_text = message.snippet.clone();

        Ok(CachedMessage {
            id: message_id.to_string(),
            thread_id: message.thread_id.clone(),
            label_ids: message.label_ids.clone().unwrap_or_default(),
            snippet: message.snippet.clone(),
            subject,
            from_addr,
            to_addr: None, // TODO: Extract from headers
            date_str,
            body_text,
            body_html: None,           // TODO: Extract HTML body
            received_date: Utc::now(), // TODO: Parse from headers
            internal_date: Utc::now(), // TODO: Use Gmail's internal date
            is_unread: message
                .label_ids
                .as_ref()
                .map_or(false, |labels| labels.contains(&"UNREAD".to_string())),
            is_starred: message
                .label_ids
                .as_ref()
                .map_or(false, |labels| labels.contains(&"STARRED".to_string())),
            cache_timestamp: Utc::now(),
        })
    }
}

// Helper functions for creating sync channels
pub fn create_sync_channels() -> (
    mpsc::Sender<SyncCommand>,
    mpsc::Receiver<SyncCommand>,
    mpsc::Sender<SyncEvent>,
    mpsc::Receiver<SyncEvent>,
) {
    let (cmd_tx, cmd_rx) = mpsc::channel(100);
    let (event_tx, event_rx) = mpsc::channel(100);
    (cmd_tx, cmd_rx, event_tx, event_rx)
}

// Background task spawner
#[allow(dead_code)]
pub async fn spawn_sync_service(
    _db: Arc<Database>,
    _app_state: Arc<RwLock<AppState>>,
) -> (mpsc::Sender<SyncCommand>, mpsc::Receiver<SyncEvent>) {
    let (cmd_tx, _cmd_rx, _event_tx, event_rx) = create_sync_channels();

    // TODO: Fix Send trait issues with error types
    // let mut sync_service = SyncService::new(db, cmd_rx, event_tx, app_state);
    // tokio::spawn(async move {
    //     sync_service.run().await;
    // });

    (cmd_tx, event_rx)
}

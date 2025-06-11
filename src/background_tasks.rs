use crate::gmail_api::fetch_messages_for_label;
use crate::state::AppState;
use std::sync::Arc;
use tokio::sync::RwLock;

// Helper function to spawn background message fetching with cache-first approach
pub fn spawn_message_fetch_with_cache(state_arc: Arc<RwLock<AppState>>) {
    tokio::spawn(async move {
        let mut state_guard = state_arc.write().await;

        // Get current label ID
        let label_id = state_guard
            .get_current_label()
            .and_then(|label| label.id.clone());

        if let Some(label_id) = label_id {
            // Try cache first for immediate display
            let _cache_loaded = state_guard
                .load_messages_from_cache(&label_id)
                .await
                .is_ok()
                && !state_guard.messages.is_empty();

            // Always fetch from API in background to get fresh data and update cache
            // This ensures we get both fast loading AND fresh data
            fetch_messages_for_label(&mut state_guard).await;

            // If cache wasn't loaded initially, we've now loaded from API
            // If cache was loaded, we've now refreshed with latest data from API
        }

        state_guard.set_loading_messages(false);
    });
}

// Helper function for notification-triggered fetching (API only)
pub fn spawn_message_fetch(state_arc: Arc<RwLock<AppState>>) {
    tokio::spawn(async move {
        let mut state_guard = state_arc.write().await;
        fetch_messages_for_label(&mut state_guard).await;
        state_guard.set_loading_messages(false);
    });
}

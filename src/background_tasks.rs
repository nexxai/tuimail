use crate::gmail_api::fetch_messages_for_label;
use crate::state::AppState;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

// Global set to track ongoing fetches to prevent concurrent duplicates
lazy_static::lazy_static! {
    static ref ONGOING_FETCHES: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
}

// Helper function to spawn background message fetching with cache-first approach
// This function NEVER blocks the UI - it loads cache immediately and fetches in background
pub fn spawn_message_fetch_with_cache(state_arc: Arc<RwLock<AppState>>) {
    tokio::spawn(async move {
        // First, immediately load from cache without setting loading state
        let label_id = {
            let mut state_guard = state_arc.write().await;
            let label_id = state_guard
                .get_current_label()
                .and_then(|label| label.id.clone());

            if let Some(ref label_id) = label_id {
                // Load cache immediately - this never blocks UI
                let _ = state_guard.load_messages_from_cache(label_id).await;
            }
            label_id
        };

        if let Some(label_id) = label_id {
            // Check if we need to fetch from API (without blocking UI)
            let should_fetch = {
                let state_guard = state_arc.read().await;
                state_guard.is_cache_stale(&label_id).await
            };

            if should_fetch {
                // Prevent concurrent fetches for the same label
                {
                    let mut ongoing = ONGOING_FETCHES.lock().await;
                    if ongoing.contains(&label_id) {
                        return; // Another fetch is already in progress
                    }
                    ongoing.insert(label_id.clone());
                }

                // Fetch from API in background without blocking UI
                {
                    let mut state_guard = state_arc.write().await;
                    fetch_messages_for_label(&mut state_guard).await;
                }

                // Remove from ongoing fetches
                {
                    let mut ongoing = ONGOING_FETCHES.lock().await;
                    ongoing.remove(&label_id);
                }
            }
        }
    });
}

// Helper function for notification-triggered fetching (background only, no UI blocking)
pub fn spawn_background_fetch(state_arc: Arc<RwLock<AppState>>) {
    tokio::spawn(async move {
        let label_id = {
            let state_guard = state_arc.read().await;
            state_guard
                .get_current_label()
                .and_then(|label| label.id.clone())
        };

        if let Some(label_id) = label_id {
            // Prevent concurrent fetches
            {
                let mut ongoing = ONGOING_FETCHES.lock().await;
                if ongoing.contains(&label_id) {
                    return; // Another fetch is already in progress
                }
                ongoing.insert(label_id.clone());
            }

            // Fetch in background without affecting UI state
            {
                let mut state_guard = state_arc.write().await;
                fetch_messages_for_label(&mut state_guard).await;
            }

            // Remove from ongoing fetches
            {
                let mut ongoing = ONGOING_FETCHES.lock().await;
                ongoing.remove(&label_id);
            }
        }
    });
}

// Helper function for user-initiated fetching (shows loading state)
pub fn spawn_message_fetch(state_arc: Arc<RwLock<AppState>>) {
    tokio::spawn(async move {
        let mut state_guard = state_arc.write().await;
        fetch_messages_for_label(&mut state_guard).await;
        state_guard.set_loading_messages(false);
    });
}

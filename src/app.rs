use crate::background_tasks::{spawn_background_fetch, spawn_message_fetch_with_cache};
use crate::database::Database;
use crate::gmail_api::{fetch_labels, try_authenticate};
use crate::notifications::{
    self, setup_real_time_notifications, NotificationConfig, NotificationEvent,
};
use crate::state::AppState;
use crate::types::LoadingStage;
use crate::ui::{draw_compose_ui, draw_loading_screen, draw_main_ui};
use ratatui::Terminal;
use std::sync::Arc;
use tokio::sync::{mpsc::Receiver, RwLock};

pub async fn initialize_app(
) -> Result<(Arc<RwLock<AppState>>, Receiver<NotificationEvent>), Box<dyn std::error::Error>> {
    // Initialize database
    let db = Arc::new(Database::new("sqlite:rmail.db").await?);

    // Create initial state
    let client = reqwest::Client::new();
    let mut state = AppState::new(client, "".to_string());

    // Set up database integration
    state.set_database(db.clone());

    // Authenticate
    let token = try_authenticate().await?;
    state.token = token;

    // Initialize notification system
    let state_arc = Arc::new(RwLock::new(state));
    let notification_config = NotificationConfig::default();
    let notification_rx = setup_real_time_notifications(state_arc.clone(), notification_config)
        .await
        .unwrap_or_else(|e| {
            // Cannot use app_state here as it's not initialized yet.
            // Keep eprintln for pre-UI exit.
            eprintln!("Failed to setup notifications: {}", e);
            let (_, rx) = notifications::create_notification_channels();
            rx
        });

    // Try to load labels from cache first, fallback to API
    {
        let mut state_guard = state_arc.write().await;
        let cache_loaded =
            state_guard.load_labels_from_cache().await.is_ok() && !state_guard.labels.is_empty();

        if !cache_loaded {
            // If cache fails or is empty, fetch from API
            match fetch_labels(&state_guard).await {
                Ok(labels) => {
                    state_guard.labels = labels;
                    state_guard.filter_labels();
                    state_guard.order_labels();

                    // Save labels to cache for future use
                    if let Some(db) = &state_guard.database {
                        for label in &state_guard.labels {
                            if let (Some(_id), Some(_name)) = (&label.id, &label.name) {
                                let _ = db.upsert_label(label).await;
                            }
                        }
                    }

                    if !state_guard.labels.is_empty() {
                        state_guard.selected_label = 0;
                        state_guard.update_label_state();
                        state_guard.clear_error_message(); // Clear any previous errors

                        // Load messages for the first label automatically - cache first, no blocking
                        drop(state_guard); // Release the lock before spawning
                        spawn_message_fetch_with_cache(state_arc.clone());
                    } else {
                        state_guard.set_error_message(
                            "No email labels found. Your Gmail account may be empty.".to_string(),
                        );
                        drop(state_guard); // Release the lock
                    }
                }
                Err(e) => {
                    let error_str = e.to_string();
                    // Check if this is an authentication error
                    if error_str.contains("401") || error_str.contains("Unauthorized") {
                        // Authentication error - provide clear instructions to user
                        state_guard.set_error_message(
                            "Authentication expired or invalid. Press Ctrl+R to re-authenticate, or restart the app if the problem persists. Make sure client_secret.json is present and valid.".to_string()
                        );
                        drop(state_guard);
                    } else {
                        // Network or other error
                        state_guard.set_error_message(format!("Failed to load email labels: {}. Please check your internet connection and try restarting the app.", e));
                        drop(state_guard);
                    }
                }
            }
        } else {
            // Cache was loaded successfully, load messages for the selected label
            if !state_guard.labels.is_empty() {
                state_guard.clear_error_message(); // Clear any previous errors
                drop(state_guard); // Release the lock before spawning
                spawn_message_fetch_with_cache(state_arc.clone());
            } else {
                state_guard.set_error_message(
                    "Email labels cache is empty. Please refresh to load from server.".to_string(),
                );
                drop(state_guard); // Release the lock
            }
        }
    }

    Ok((state_arc, notification_rx))
}

pub async fn run_app_loop(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    state_arc: Arc<RwLock<AppState>>,
    mut notification_rx: Receiver<NotificationEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::event_handler::handle_key_event;
    use crossterm::event;

    // Main UI loop with notification event handling
    loop {
        // Handle real-time notifications
        while let Ok(notification) = notification_rx.try_recv() {
            let state_guard = state_arc.write().await;
            match notification {
                NotificationEvent::SyncRequired => {
                    // Fetch in background without blocking UI
                    drop(state_guard); // Release the lock before spawning
                    spawn_background_fetch(state_arc.clone());
                }
            }
        }

        // Draw UI
        {
            let mut state_guard = state_arc.write().await;
            terminal.draw(|f| {
                if state_guard.composing {
                    draw_main_ui(f, &mut state_guard);
                    draw_compose_ui(f, &mut state_guard);
                } else {
                    draw_main_ui(f, &mut state_guard);
                }
            })?;
        }

        // Handle input (navigation, quit, etc.)
        if event::poll(std::time::Duration::from_millis(100))? {
            if let event::Event::Key(key) = event::read()? {
                if handle_key_event(key, state_arc.clone()).await? {
                    break; // Quit signal received
                }
            }
        }
    }

    Ok(())
}

pub fn draw_loading_screens(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    stage: LoadingStage,
) -> Result<(), Box<dyn std::error::Error>> {
    terminal.draw(|f| {
        draw_loading_screen(f, &stage);
    })?;
    Ok(())
}

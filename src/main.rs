mod app;
mod background_tasks;
mod cli;
mod database;
mod email_content;
mod event_handler;
mod gmail_api;
mod notifications;
mod state;
mod sync;
mod terminal;
mod types;
mod ui;

use app::{draw_loading_screens, initialize_app, run_app_loop};
use clap::Parser;
use cli::{handle_keyring_clear, Cli};
use terminal::{cleanup_terminal, setup_terminal};
use types::LoadingStage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.clear_keyring {
        handle_keyring_clear()?;
        return Ok(());
    }

    let mut terminal = setup_terminal()?;

    // Show loading screen for authentication
    draw_loading_screens(&mut terminal, LoadingStage::Authenticating)?;

    // Initialize the application (authentication, database, notifications, labels)
    let (state_arc, notification_rx) = match initialize_app().await {
        Ok((state, rx)) => (state, rx),
        Err(e) => {
            cleanup_terminal(&mut terminal)?;
            // Use eprintln for critical errors before UI is fully set up
            // Cannot use app_state here as it's not initialized yet.
            // Keep eprintln for pre-UI exit.
            eprintln!("Application initialization failed: {}", e);
            return Ok(());
        }
    };

    // Show loading screen for labels
    draw_loading_screens(&mut terminal, LoadingStage::FetchingLabels)?;

    // Run the main application loop
    if let Err(e) = run_app_loop(&mut terminal, state_arc, notification_rx).await {
        cleanup_terminal(&mut terminal)?;
        eprintln!("Application error: {}", e);
        return Ok(());
    }

    cleanup_terminal(&mut terminal)?;
    Ok(())
}

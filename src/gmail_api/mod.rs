//! Gmail API module split into logical submodules
//!
//! This module provides all Gmail API functionality organized into:
//! - auth: Authentication and keyring operations
//! - labels: Label fetching operations
//! - messages: Message fetching and loading
//! - operations: Message actions (send, archive, delete)

pub mod auth;
pub mod labels;
pub mod messages;
pub mod operations;

// Re-export commonly used functions for backwards compatibility
pub use auth::try_authenticate;
pub use labels::fetch_labels;
pub use messages::{fetch_full_message, fetch_messages_for_label, load_more_messages};
pub use operations::{archive_message, delete_message, send_email};

// Re-export auth constants
pub use auth::{KEYRING_SERVICE_NAME, KEYRING_USERNAME};

// Re-export traits for testing (when needed)
#[cfg(test)]
pub use auth::{KeyringEntry, OAuthFlow, RealOAuthFlow};

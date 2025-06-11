use crate::gmail_api::{KEYRING_SERVICE_NAME, KEYRING_USERNAME};
use clap::Parser;
use keyring::Entry;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    /// Clear the stored credentials from the system keyring and exit.
    #[clap(long)]
    pub clear_keyring: bool,
}

pub fn handle_keyring_clear() -> Result<(), Box<dyn std::error::Error>> {
    let credentials_keyring = Entry::new(KEYRING_SERVICE_NAME, KEYRING_USERNAME)?;

    if let Err(e) = credentials_keyring.delete_password() {
        // Cannot use app_state here as it's not initialized yet.
        // Keep eprintln for pre-UI exit.
        eprintln!("Failed to delete credentials from keyring: {}", e);
    } else {
        println!("Credentials removed from keyring. Exiting."); // Keep this one for user feedback
    }
    Ok(())
}

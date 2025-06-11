use async_trait::async_trait;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use yup_oauth2::{ApplicationSecret, InstalledFlowAuthenticator, InstalledFlowReturnMethod};

pub const KEYRING_SERVICE_NAME: &str = "rmail-gmail-credentials";
pub const KEYRING_USERNAME: &str = "default_user"; // Could be user's email if available

#[derive(Serialize, Deserialize, Clone)]
pub struct SecureCredentials {
    pub client_secret: Option<ApplicationSecret>,
    pub token: Option<String>,
}

impl SecureCredentials {
    pub fn new() -> Self {
        Self {
            client_secret: None,
            token: None,
        }
    }

    pub fn with_client_secret(mut self, secret: ApplicationSecret) -> Self {
        self.client_secret = Some(secret);
        self
    }

    pub fn with_token(mut self, token: String) -> Self {
        self.token = Some(token);
        self
    }
}

// Define a trait for Keyring operations to allow mocking
#[cfg_attr(test, mockall::automock)]
pub trait KeyringEntry: Send + Sync {
    fn get_password(&self) -> Result<String, keyring::Error>;
    fn set_password(&self, password: &str) -> Result<(), keyring::Error>;
    fn delete_password(&self) -> Result<(), keyring::Error>;
}

// Implement the trait for the real keyring::Entry
impl KeyringEntry for Entry {
    fn get_password(&self) -> Result<String, keyring::Error> {
        self.get_password()
    }
    fn set_password(&self, password: &str) -> Result<(), keyring::Error> {
        self.set_password(password)
    }
    fn delete_password(&self) -> Result<(), keyring::Error> {
        self.delete_password()
    }
}

// Define a trait for OAuth flow operations to allow mocking
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait OAuthFlow: Send + Sync {
    async fn perform_flow(
        &self,
        secret: ApplicationSecret,
        scopes: Vec<String>,
    ) -> Result<String, Box<dyn std::error::Error>>;
}

// Implement the trait for the real InstalledFlowAuthenticator
pub struct RealOAuthFlow;

#[async_trait]
impl OAuthFlow for RealOAuthFlow {
    async fn perform_flow(
        &self,
        secret: ApplicationSecret,
        scopes: Vec<String>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let auth =
            InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
                .build()
                .await?;
        let scopes_refs: Vec<&str> = scopes.iter().map(|s| s.as_str()).collect();
        let token = auth
            .token(&scopes_refs)
            .await?
            .token()
            .unwrap_or("")
            .to_string();
        Ok(token)
    }
}

// Helper function to load secure credentials from keyring
async fn load_secure_credentials<K: KeyringEntry>(
    credentials_keyring: &K,
) -> Result<SecureCredentials, Box<dyn std::error::Error>> {
    let credentials_json = credentials_keyring.get_password()?;
    let credentials: SecureCredentials = serde_json::from_str(&credentials_json)?;
    Ok(credentials)
}

// Helper function to save secure credentials to keyring
async fn save_secure_credentials<K: KeyringEntry>(
    credentials_keyring: &K,
    credentials: &SecureCredentials,
) -> Result<(), Box<dyn std::error::Error>> {
    let credentials_json = serde_json::to_string(credentials)?;
    credentials_keyring.set_password(&credentials_json)?;
    Ok(())
}

// Helper function to perform the OAuth flow and get a token
async fn perform_oauth_flow<K: KeyringEntry, O: OAuthFlow>(
    oauth_flow_impl: &O,
    secret: ApplicationSecret,
    credentials_keyring: &K,
) -> Result<String, Box<dyn std::error::Error>> {
    let scopes = vec!["https://mail.google.com/".to_string()];
    let token_string = oauth_flow_impl.perform_flow(secret.clone(), scopes).await?;

    // Load existing credentials or create new ones
    let mut credentials = load_secure_credentials(credentials_keyring)
        .await
        .unwrap_or_else(|_| SecureCredentials::new());

    // Update with new token and client secret
    credentials = credentials
        .with_client_secret(secret)
        .with_token(token_string.clone());

    // Save the updated credentials to keyring
    if let Err(e) = save_secure_credentials(credentials_keyring, &credentials).await {
        eprintln!("Failed to save credentials to keyring: {}", e);
    }

    Ok(token_string)
}

// Helper function to load the client secret
async fn load_client_secret<K: KeyringEntry>(
    credentials_keyring: &K,
) -> Result<(ApplicationSecret, bool), Box<dyn std::error::Error>> {
    // Try to load from consolidated credentials first
    if let Ok(credentials) = load_secure_credentials(credentials_keyring).await {
        if let Some(secret) = credentials.client_secret {
            return Ok((secret, false)); // From keyring, not from file
        }
    }

    // Fallback to reading from file
    match yup_oauth2::read_application_secret("client_secret.json").await {
        Ok(secret) => {
            // Save to consolidated credentials
            let mut credentials = load_secure_credentials(credentials_keyring)
                .await
                .unwrap_or_else(|_| SecureCredentials::new());
            credentials = credentials.with_client_secret(secret.clone());

            if let Err(e) = save_secure_credentials(credentials_keyring, &credentials).await {
                eprintln!("Failed to save client secret to keyring: {}", e);
            }
            Ok((secret, true)) // From file
        }
        Err(e) => {
            eprintln!("Failed to read client_secret.json: {}", e);
            eprintln!("Please ensure client_secret.json is in the project root or run `rmail --help` for instructions on how to retrieve it.");
            Err("Client secret not found. Please check README for instructions.".into())
        }
    }
}

// Authentication result with additional info
pub struct AuthResult {
    pub token: String,
    pub client_secret_loaded_from_file: bool,
}

// Main authentication function
pub async fn try_authenticate() -> Result<AuthResult, Box<dyn std::error::Error>> {
    let credentials_keyring = Entry::new(KEYRING_SERVICE_NAME, KEYRING_USERNAME)?;
    let oauth_flow_impl = RealOAuthFlow;

    try_authenticate_internal(&credentials_keyring, &oauth_flow_impl).await
}

async fn try_authenticate_internal<K: KeyringEntry, O: OAuthFlow>(
    credentials_keyring: &K,
    oauth_flow_impl: &O,
) -> Result<AuthResult, Box<dyn std::error::Error>> {
    let mut retry_count = 0;
    let mut client_secret_from_file = false;
    loop {
        let (secret, from_file) = load_client_secret(credentials_keyring).await?; // Load secret
        if from_file {
            client_secret_from_file = true;
        }

        // Try to retrieve token from consolidated credentials first
        if retry_count == 0 {
            if let Ok(credentials) = load_secure_credentials(credentials_keyring).await {
                if let Some(token) = credentials.token {
                    return Ok(AuthResult {
                        token,
                        client_secret_loaded_from_file: client_secret_from_file,
                    }); // Success
                }
            }
        }

        // If no token in keyring or it's a retry attempt, perform OAuth flow
        match perform_oauth_flow(oauth_flow_impl, secret, credentials_keyring).await {
            Ok(token_string) => {
                return Ok(AuthResult {
                    token: token_string,
                    client_secret_loaded_from_file: client_secret_from_file,
                }); // Success
            }
            Err(e) => {
                eprintln!("Authentication failed: {}", e);
                if retry_count == 0 {
                    eprintln!("Attempting to re-authenticate by reloading client_secret.json and clearing keyring...");
                    // Clear credentials from keyring to force re-reading client_secret.json and re-authenticating
                    let _ = credentials_keyring.delete_password();
                    retry_count += 1;
                    continue; // Retry
                } else {
                    eprintln!("Authentication still failed after retry. Please ensure client_secret.json is valid and try again.");
                    return Err("Authentication failed after retry.".into());
                }
            }
        }
    }
}

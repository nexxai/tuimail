use crate::state::AppState;
use crate::types::{Label, LabelsResponse};

// Helper function to fetch labels
pub async fn fetch_labels(state: &AppState) -> Result<Vec<Label>, Box<dyn std::error::Error>> {
    let labels_url = "https://gmail.googleapis.com/gmail/v1/users/me/labels";
    let response = state
        .client
        .get(labels_url)
        .bearer_auth(&state.token)
        .send()
        .await?;

    if response.status().is_success() {
        let labels_data: LabelsResponse = response.json().await?;
        Ok(labels_data.labels.unwrap_or_default())
    } else {
        Err(format!("Failed to fetch labels: {}", response.status()).into())
    }
}

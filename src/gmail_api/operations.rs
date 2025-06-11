use crate::state::AppState;

// Send email using Gmail API
pub async fn send_email(
    state: &AppState,
    to: &str,
    cc: &str,
    bcc: &str,
    subject: &str,
    body: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::engine::Engine;

    // Create email message in RFC 2822 format
    let mut email_content = String::new();

    // Add headers
    email_content.push_str(&format!("To: {}\r\n", to));
    if !cc.is_empty() {
        email_content.push_str(&format!("Cc: {}\r\n", cc));
    }
    if !bcc.is_empty() {
        email_content.push_str(&format!("Bcc: {}\r\n", bcc));
    }
    email_content.push_str(&format!("Subject: {}\r\n", subject));
    email_content.push_str("Content-Type: text/plain; charset=utf-8\r\n");
    email_content.push_str("\r\n");

    // Add body
    email_content.push_str(body);

    // Encode the email content in base64
    let encoded_email = URL_SAFE_NO_PAD.encode(email_content.as_bytes());

    // Create the request body
    let request_body = serde_json::json!({
        "raw": encoded_email
    });

    // Send the email
    let send_url = "https://gmail.googleapis.com/gmail/v1/users/me/messages/send";
    let response = state
        .client
        .post(send_url)
        .bearer_auth(&state.token)
        .json(&request_body)
        .send()
        .await?;

    if response.status().is_success() {
        Ok(())
    } else {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(format!("Failed to send email: {}", error_text).into())
    }
}

// Archive a message by removing the INBOX label
pub async fn archive_message(
    state: &AppState,
    message_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let modify_url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/modify",
        message_id
    );

    let request_body = serde_json::json!({
        "removeLabelIds": ["INBOX"]
    });

    let response = state
        .client
        .post(&modify_url)
        .bearer_auth(&state.token)
        .json(&request_body)
        .send()
        .await?;

    if response.status().is_success() {
        Ok(())
    } else {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(format!("Failed to archive message: {}", error_text).into())
    }
}

// Delete a message by moving it to trash
pub async fn delete_message(
    state: &AppState,
    message_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let trash_url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/trash",
        message_id
    );

    let response = state
        .client
        .post(&trash_url)
        .bearer_auth(&state.token)
        .send()
        .await?;

    if response.status().is_success() {
        Ok(())
    } else {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(format!("Failed to delete message: {}", error_text).into())
    }
}

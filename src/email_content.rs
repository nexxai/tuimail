use crate::types::MessagePart;
use base64::engine::general_purpose::URL_SAFE;
use base64::engine::Engine;

// Extract plain text content specifically
pub fn extract_plain_text_body(payload: &MessagePart) -> Option<String> {
    // Check if this part is plain text
    if let Some(mime_type) = &payload.mime_type {
        if mime_type == "text/plain" {
            if let Some(data) = &payload.body.as_ref().and_then(|b| b.data.as_ref()) {
                if let Ok(decoded) = URL_SAFE.decode(data) {
                    if let Ok(text) = String::from_utf8(decoded) {
                        return Some(text);
                    }
                }
            }
        }
    }

    // Recursively search parts for plain text
    if let Some(parts) = &payload.parts {
        for part in parts {
            if let Some(text) = extract_plain_text_body(part) {
                if !text.trim().is_empty() {
                    return Some(text);
                }
            }
        }
    }

    None
}

// Extract HTML content specifically
pub fn extract_html_body(payload: &MessagePart) -> Option<String> {
    // Check if this part is HTML
    if let Some(mime_type) = &payload.mime_type {
        if mime_type == "text/html" {
            if let Some(data) = &payload.body.as_ref().and_then(|b| b.data.as_ref()) {
                if let Ok(decoded) = URL_SAFE.decode(data) {
                    if let Ok(text) = String::from_utf8(decoded) {
                        return Some(text);
                    }
                }
            }
        }
    }

    // Recursively search parts for HTML
    if let Some(parts) = &payload.parts {
        for part in parts {
            if let Some(text) = extract_html_body(part) {
                if !text.trim().is_empty() {
                    return Some(text);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MessagePartBody;

    fn create_message_part(
        mime_type: &str,
        data: Option<&str>,
        parts: Option<Vec<MessagePart>>,
    ) -> MessagePart {
        MessagePart {
            mime_type: Some(mime_type.to_string()),
            headers: None,
            body: data.map(|d| MessagePartBody {
                data: Some(URL_SAFE.encode(d)),
            }),
            parts,
        }
    }

    #[test]
    fn test_extract_plain_text_body_simple() {
        let payload = create_message_part("text/plain", Some("Hello, world!"), None);
        assert_eq!(
            extract_plain_text_body(&payload),
            Some("Hello, world!".to_string())
        );
    }

    #[test]
    fn test_extract_plain_text_body_nested() {
        let inner_plain = create_message_part("text/plain", Some("Inner plain text."), None);
        let inner_html = create_message_part("text/html", Some("<b>Inner HTML</b>"), None);
        let multipart = create_message_part(
            "multipart/alternative",
            None,
            Some(vec![inner_html, inner_plain]),
        );
        assert_eq!(
            extract_plain_text_body(&multipart),
            Some("Inner plain text.".to_string())
        );
    }

    #[test]
    fn test_extract_plain_text_body_no_plain_text() {
        let inner_html = create_message_part("text/html", Some("<b>Inner HTML</b>"), None);
        let multipart = create_message_part("multipart/alternative", None, Some(vec![inner_html]));
        assert_eq!(extract_plain_text_body(&multipart), None);
    }

    #[test]
    fn test_extract_plain_text_body_empty() {
        let payload = create_message_part("text/plain", Some(""), None);
        assert_eq!(extract_plain_text_body(&payload), Some("".to_string()));
    }

    #[test]
    fn test_extract_html_body_simple() {
        let payload = create_message_part("text/html", Some("<b>Hello, HTML!</b>"), None);
        assert_eq!(
            extract_html_body(&payload),
            Some("<b>Hello, HTML!</b>".to_string())
        );
    }

    #[test]
    fn test_extract_html_body_nested() {
        let inner_plain = create_message_part("text/plain", Some("Inner plain text."), None);
        let inner_html = create_message_part("text/html", Some("<b>Inner HTML</b>"), None);
        let multipart = create_message_part(
            "multipart/alternative",
            None,
            Some(vec![inner_plain, inner_html]),
        );
        assert_eq!(
            extract_html_body(&multipart),
            Some("<b>Inner HTML</b>".to_string())
        );
    }

    #[test]
    fn test_extract_html_body_no_html() {
        let inner_plain = create_message_part("text/plain", Some("Inner plain text."), None);
        let multipart = create_message_part("multipart/alternative", None, Some(vec![inner_plain]));
        assert_eq!(extract_html_body(&multipart), None);
    }

    #[test]
    fn test_extract_html_body_empty() {
        let payload = create_message_part("text/html", Some(""), None);
        assert_eq!(extract_html_body(&payload), Some("".to_string()));
    }
}

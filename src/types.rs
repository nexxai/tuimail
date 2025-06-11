use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct LabelsResponse {
    pub labels: Option<Vec<Label>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Label {
    pub id: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MessagesResponse {
    pub messages: Option<Vec<MessageRef>>,
}

#[derive(Debug, Deserialize)]
pub struct MessageRef {
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Message {
    pub id: Option<String>,
    pub snippet: Option<String>,
    pub payload: Option<MessagePart>,
    #[serde(rename = "threadId")]
    #[allow(dead_code)]
    pub thread_id: Option<String>,
    #[serde(rename = "labelIds")]
    #[allow(dead_code)]
    pub label_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MessagePart {
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    pub headers: Option<Vec<Header>>,
    pub body: Option<MessagePartBody>,
    pub parts: Option<Vec<MessagePart>>,
}

impl Default for MessagePart {
    fn default() -> Self {
        MessagePart {
            mime_type: None,
            headers: None,
            body: None,
            parts: None,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Header {
    pub name: Option<String>,
    pub value: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MessageHeadersDisplay {
    pub subject: String,
    pub from: String,
    pub to: String,
    pub date: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MessagePartBody {
    pub data: Option<String>,
}

#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub enum LoadingStage {
    Authenticating,
    FetchingLabels,
    FetchingMessages,
    Complete,
}

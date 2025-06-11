use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

use crate::types::Label;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMessage {
    pub id: String,
    pub thread_id: Option<String>,
    pub label_ids: Vec<String>,
    pub snippet: Option<String>,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub to_addr: Option<String>,
    pub date_str: Option<String>,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub received_date: DateTime<Utc>,
    pub internal_date: DateTime<Utc>,
    pub is_unread: bool,
    pub is_starred: bool,
    pub cache_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CachedLabel {
    pub id: String,
    pub name: String,
    pub message_count: i64,
    pub unread_count: i64,
    pub last_sync: DateTime<Utc>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SyncState {
    pub label_id: String,
    pub history_id: Option<String>,
    pub last_sync: DateTime<Utc>,
    pub message_count: i64,
}

pub struct Database {
    pool: SqlitePool,
}

#[allow(dead_code)]
impl Database {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        // Use connect_with to ensure the database file is created if it doesn't exist
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(database_url.trim_start_matches("sqlite:"))
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options).await?;
        let db = Database { pool };
        db.create_tables().await?;
        Ok(db)
    }

    async fn create_tables(&self) -> Result<(), sqlx::Error> {
        // Create labels table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS labels (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                message_count INTEGER DEFAULT 0,
                unread_count INTEGER DEFAULT 0,
                last_sync DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create messages table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                thread_id TEXT,
                snippet TEXT,
                subject TEXT,
                from_addr TEXT,
                to_addr TEXT,
                date_str TEXT,
                body_text TEXT,
                body_html TEXT,
                received_date DATETIME,
                internal_date DATETIME,
                is_unread BOOLEAN DEFAULT FALSE,
                is_starred BOOLEAN DEFAULT FALSE,
                cache_timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Add date_str column to existing tables if it doesn't exist
        let _ = sqlx::query("ALTER TABLE messages ADD COLUMN date_str TEXT")
            .execute(&self.pool)
            .await; // Ignore error if column already exists

        // Add to_addr column to existing tables if it doesn't exist
        let _ = sqlx::query("ALTER TABLE messages ADD COLUMN to_addr TEXT")
            .execute(&self.pool)
            .await; // Ignore error if column already exists

        // Create message_labels junction table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS message_labels (
                message_id TEXT,
                label_id TEXT,
                PRIMARY KEY (message_id, label_id),
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE,
                FOREIGN KEY (label_id) REFERENCES labels(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create sync_state table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sync_state (
                label_id TEXT PRIMARY KEY,
                history_id TEXT,
                last_sync DATETIME DEFAULT CURRENT_TIMESTAMP,
                message_count INTEGER DEFAULT 0,
                FOREIGN KEY (label_id) REFERENCES labels(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes for performance
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_messages_received_date ON messages(received_date DESC)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_messages_internal_date ON messages(internal_date DESC)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_unread ON messages(is_unread)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_message_labels_label_id ON message_labels(label_id)",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // Label operations
    pub async fn upsert_label(&self, label: &Label) -> Result<(), sqlx::Error> {
        let id = label.id.as_deref().unwrap_or("");
        let name = label.name.as_deref().unwrap_or("");

        sqlx::query(
            r#"
            INSERT INTO labels (id, name, last_sync)
            VALUES (?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                last_sync = CURRENT_TIMESTAMP
            "#,
        )
        .bind(id)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_labels(&self) -> Result<Vec<CachedLabel>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, message_count, unread_count, last_sync
            FROM labels
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut labels = Vec::new();
        for row in rows {
            labels.push(CachedLabel {
                id: row.get("id"),
                name: row.get("name"),
                message_count: row.get("message_count"),
                unread_count: row.get("unread_count"),
                last_sync: row.get("last_sync"),
            });
        }

        Ok(labels)
    }

    // Message operations
    pub async fn upsert_message(&self, message: &CachedMessage) -> Result<(), sqlx::Error> {
        // Insert/update message
        sqlx::query(
            r#"
            INSERT INTO messages (
                id, thread_id, snippet, subject, from_addr, to_addr, date_str,
                body_text, body_html, received_date, internal_date,
                is_unread, is_starred, cache_timestamp
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                thread_id = excluded.thread_id,
                snippet = excluded.snippet,
                subject = excluded.subject,
                from_addr = excluded.from_addr,
                to_addr = excluded.to_addr,
                date_str = excluded.date_str,
                body_text = excluded.body_text,
                body_html = excluded.body_html,
                received_date = excluded.received_date,
                internal_date = excluded.internal_date,
                is_unread = excluded.is_unread,
                is_starred = excluded.is_starred,
                cache_timestamp = excluded.cache_timestamp
            "#,
        )
        .bind(&message.id)
        .bind(&message.thread_id)
        .bind(&message.snippet)
        .bind(&message.subject)
        .bind(&message.from_addr)
        .bind(&message.to_addr)
        .bind(&message.date_str)
        .bind(&message.body_text)
        .bind(&message.body_html)
        .bind(&message.received_date)
        .bind(&message.internal_date)
        .bind(message.is_unread)
        .bind(message.is_starred)
        .bind(&message.cache_timestamp)
        .execute(&self.pool)
        .await?;

        // Clear existing label associations
        sqlx::query("DELETE FROM message_labels WHERE message_id = ?")
            .bind(&message.id)
            .execute(&self.pool)
            .await?;

        // Insert new label associations
        for label_id in &message.label_ids {
            sqlx::query(
                "INSERT OR IGNORE INTO message_labels (message_id, label_id) VALUES (?, ?)",
            )
            .bind(&message.id)
            .bind(label_id)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn get_messages_for_label(
        &self,
        label_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CachedMessage>, sqlx::Error> {
        let rows = if label_id.to_uppercase() == "ALLMAIL" {
            // For ALLMAIL, get all messages regardless of label
            sqlx::query(
                r#"
                SELECT DISTINCT m.id, m.thread_id, m.snippet, m.subject, m.from_addr, m.to_addr, m.date_str,
                       m.body_text, m.body_html, m.received_date, m.internal_date,
                       m.is_unread, m.is_starred, m.cache_timestamp
                FROM messages m
                ORDER BY m.internal_date DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        } else {
            // For specific labels, only get messages with that label
            sqlx::query(
                r#"
                SELECT DISTINCT m.id, m.thread_id, m.snippet, m.subject, m.from_addr, m.to_addr, m.date_str,
                       m.body_text, m.body_html, m.received_date, m.internal_date,
                       m.is_unread, m.is_starred, m.cache_timestamp
                FROM messages m
                JOIN message_labels ml ON m.id = ml.message_id
                WHERE ml.label_id = ?
                ORDER BY m.internal_date DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(label_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        };

        let mut messages = Vec::new();
        for row in rows {
            let message_id: String = row.get("id");

            // Get label IDs for this message
            let label_rows =
                sqlx::query("SELECT label_id FROM message_labels WHERE message_id = ?")
                    .bind(&message_id)
                    .fetch_all(&self.pool)
                    .await?;

            let label_ids: Vec<String> = label_rows.iter().map(|r| r.get("label_id")).collect();

            messages.push(CachedMessage {
                id: message_id,
                thread_id: row.get("thread_id"),
                label_ids,
                snippet: row.get("snippet"),
                subject: row.get("subject"),
                from_addr: row.get("from_addr"),
                to_addr: row.get("to_addr"),
                date_str: row.get("date_str"),
                body_text: row.get("body_text"),
                body_html: row.get("body_html"),
                received_date: row.get("received_date"),
                internal_date: row.get("internal_date"),
                is_unread: row.get("is_unread"),
                is_starred: row.get("is_starred"),
                cache_timestamp: row.get("cache_timestamp"),
            });
        }

        Ok(messages)
    }

    pub async fn get_message_by_id(
        &self,
        message_id: &str,
    ) -> Result<Option<CachedMessage>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, thread_id, snippet, subject, from_addr, to_addr, date_str,
                   body_text, body_html, received_date, internal_date,
                   is_unread, is_starred, cache_timestamp
            FROM messages
            WHERE id = ?
            "#,
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            // Get label IDs for this message
            let label_rows =
                sqlx::query("SELECT label_id FROM message_labels WHERE message_id = ?")
                    .bind(message_id)
                    .fetch_all(&self.pool)
                    .await?;

            let label_ids: Vec<String> = label_rows.iter().map(|r| r.get("label_id")).collect();

            Ok(Some(CachedMessage {
                id: row.get("id"),
                thread_id: row.get("thread_id"),
                label_ids,
                snippet: row.get("snippet"),
                subject: row.get("subject"),
                from_addr: row.get("from_addr"),
                to_addr: row.get("to_addr"),
                date_str: row.get("date_str"),
                body_text: row.get("body_text"),
                body_html: row.get("body_html"),
                received_date: row.get("received_date"),
                internal_date: row.get("internal_date"),
                is_unread: row.get("is_unread"),
                is_starred: row.get("is_starred"),
                cache_timestamp: row.get("cache_timestamp"),
            }))
        } else {
            Ok(None)
        }
    }

    // Optimistic operations
    pub async fn mark_message_archived(&self, message_id: &str) -> Result<(), sqlx::Error> {
        // Remove INBOX label
        sqlx::query("DELETE FROM message_labels WHERE message_id = ? AND label_id = 'INBOX'")
            .bind(message_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_message_deleted(&self, message_id: &str) -> Result<(), sqlx::Error> {
        // Add TRASH label, remove others
        sqlx::query("DELETE FROM message_labels WHERE message_id = ?")
            .bind(message_id)
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "INSERT OR IGNORE INTO message_labels (message_id, label_id) VALUES (?, 'TRASH')",
        )
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // Sync state operations
    pub async fn update_sync_state(
        &self,
        label_id: &str,
        history_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO sync_state (label_id, history_id, last_sync)
            VALUES (?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(label_id) DO UPDATE SET
                history_id = excluded.history_id,
                last_sync = CURRENT_TIMESTAMP
            "#,
        )
        .bind(label_id)
        .bind(history_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_sync_state(&self, label_id: &str) -> Result<Option<SyncState>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT label_id, history_id, last_sync, message_count FROM sync_state WHERE label_id = ?",
        )
        .bind(label_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(SyncState {
                label_id: row.get("label_id"),
                history_id: row.get("history_id"),
                last_sync: row.get("last_sync"),
                message_count: row.get("message_count"),
            }))
        } else {
            Ok(None)
        }
    }

    // Cleanup operations
    pub async fn cleanup_old_messages(&self, days: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM messages WHERE cache_timestamp < datetime('now', '-' || ? || ' days')",
        )
        .bind(days)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    // Statistics
    pub async fn get_cache_stats(&self) -> Result<HashMap<String, i64>, sqlx::Error> {
        let mut stats = HashMap::new();

        let total_messages: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages")
            .fetch_one(&self.pool)
            .await?;
        stats.insert("total_messages".to_string(), total_messages.0);

        let total_labels: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM labels")
            .fetch_one(&self.pool)
            .await?;
        stats.insert("total_labels".to_string(), total_labels.0);

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use tokio;

    async fn setup_test_db() -> Result<Database, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        let db = Database { pool };
        db.create_tables().await?;
        Ok(db)
    }

    #[tokio::test]
    async fn test_database_creation() {
        let db = setup_test_db().await;
        assert!(db.is_ok());
    }

    #[tokio::test]
    async fn test_upsert_and_get_label() {
        let db = setup_test_db().await.unwrap();
        let label = Label {
            id: Some("INBOX".to_string()),
            name: Some("Inbox".to_string()),
        };

        db.upsert_label(&label).await.unwrap();

        let fetched_labels = db.get_labels().await.unwrap();
        assert_eq!(fetched_labels.len(), 1);
        assert_eq!(fetched_labels[0].id, "INBOX");
        assert_eq!(fetched_labels[0].name, "Inbox");
    }

    #[tokio::test]
    async fn test_upsert_and_get_message() {
        let db = setup_test_db().await.unwrap();

        // Ensure labels exist before upserting message
        db.upsert_label(&Label {
            id: Some("INBOX".to_string()),
            name: Some("Inbox".to_string()),
        })
        .await
        .unwrap();
        db.upsert_label(&Label {
            id: Some("IMPORTANT".to_string()),
            name: Some("Important".to_string()),
        })
        .await
        .unwrap();

        let message = CachedMessage {
            id: "test_msg_1".to_string(),
            thread_id: Some("test_thread_1".to_string()),
            label_ids: vec!["INBOX".to_string(), "IMPORTANT".to_string()],
            snippet: Some("This is a test snippet.".to_string()),
            subject: Some("Test Subject".to_string()),
            from_addr: Some("sender@example.com".to_string()),
            to_addr: Some("recipient@example.com".to_string()),
            date_str: Some("Tue, 10 Jun 2025 14:00:00 -0600".to_string()),
            body_text: Some("This is the plain text body.".to_string()),
            body_html: Some("This is the HTML body.".to_string()),
            received_date: Utc::now(),
            internal_date: Utc::now(),
            is_unread: true,
            is_starred: false,
            cache_timestamp: Utc::now(),
        };

        db.upsert_message(&message).await.unwrap();

        let fetched_message = db.get_message_by_id("test_msg_1").await.unwrap().unwrap();
        assert_eq!(fetched_message.id, "test_msg_1");
        assert_eq!(fetched_message.subject, Some("Test Subject".to_string()));
        assert!(fetched_message.label_ids.contains(&"INBOX".to_string()));
        assert!(fetched_message.label_ids.contains(&"IMPORTANT".to_string()));

        let messages_inbox = db.get_messages_for_label("INBOX", 10, 0).await.unwrap();
        assert_eq!(messages_inbox.len(), 1);
        assert_eq!(messages_inbox[0].id, "test_msg_1");

        let messages_allmail = db.get_messages_for_label("ALLMAIL", 10, 0).await.unwrap();
        assert_eq!(messages_allmail.len(), 1);
        assert_eq!(messages_allmail[0].id, "test_msg_1");
    }

    #[tokio::test]
    async fn test_mark_message_archived() {
        let db = setup_test_db().await.unwrap();

        // Ensure labels exist
        db.upsert_label(&Label {
            id: Some("INBOX".to_string()),
            name: Some("Inbox".to_string()),
        })
        .await
        .unwrap();
        db.upsert_label(&Label {
            id: Some("SENT".to_string()),
            name: Some("Sent".to_string()),
        })
        .await
        .unwrap();

        let message = CachedMessage {
            id: "archive_msg_1".to_string(),
            thread_id: Some("archive_thread_1".to_string()),
            label_ids: vec!["INBOX".to_string(), "SENT".to_string()],
            snippet: Some("Snippet.".to_string()),
            subject: Some("Subject.".to_string()),
            from_addr: Some("from@example.com".to_string()),
            to_addr: Some("to@example.com".to_string()),
            date_str: Some("Date.".to_string()),
            body_text: Some("Body.".to_string()),
            body_html: None,
            received_date: Utc::now(),
            internal_date: Utc::now(),
            is_unread: true,
            is_starred: false,
            cache_timestamp: Utc::now(),
        };
        db.upsert_message(&message).await.unwrap();

        db.mark_message_archived("archive_msg_1").await.unwrap();

        let fetched_message = db
            .get_message_by_id("archive_msg_1")
            .await
            .unwrap()
            .unwrap();
        assert!(!fetched_message.label_ids.contains(&"INBOX".to_string()));
        assert!(fetched_message.label_ids.contains(&"SENT".to_string()));
    }

    #[tokio::test]
    async fn test_mark_message_deleted() {
        let db = setup_test_db().await.unwrap();

        // Ensure labels exist
        db.upsert_label(&Label {
            id: Some("INBOX".to_string()),
            name: Some("Inbox".to_string()),
        })
        .await
        .unwrap();
        db.upsert_label(&Label {
            id: Some("SENT".to_string()),
            name: Some("Sent".to_string()),
        })
        .await
        .unwrap();
        db.upsert_label(&Label {
            id: Some("TRASH".to_string()),
            name: Some("Trash".to_string()),
        })
        .await
        .unwrap();

        let message = CachedMessage {
            id: "delete_msg_1".to_string(),
            thread_id: Some("delete_thread_1".to_string()),
            label_ids: vec!["INBOX".to_string(), "SENT".to_string()],
            snippet: Some("Snippet.".to_string()),
            subject: Some("Subject.".to_string()),
            from_addr: Some("from@example.com".to_string()),
            to_addr: Some("to@example.com".to_string()),
            date_str: Some("Date.".to_string()),
            body_text: Some("Body.".to_string()),
            body_html: None,
            received_date: Utc::now(),
            internal_date: Utc::now(),
            is_unread: true,
            is_starred: false,
            cache_timestamp: Utc::now(),
        };
        db.upsert_message(&message).await.unwrap();

        db.mark_message_deleted("delete_msg_1").await.unwrap();

        let fetched_message = db.get_message_by_id("delete_msg_1").await.unwrap().unwrap();
        assert!(fetched_message.label_ids.contains(&"TRASH".to_string()));
        assert_eq!(fetched_message.label_ids.len(), 1); // Only TRASH should remain
    }

    #[tokio::test]
    async fn test_sync_state_operations() {
        let db = setup_test_db().await.unwrap();

        // Ensure label exists
        db.upsert_label(&Label {
            id: Some("INBOX".to_string()),
            name: Some("Inbox".to_string()),
        })
        .await
        .unwrap();

        let label_id = "INBOX";
        let history_id = Some("12345");

        db.update_sync_state(label_id, history_id).await.unwrap();

        let sync_state = db.get_sync_state(label_id).await.unwrap().unwrap();
        assert_eq!(sync_state.label_id, label_id);
        assert_eq!(sync_state.history_id, history_id.map(|s| s.to_string()));

        // Update existing sync state
        let new_history_id = Some("67890");
        db.update_sync_state(label_id, new_history_id)
            .await
            .unwrap();
        let updated_sync_state = db.get_sync_state(label_id).await.unwrap().unwrap();
        assert_eq!(
            updated_sync_state.history_id,
            new_history_id.map(|s| s.to_string())
        );
    }

    #[tokio::test]
    async fn test_cleanup_old_messages() {
        let db = setup_test_db().await.unwrap();
        let message1 = CachedMessage {
            id: "old_msg_1".to_string(),
            thread_id: Some("old_thread_1".to_string()),
            label_ids: vec![],
            snippet: None,
            subject: None,
            from_addr: None,
            to_addr: None,
            date_str: None,
            body_text: None,
            body_html: None,
            received_date: Utc::now(),
            internal_date: Utc::now(),
            is_unread: false,
            is_starred: false,
            cache_timestamp: Utc::now() - chrono::Duration::days(5), // 5 days old
        };
        let message2 = CachedMessage {
            id: "recent_msg_1".to_string(),
            thread_id: Some("recent_thread_1".to_string()),
            label_ids: vec![],
            snippet: None,
            subject: None,
            from_addr: None,
            to_addr: None,
            date_str: None,
            body_text: None,
            body_html: None,
            received_date: Utc::now(),
            internal_date: Utc::now(),
            is_unread: false,
            is_starred: false,
            cache_timestamp: Utc::now(), // Current
        };

        db.upsert_message(&message1).await.unwrap();
        db.upsert_message(&message2).await.unwrap();

        let initial_count = db
            .get_messages_for_label("ALLMAIL", 10, 0)
            .await
            .unwrap()
            .len();
        assert_eq!(initial_count, 2);

        let cleaned_count = db.cleanup_old_messages(3).await.unwrap(); // Clean messages older than 3 days
        assert_eq!(cleaned_count, 1); // Only message1 should be deleted

        let remaining_count = db
            .get_messages_for_label("ALLMAIL", 10, 0)
            .await
            .unwrap()
            .len();
        assert_eq!(remaining_count, 1);
        assert_eq!(
            db.get_message_by_id("recent_msg_1")
                .await
                .unwrap()
                .unwrap()
                .id,
            "recent_msg_1"
        );
        assert!(db.get_message_by_id("old_msg_1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_cache_stats() {
        let db = setup_test_db().await.unwrap();

        let label = Label {
            id: Some("INBOX".to_string()),
            name: Some("Inbox".to_string()),
        };
        db.upsert_label(&label).await.unwrap();

        let message = CachedMessage {
            id: "stats_msg_1".to_string(),
            thread_id: Some("stats_thread_1".to_string()),
            label_ids: vec!["INBOX".to_string()],
            snippet: None,
            subject: None,
            from_addr: None,
            to_addr: None,
            date_str: None,
            body_text: None,
            body_html: None,
            received_date: Utc::now(),
            internal_date: Utc::now(),
            is_unread: false,
            is_starred: false,
            cache_timestamp: Utc::now(),
        };
        db.upsert_message(&message).await.unwrap();

        let stats = db.get_cache_stats().await.unwrap();
        assert_eq!(*stats.get("total_messages").unwrap(), 1);
        assert_eq!(*stats.get("total_labels").unwrap(), 1);
    }
}

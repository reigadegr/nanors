#![deny(
    clippy::all,
    clippy::nursery,
    clippy::pedantic,
    clippy::style,
    clippy::complexity,
    clippy::perf,
    clippy::correctness,
    clippy::suspicious,
    clippy::unwrap_used,
    clippy::expect_used
)]
#![allow(
    clippy::similar_names,
    clippy::missing_safety_doc,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc
)]

use async_trait::async_trait;
use nanors_core::{ChatMessage, Role, Session as CoreSession, SessionStorage};
use sqlx::{SqlitePool, Row};
use tracing::info;

pub struct SessionManager {
    pool: SqlitePool,
}

impl SessionManager {
    pub async fn new(db_path: std::path::PathBuf) -> anyhow::Result<Self> {
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        info!("Connecting to database: {}", db_url);
        
        let pool = SqlitePool::connect(&db_url).await?;
        
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sessions (
                key TEXT PRIMARY KEY,
                messages TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        ).execute(&pool).await?;

        info!("SessionManager initialized");
        Ok(Self { pool })
    }

    pub async fn clear_session(&self, key: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await?;

        info!("Cleared session: {}", key);
        Ok(())
    }

    pub async fn list_sessions(&self) -> anyhow::Result<Vec<String>> {
        let rows = sqlx::query("SELECT key FROM sessions")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.iter().map(|row| row.get("key")).collect())
    }
}

#[async_trait]
impl SessionStorage for SessionManager {
    async fn get_or_create(&self, key: &str) -> anyhow::Result<CoreSession> {
        let row = sqlx::query("SELECT * FROM sessions WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let messages_json: String = row.get("messages");
            let messages: Vec<ChatMessage> = serde_json::from_str(&messages_json)?;

            Ok(CoreSession {
                key: row.get("key"),
                messages,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
        } else {
            let now = chrono::Utc::now();
            Ok(CoreSession {
                key: key.to_string(),
                messages: vec![],
                created_at: now,
                updated_at: now,
            })
        }
    }

    async fn add_message(&self, key: &str, role: Role, content: &str) -> anyhow::Result<()> {
        let mut session = self.get_or_create(key).await?;
        session.messages.push(ChatMessage {
            role,
            content: content.to_string(),
        });
        session.updated_at = chrono::Utc::now();

        let messages_json = serde_json::to_string(&session.messages)?;

        sqlx::query(
            "INSERT INTO sessions (key, messages, created_at, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET
             messages = excluded.messages,
             updated_at = excluded.updated_at",
        )
        .bind(&session.key)
        .bind(&messages_json)
        .bind(session.created_at)
        .bind(session.updated_at)
        .execute(&self.pool)
        .await?;

        info!("Added message to session: {}", key);
        Ok(())
    }
}

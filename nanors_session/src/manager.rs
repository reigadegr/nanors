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
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, DbErr, EntityTrait, Set};
use std::path::PathBuf;
use tracing::info;

use crate::entity::sessions;

pub struct SessionManager {
    db: DatabaseConnection,
}

impl SessionManager {
    pub async fn new(db_path: PathBuf) -> anyhow::Result<Self> {
        let db_url = format!("sqlite:{}", db_path.display());
        info!("Connecting to database: {}", db_url);

        let db = Database::connect(&db_url).await?;

        info!("SessionManager initialized");
        Ok(Self { db })
    }

    pub async fn clear_session(&self, key: &str) -> anyhow::Result<()> {
        sessions::Entity::delete_by_id(key.to_owned())
            .exec(&self.db)
            .await?;

        info!("Cleared session: {}", key);
        Ok(())
    }

    pub async fn list_sessions(&self) -> anyhow::Result<Vec<String>> {
        let session_models = sessions::Entity::find().all(&self.db).await?;

        Ok(session_models.into_iter().map(|s| s.key).collect())
    }
}

#[async_trait]
impl SessionStorage for SessionManager {
    async fn get_or_create(&self, key: &str) -> anyhow::Result<CoreSession> {
        let session_model = sessions::Entity::find_by_id(key.to_owned())
            .one(&self.db)
            .await?;

        if let Some(model) = session_model {
            let messages: Vec<ChatMessage> = serde_json::from_str(&model.messages)?;

            Ok(CoreSession {
                key: model.key,
                messages,
                created_at: model.created_at.and_utc(),
                updated_at: model.updated_at.and_utc(),
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

        let now = session.updated_at.naive_utc();
        let created_at = session.created_at.naive_utc();

        let result = sessions::Entity::update(sessions::ActiveModel {
            key: Set(session.key.clone()),
            messages: Set(messages_json.clone()),
            created_at: Set(created_at),
            updated_at: Set(now),
        })
        .exec(&self.db)
        .await;

        match result {
            Ok(_) => {
                info!("Added message to session: {}", key);
                Ok(())
            }
            Err(DbErr::RecordNotFound(_)) => {
                sessions::ActiveModel {
                    key: Set(session.key),
                    messages: Set(messages_json),
                    created_at: Set(created_at),
                    updated_at: Set(now),
                }
                .insert(&self.db)
                .await?;

                info!("Added message to session: {}", key);
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }
}

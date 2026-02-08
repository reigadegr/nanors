#![warn(
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
use nanors_entities::sessions;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait, Set};
use tracing::info;
use uuid::Uuid;

pub struct SessionManager {
    db: DatabaseConnection,
}

impl SessionManager {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        info!("Connecting to database: {}", database_url);

        let db = Database::connect(database_url).await?;

        info!("SessionManager initialized");
        Ok(Self { db })
    }

    pub async fn clear_session(&self, id: &Uuid) -> anyhow::Result<()> {
        sessions::Entity::delete_by_id(*id).exec(&self.db).await?;

        info!("Cleared session: {}", id);
        Ok(())
    }

    pub async fn list_sessions(&self) -> anyhow::Result<Vec<Uuid>> {
        let session_models = sessions::Entity::find().all(&self.db).await?;

        Ok(session_models.into_iter().map(|s| s.id).collect())
    }
}

#[async_trait]
impl SessionStorage for SessionManager {
    async fn get_or_create(&self, id: &Uuid) -> anyhow::Result<CoreSession> {
        let session_model = sessions::Entity::find_by_id(*id).one(&self.db).await?;

        if let Some(model) = session_model {
            let messages: Vec<ChatMessage> = serde_json::from_str(&model.messages)?;

            Ok(CoreSession {
                id: model.id,
                messages,
                created_at: model.created_at.and_utc(),
                updated_at: model.updated_at.and_utc(),
            })
        } else {
            let now = chrono::Utc::now();
            Ok(CoreSession {
                id: *id,
                messages: vec![],
                created_at: now,
                updated_at: now,
            })
        }
    }

    async fn add_message(&self, id: &Uuid, role: Role, content: &str) -> anyhow::Result<()> {
        let now = chrono::Utc::now().naive_utc();

        if let Some(model) = sessions::Entity::find_by_id(*id).one(&self.db).await? {
            let mut messages: Vec<ChatMessage> = serde_json::from_str(&model.messages)?;
            messages.push(ChatMessage {
                role,
                content: content.to_string(),
            });
            let messages_json = serde_json::to_string(&messages)?;

            sessions::Entity::update(sessions::ActiveModel {
                id: Set(model.id),
                messages: Set(messages_json),
                created_at: Set(model.created_at),
                updated_at: Set(now),
            })
            .exec(&self.db)
            .await?;
        } else {
            let messages = vec![ChatMessage {
                role,
                content: content.to_string(),
            }];
            let messages_json = serde_json::to_string(&messages)?;

            sessions::ActiveModel {
                id: Set(*id),
                messages: Set(messages_json),
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(&self.db)
            .await?;
        }

        info!("Added message to session: {}", id);
        Ok(())
    }
}

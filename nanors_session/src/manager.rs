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
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, Database, DatabaseConnection, DbErr, EntityTrait, Schema,
    Set,
};
use std::path::PathBuf;
use tracing::info;

use crate::entity::sessions;

fn is_table_already_exists_error(err: &DbErr) -> bool {
    err.to_string().contains("table") && err.to_string().contains("already exists")
}

pub struct SessionManager {
    db: DatabaseConnection,
}

impl SessionManager {
    pub async fn new(db_path: PathBuf) -> anyhow::Result<Self> {
        let db_url = format!("sqlite:{}", db_path.display());
        info!("Connecting to database: {}", db_url);

        let db = Database::connect(&db_url).await?;

        let backend = db.get_database_backend();
        let schema = Schema::new(backend);
        let stmt = schema.create_table_from_entity(sessions::Entity);
        let builder = db.get_database_backend();
        match db
            .execute_unprepared(&builder.build(&stmt).to_string())
            .await
        {
            Ok(_) => {}
            Err(e) if is_table_already_exists_error(&e) => {
                info!("Table already exists, skipping creation");
            }
            Err(e) => return Err(e.into()),
        }

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

        let exists = sessions::Entity::find_by_id(key.to_owned())
            .one(&self.db)
            .await?
            .is_some();

        if exists {
            sessions::Entity::update(sessions::ActiveModel {
                key: Set(session.key.clone()),
                messages: Set(messages_json.clone()),
                created_at: Set(created_at),
                updated_at: Set(now),
            })
            .exec(&self.db)
            .await?;
        } else {
            sessions::ActiveModel {
                key: Set(session.key),
                messages: Set(messages_json),
                created_at: Set(created_at),
                updated_at: Set(now),
            }
            .insert(&self.db)
            .await?;
        }

        info!("Added message to session: {}", key);
        Ok(())
    }
}

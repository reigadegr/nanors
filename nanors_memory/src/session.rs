use async_trait::async_trait;
use nanors_core::{ChatMessage, MessageContent, Role, Session, SessionStorage};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use uuid::Uuid;

use crate::manager::MemoryManager;
use nanors_entities::sessions;

#[async_trait]
impl SessionStorage for MemoryManager {
    async fn get_or_create(&self, id: &Uuid) -> anyhow::Result<Session> {
        let session_model = sessions::Entity::find_by_id(*id).one(&self.db).await?;

        if let Some(model) = session_model {
            let messages: Vec<ChatMessage> = serde_json::from_str(&model.messages)?;

            Ok(Session {
                id: model.id,
                messages,
                created_at: model.created_at.and_utc(),
                updated_at: model.updated_at.and_utc(),
            })
        } else {
            let now = chrono::Utc::now();
            Ok(Session {
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
                content: MessageContent::Text(content.to_string()),
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
                content: MessageContent::Text(content.to_string()),
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

        tracing::info!("Added message to session: {}", id);
        Ok(())
    }
}

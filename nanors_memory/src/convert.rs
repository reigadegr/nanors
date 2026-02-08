use nanors_core::memory::{CategoryItem, MemoryCategory, MemoryItem, MemoryType, Resource};
use nanors_entities::{category_items, memory_categories, memory_items, resources};
use sea_orm::JsonValue;

#[allow(clippy::cast_possible_truncation)]
fn json_to_embedding(val: &JsonValue) -> Option<Vec<f32>> {
    let arr = val.as_array()?;
    Some(
        arr.iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect(),
    )
}

fn embedding_to_json(emb: &[f32]) -> JsonValue {
    JsonValue::Array(emb.iter().map(|f| JsonValue::from(f64::from(*f))).collect())
}

pub fn memory_item_from_model(m: memory_items::Model) -> MemoryItem {
    let embedding = m
        .embedding
        .as_ref()
        .map(|json| json_to_embedding(json))
        .flatten();
    let memory_type = m
        .memory_type
        .parse::<MemoryType>()
        .unwrap_or(MemoryType::Episodic);

    MemoryItem {
        id: m.id,
        user_scope: m.user_scope,
        resource_id: m.resource_id,
        memory_type,
        summary: m.summary,
        embedding,
        happened_at: m.happened_at.into(),
        extra: m.extra,
        content_hash: m.content_hash,
        reinforcement_count: m.reinforcement_count,
        created_at: m.created_at.into(),
        updated_at: m.updated_at.into(),
    }
}

pub fn memory_category_from_model(m: memory_categories::Model) -> MemoryCategory {
    let embedding = m
        .embedding
        .as_ref()
        .map(|json| json_to_embedding(json))
        .flatten();

    MemoryCategory {
        id: m.id,
        user_scope: m.user_scope,
        name: m.name,
        description: m.description,
        embedding,
        summary: m.summary,
        created_at: m.created_at.into(),
        updated_at: m.updated_at.into(),
    }
}

pub const fn category_item_from_model(m: &category_items::Model) -> CategoryItem {
    CategoryItem {
        item_id: m.item_id,
        category_id: m.category_id,
    }
}

pub fn resource_from_model(m: resources::Model) -> Resource {
    let embedding = m
        .embedding
        .as_ref()
        .map(|json| json_to_embedding(json))
        .flatten();

    Resource {
        id: m.id,
        user_scope: m.user_scope,
        url: m.url,
        modality: m.modality,
        local_path: m.local_path,
        caption: m.caption,
        embedding,
        created_at: m.created_at.into(),
        updated_at: m.updated_at.into(),
    }
}

pub fn embedding_option_to_json(emb: Option<&Vec<f32>>) -> Option<JsonValue> {
    emb.map(|v| embedding_to_json(v.as_slice()))
}

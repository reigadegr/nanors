use nanors_core::memory::MemoryItem;
use nanors_entities::memory_items;
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

pub fn embedding_to_json(emb: &[f32]) -> JsonValue {
    JsonValue::Array(emb.iter().map(|f| JsonValue::from(f64::from(*f))).collect())
}

pub fn memory_item_from_model(m: memory_items::Model) -> MemoryItem {
    let embedding = m.embedding.as_ref().and_then(json_to_embedding);
    let memory_type = m
        .memory_type
        .parse::<nanors_core::memory::MemoryType>()
        .unwrap_or(nanors_core::memory::MemoryType::Episodic);

    MemoryItem {
        id: m.id,
        user_scope: m.user_scope,
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

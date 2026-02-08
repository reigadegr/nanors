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
use chrono::Utc;
use nanors_core::{CategoryCompressor, CategoryItemRepo, MemoryCategoryRepo};
use tracing::{debug, info};
use uuid::Uuid;

/// Trait for compressing category summaries
#[async_trait]
pub trait CategoryCompression: Send + Sync {
    /// Update a category summary by compressing it with new items
    ///
    /// # Arguments
    /// * `category_id` - ID of the category to update
    /// * `compressor` - Category compressor implementation
    /// * `new_items` - New items to merge (item_id, summary) pairs
    /// * `target_length` - Target length in tokens
    ///
    /// # Returns
    /// * Result indicating success or failure
    async fn update_category_summary(
        &self,
        category_id: &Uuid,
        compressor: &dyn CategoryCompressor,
        new_items: &[(Uuid, String)],
        target_length: usize,
    ) -> anyhow::Result<()>;

    /// Find categories that need compression based on new memory items
    ///
    /// # Arguments
    /// * `item_ids` - IDs of newly inserted memory items
    ///
    /// # Returns
    /// * Map of category_id to (category_name, current_summary, new_items)
    async fn find_categories_for_compression(
        &self,
        item_ids: &[Uuid],
    ) -> anyhow::Result<
        Vec<(
            Uuid,                // category_id
            String,              // category_name
            String,              // current_summary
            Vec<(Uuid, String)>, // new_items (id, summary)
        )>,
    >;

    /// Batch compress multiple categories
    ///
    /// # Arguments
    /// * `compressor` - Category compressor implementation
    /// * `target_length` - Target length in tokens for all summaries
    ///
    /// # Returns
    /// * Number of categories compressed
    async fn compress_categories(
        &self,
        compressor: &dyn CategoryCompressor,
        target_length: usize,
    ) -> anyhow::Result<usize>;
}

/// Extension trait to add compression functionality to `MemoryManager`
#[async_trait]
impl CategoryCompression for super::MemoryManager {
    async fn update_category_summary(
        &self,
        category_id: &Uuid,
        compressor: &dyn CategoryCompressor,
        new_items: &[(Uuid, String)],
        target_length: usize,
    ) -> anyhow::Result<()> {
        // Get the current category
        let Some(mut category) = MemoryCategoryRepo::find_by_id(self, category_id).await? else {
            debug!("Category {} not found, skipping compression", category_id);
            return Ok(());
        };

        let current_summary = category.summary.as_deref().unwrap_or("");

        // Compress the summary
        let result = compressor
            .compress_category_summary(&category.name, current_summary, new_items, target_length)
            .await?;

        debug!(
            "Compressed category '{}' summary: {} -> {} chars",
            category.name,
            current_summary.len(),
            result.summary.len()
        );

        // Update the category with the new summary
        category.summary = Some(result.summary);
        category.updated_at = Utc::now();

        MemoryCategoryRepo::update(self, &category).await?;

        info!("Updated category summary for '{}'", category.name);

        Ok(())
    }

    async fn find_categories_for_compression(
        &self,
        item_ids: &[Uuid],
    ) -> anyhow::Result<Vec<(Uuid, String, String, Vec<(Uuid, String)>)>> {
        let mut result: Vec<(Uuid, String, String, Vec<(Uuid, String)>)> = Vec::new();

        for item_id in item_ids {
            // Find all categories linked to this item
            let links = CategoryItemRepo::categories_for_item(self, item_id).await?;

            for link in links {
                // Get the category details
                if let Some(category) =
                    MemoryCategoryRepo::find_by_id(self, &link.category_id).await?
                {
                    // Check if we already have this category in our result
                    if let Some(entry) = result
                        .iter_mut()
                        .find(|(id, _, _, _)| *id == link.category_id)
                    {
                        // Add the item to existing entry
                        // Note: We don't have the item summary here, so we'll need to fetch it
                        // For now, we'll add a placeholder
                        entry.3.push((*item_id, "[item summary]".to_string()));
                    } else {
                        // Create new entry
                        result.push((
                            link.category_id,
                            category.name.clone(),
                            category.summary.unwrap_or_default(),
                            vec![(*item_id, "[item summary]".to_string())],
                        ));
                    }
                }
            }
        }

        Ok(result)
    }

    async fn compress_categories(
        &self,
        _compressor: &dyn CategoryCompressor,
        _target_length: usize,
    ) -> anyhow::Result<usize> {
        // This would be used for batch compression of all categories
        // For now, we'll return 0 as this is typically called on-demand
        debug!("Batch compression not yet implemented");
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_placeholder() {
        // Placeholder test to ensure the module compiles
    }
}

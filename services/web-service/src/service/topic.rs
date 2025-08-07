use std::{sync::Arc, time::Instant};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use futures::TryStreamExt as _;
use mongodb::{Collection, bson::doc};
use parking_lot::RwLock;
use share::models::{
    database::{CreateTopicStatus, TopicAuditInfo, VotingTopic},
    excel::CharacterInfo,
};
use tokio::sync::RwLock as AsyncRwLock;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct CacheEntry {
    data: VotingTopic,
    pool: Vec<i32>,
    last_accessed: Arc<RwLock<Instant>>,
}

impl CacheEntry {
    fn new(data: VotingTopic) -> Self {
        Self {
            data,
            pool: Vec::new(),
            last_accessed: Arc::new(RwLock::new(Instant::now())),
        }
    }

    fn access(&self) -> VotingTopic {
        *self.last_accessed.write() = Instant::now();
        self.data.clone()
    }
}

#[derive(Clone)]
pub struct TopicCache {
    pub cache: DashMap<String, CacheEntry>,
    pub last_full_refresh: Arc<RwLock<DateTime<Utc>>>,
}

impl TopicCache {
    pub fn get(&self, topic_id: &str) -> Option<VotingTopic> {
        self.cache.get(topic_id).map(|entry| entry.access())
    }

    pub fn get_pool(&self, topic_id: &str) -> Option<Vec<i32>> {
        self.cache.get(topic_id).map(|entry| entry.pool.clone())
    }

    pub fn cache_topic_pool(&self, topic_id: &str, pool: Vec<i32>) {
        if let Some(mut entry) = self.cache.get_mut(topic_id) {
            entry.pool = pool;
        } else {
            tracing::warn!(
                "Attempted to cache pool for non-existent topic: {}",
                topic_id
            );
        }
    }

    pub fn insert(&self, topic: &VotingTopic) -> bool {
        let topic_id = topic.id.clone();

        if let Some(existing) = self.cache.get(&topic_id)
            && !self.should_update_entry(&existing.data, topic)
        {
            return false;
        }

        let entry = CacheEntry::new(topic.clone());
        let was_new = self.cache.insert(topic_id.clone(), entry).is_none();

        if was_new {
            tracing::debug!("Cached new topic: {}", topic_id);
        } else {
            tracing::debug!("Updated cached topic: {}", topic_id);
        }

        true
    }

    pub fn insert_batch(&self, topics: &[VotingTopic]) -> usize {
        let mut inserted_count = 0;

        for topic in topics {
            if self.insert(topic) {
                inserted_count += 1;
            }
        }

        tracing::info!("Batch inserted {} topics", inserted_count);
        inserted_count
    }

    pub fn get_active_topic_ids(&self) -> Vec<String> {
        self.cache
            .iter()
            .filter_map(|entry| {
                let topic = &entry.value().data;
                if topic.is_active {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect()
    }

    fn should_update_entry(&self, cached: &VotingTopic, new: &VotingTopic) -> bool {
        match (&cached.updated_at, &new.updated_at) {
            (None, Some(_)) => true,
            (Some(cached_time), Some(new_time)) => cached_time < new_time,
            (Some(_), None) => false,
            (None, None) => {
                cached.description != new.description
                    || cached.name != new.name
                    || cached.is_active != new.is_active
            }
        }
    }
}

#[derive(Clone)]
pub struct TopicService {
    topic_collection: Collection<VotingTopic>,
    cache: TopicCache,

    refresh_lock: Arc<AsyncRwLock<()>>,
}

impl TopicService {
    pub fn new(mongo: mongodb::Database) -> Self {
        let topic_collection = mongo.collection::<VotingTopic>("topics");
        let topic_cache = TopicCache {
            cache: DashMap::new(),
            last_full_refresh: Arc::new(RwLock::new(Utc::now())),
        };
        let refresh_lock = Arc::new(AsyncRwLock::new(()));

        tokio::spawn(Self::cache_updater(
            topic_collection.clone(),
            topic_cache.clone(),
        ));

        Self {
            topic_collection,
            cache: topic_cache,
            refresh_lock,
        }
    }

    pub async fn create_topic(&self, topic: &VotingTopic) -> Result<(), AppError> {
        self.topic_collection.insert_one(topic).await?;

        Ok(())
    }

    pub async fn _update_topic(&self, mut topic: VotingTopic) -> Result<(), AppError> {
        let filter = doc! { "id": &topic.id };
        topic.updated_at = Some(Utc::now());
        self.topic_collection.replace_one(filter, &topic).await?;
        self.cache.insert(&topic);

        Ok(())
    }

    pub async fn _delete_topic(&self, topic_id: &str) -> Result<(), AppError> {
        let filter = doc! { "id": topic_id };
        self.topic_collection.delete_one(filter).await?;
        self.cache.cache.remove(topic_id);

        Ok(())
    }

    pub async fn get_topic(&self, topic_id: &str) -> Result<Option<VotingTopic>, AppError> {
        if let Some(cached_topic) = self.cache.get(topic_id) {
            return Ok(Some(cached_topic));
        }

        let _read_lock = self.refresh_lock.read().await;
        let filter = doc! { "id": topic_id };

        if let Some(topic) = self.topic_collection.find_one(filter).await? {
            self.cache.insert(&topic);
            Ok(Some(topic))
        } else {
            tracing::debug!("Topic not found in database: {}", topic_id);

            Ok(None)
        }
    }

    pub async fn get_active_topic_ids(&self) -> Result<Vec<String>, AppError> {
        Ok(self.cache.get_active_topic_ids())
    }

    pub async fn get_need_audit_topics(&self) -> Result<Vec<VotingTopic>, AppError> {
        let filter = doc! { "status": "WaitingAudit" };
        let mut cursor = self.topic_collection.find(filter).await?;
        let mut topics = Vec::new();

        while let Some(topic) = cursor.try_next().await? {
            topics.push(topic);
        }

        Ok(topics)
    }

    pub async fn audit_topic(
        &self,
        topic_id: &str,
        audit_info: TopicAuditInfo,
    ) -> Result<(), AppError> {
        let status = if audit_info.is_approved() {
            CreateTopicStatus::Approved(audit_info)
        } else {
            CreateTopicStatus::Rejected(audit_info)
        };

        let filter = doc! { "id": topic_id };
        let update = doc! {
            "$set": {
                "status": mongodb::bson::to_bson(&status).unwrap(),
                "updated_at": mongodb::bson::to_bson(&Utc::now()).unwrap()
            }
        };

        self.topic_collection.update_one(filter, update).await?;
        if let Some(topic) = self.get_topic(topic_id).await? {
            self.cache.insert(&topic);
        }

        Ok(())
    }

    pub async fn get_candidate_pool(
        &self,
        topic_id: &str,
        character_infos: &[CharacterInfo],
    ) -> Option<Vec<i32>> {
        if let Some(pool) = self.cache.get_pool(topic_id)
            && !pool.is_empty()
        {
            return Some(pool);
        }

        match self.get_topic(topic_id).await {
            Ok(Some(topic)) => {
                let pool = topic.candidate_pool.generate_pool(character_infos);
                if !pool.is_empty() {
                    self.cache.cache_topic_pool(topic_id, pool.clone());
                    Some(pool)
                } else {
                    None
                }
            }
            Ok(None) => None,
            Err(_) => None,
        }
    }

    pub async fn _refresh_cache(&self) -> Result<usize, AppError> {
        let _write_lock = self.refresh_lock.write().await;
        self._update_cache_internal().await
    }

    async fn _update_cache_internal(&self) -> Result<usize, AppError> {
        let filter = doc! {};
        let mut cursor = self.topic_collection.find(filter).await?;
        let mut topics = Vec::new();

        while let Some(topic) = cursor.try_next().await? {
            topics.push(topic);
        }

        let inserted_count = self.cache.insert_batch(&topics);
        tracing::info!("Cache refresh completed: {} topics updated", inserted_count);

        Ok(inserted_count)
    }

    async fn cache_updater(topic_collection: Collection<VotingTopic>, topic_cache: TopicCache) {
        const CACHE_UPDATE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

        Self::initial_warm_cache(&topic_collection, &topic_cache)
            .await
            .map_err(|e| {
                tracing::error!("Failed to warm up cache: {}", e);
            })
            .unwrap_or_else(|_| {
                tracing::info!("Cache warmed up successfully.");
            });

        loop {
            let start = std::time::Instant::now();

            match Self::incremental_cache_update(&topic_collection, &topic_cache).await {
                Ok(updated_count) => {
                    tracing::debug!(
                        "Incremental cache update completed: {} topics updated",
                        updated_count
                    );
                }
                Err(e) => {
                    tracing::error!("Error during incremental cache update: {}", e);
                    if let Err(e) = Self::full_cache_update(&topic_collection, &topic_cache).await {
                        tracing::error!("Full cache update failed: {}", e);
                    } else {
                        tracing::debug!("Full cache update completed successfully.");
                    }
                }
            }

            let elapsed = start.elapsed();
            if elapsed < CACHE_UPDATE_INTERVAL {
                tokio::time::sleep(CACHE_UPDATE_INTERVAL - elapsed).await;
            }
        }
    }

    async fn initial_warm_cache(
        topic_collection: &Collection<VotingTopic>,
        cache: &TopicCache,
    ) -> Result<(), AppError> {
        tracing::info!("Warming up cache...");

        let filter = doc! {};
        let mut cursor = topic_collection.find(filter).await?;
        let mut topics = Vec::new();

        while let Some(topic) = cursor.try_next().await? {
            topics.push(topic);
        }

        let updated_count = cache.insert_batch(&topics);
        *cache.last_full_refresh.write() = Utc::now();

        tracing::info!("Cache warmed up with {} topics", updated_count);

        Ok(())
    }

    async fn incremental_cache_update(
        topic_collection: &Collection<VotingTopic>,
        cache: &TopicCache,
    ) -> Result<usize, mongodb::error::Error> {
        let last_refresh = *cache.last_full_refresh.read();
        let since_filter = doc! {
            "updated_at": { "$gte": mongodb::bson::to_bson(&last_refresh).unwrap() }
        };

        let mut cursor = topic_collection.find(since_filter).await?;
        let mut updated_count = 0;

        while let Some(topic) = cursor.try_next().await? {
            if cache.insert(&topic) {
                updated_count += 1;
            }
        }

        Ok(updated_count)
    }

    async fn full_cache_update(
        topic_collection: &Collection<VotingTopic>,
        cache: &TopicCache,
    ) -> Result<usize, mongodb::error::Error> {
        let filter = doc! {};
        let mut cursor = topic_collection.find(filter).await?;
        let mut topics = Vec::new();

        while let Some(topic) = cursor.try_next().await? {
            topics.push(topic);
        }

        let updated_count = cache.insert_batch(&topics);
        *cache.last_full_refresh.write() = Utc::now();

        Ok(updated_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::options::ClientOptions;
    use share::models::{
        candidate_pool_preset::CandidatePoolPreset,
        database::{CreateTopicStatus, VotingTopicType},
        excel::RarityRank,
    };
    use tokio;

    #[tokio::test]
    async fn test_topic_service() {
        tracing_subscriber::fmt::init();

        let mongo_uri = "mongodb://localhost:27017";
        let client_options = ClientOptions::parse(mongo_uri).await.unwrap();
        let client = mongodb::Client::with_options(client_options).unwrap();
        let db = client.database("test_db");

        let topic_service = TopicService::new(db.clone());

        let test_topic = VotingTopic {
            id: "test_topic_1".to_string(),
            name: "Test Topic".to_string(),
            title: "Test Title".to_string(),
            description: "This is a test topic.".to_string(),
            topic_type: VotingTopicType::Pairwise,
            candidate_pool: CandidatePoolPreset::ByRarity {
                rarities: vec![RarityRank::Tier6],
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
            open_time: chrono::Utc::now(),
            close_time: chrono::Utc::now() + chrono::Duration::days(1),
            is_active: true,
            status: CreateTopicStatus::WaitingAudit,
        };

        // Test create_topic
        topic_service.create_topic(&test_topic).await.unwrap();

        // Test get_topic_by_id
        let fetched_topic = topic_service
            .get_topic("test_topic_1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched_topic.id, test_topic.id);
        assert_eq!(fetched_topic.name, test_topic.name);

        // Test get_all_active_topics
        let active_topics = topic_service.get_active_topic_ids().await.unwrap();
        assert!(active_topics.contains(&"test_topic_1".to_string()));

        // Test initial cache functionality
        {
            let cached_topic = topic_service.cache.get("test_topic_1").unwrap();
            assert_eq!(cached_topic.id, test_topic.id);
            assert_eq!(cached_topic.name, test_topic.name);
            assert_eq!(cached_topic.description, "This is a test topic.");
            println!(
                "Before update - cached description: {}",
                cached_topic.description
            );
        }

        // Create updated topic
        let test_topic_updated = VotingTopic {
            updated_at: Some(chrono::Utc::now()),
            description: "Updated description.".to_string(),
            ..test_topic.clone()
        };

        // 使用update_topic方法
        println!("About to update topic...");
        topic_service
            ._update_topic(test_topic_updated)
            .await
            .unwrap();
        println!("Topic updated in database and cache");

        println!(
            "After update - Current cache: {:?}",
            topic_service.cache.cache
        );

        {
            let cached_topic_after_update = topic_service.cache.get("test_topic_1").unwrap();

            println!(
                "Retrieved from cache after update: {}",
                cached_topic_after_update.description
            );

            assert_eq!(
                cached_topic_after_update.description,
                "Updated description.".to_string(),
                "Cache was not properly updated. Expected 'Updated description.' but got '{}'",
                cached_topic_after_update.description
            );
        }

        let updated_topic_from_service = topic_service
            .get_topic("test_topic_1")
            .await
            .unwrap()
            .unwrap();

        println!(
            "Retrieved from service: {}",
            updated_topic_from_service.description
        );

        assert_eq!(
            updated_topic_from_service.description,
            "Updated description.".to_string()
        );

        // test get_need_audit_topics
        let audit_topics = topic_service.get_need_audit_topics().await.unwrap();
        assert_eq!(audit_topics.len(), 1);
        assert_eq!(audit_topics[0].id, "test_topic_1");

        // Clean up
        topic_service._delete_topic("test_topic_1").await.unwrap();
        db.collection::<VotingTopic>("topics").drop().await.unwrap();
    }
}

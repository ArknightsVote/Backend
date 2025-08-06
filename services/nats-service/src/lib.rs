use std::{borrow::Cow, sync::Arc};

mod constants;
mod consumer;
mod db;
mod error;

use eyre::{Context, Result};
use share::config::AppConfig;

use crate::{
    constants::{
        LUA_SCRIPT_BATCH_IP_COUNTER_SCRIPT, LUA_SCRIPT_BATCH_SCORE_UPDATE_SCRIPT,
        LUA_SCRIPT_DEL_MUTIPLE, LUA_SCRIPT_GET_DEL_MANY, LUA_SCRIPT_IP_COUNTER,
        LUA_SCRIPT_UPDATE_SCORES,
    },
    consumer::available_consumers,
    db::{AppDatabase, RedisService},
};

pub struct NatsService {
    config: Arc<AppConfig>,
}

impl NatsService {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    pub async fn run(self, mut shutdown_rx: share::signal::ShutdownRx) -> Result<()> {
        let database = self.setup_database().await?;

        let stream = self.create_jetstream_setup(&database.jetstream).await?;

        self.start_consumers(&stream, &database).await?;

        tracing::info!("nats service started successfully");

        shutdown_rx
            .changed()
            .await
            .context("failed to receive shutdown signal")?;

        tracing::info!("shutting down nats consumer");
        Ok(())
    }

    async fn setup_database(&self) -> Result<Arc<AppDatabase>> {
        let nats_client = async_nats::connect(&self.config.nats.url)
            .await
            .context("failed to connect to nats")?;

        let jetstream = async_nats::jetstream::new(nats_client);

        let database_config = &self.config.database;

        let redis_client = redis::Client::open(&*database_config.redis_url)
            .context("failed to create Redis client")?;

        let mongodb_client = mongodb::Client::with_uri_str(&database_config.mongodb_url)
            .await
            .context("failed to connect to MongoDB")?;

        let mongo_database = mongodb_client.database(&database_config.mongodb_database);

        Ok(Arc::new(AppDatabase {
            redis: RedisService {
                client: redis_client,
                score_update_script: redis::Script::new(LUA_SCRIPT_UPDATE_SCORES),
                ip_counter_script: redis::Script::new(LUA_SCRIPT_IP_COUNTER),
                batch_ip_counter_script: redis::Script::new(LUA_SCRIPT_BATCH_IP_COUNTER_SCRIPT),
                batch_score_update_script: redis::Script::new(LUA_SCRIPT_BATCH_SCORE_UPDATE_SCRIPT),
                get_del_many_script: redis::Script::new(LUA_SCRIPT_GET_DEL_MANY),
                del_multiple_script: redis::Script::new(LUA_SCRIPT_DEL_MUTIPLE),
            },
            mongo_database,
            jetstream,
        }))
    }

    async fn start_consumers(
        &self,
        stream: &async_nats::jetstream::stream::Stream,
        database: &Arc<AppDatabase>,
    ) -> Result<()> {
        let consumers_config = &self.config.nats.consumers;
        let available = available_consumers();

        for config in consumers_config {
            if !config.enabled {
                tracing::info!("consumer '{}' is disabled, skipping", config.name);
                continue;
            }

            match available.get(config.name.as_str()) {
                Some(consumer) => {
                    let subject: Cow<'static, str> = Cow::Owned(config.subject.clone());
                    let stream = stream.clone();
                    let db = database.clone();
                    tracing::info!(
                        "starting consumer {} for subject {}",
                        consumer.name,
                        subject
                    );
                    (consumer.starter)(subject, stream, db, self.config.clone()).await?;
                }
                None => {
                    tracing::warn!("no consumer found for: {}", config.name);
                }
            }
        }

        tracing::debug!(
            "available consumers: {:?}. started consumers: {:?}",
            available.keys(),
            consumers_config
                .iter()
                .filter(|c| c.enabled)
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
        );

        tracing::info!("all consumers started successfully");

        Ok(())
    }

    async fn create_jetstream_setup(
        &self,
        jetstream: &async_nats::jetstream::Context,
    ) -> Result<async_nats::jetstream::stream::Stream> {
        let stream_config = &self.config.nats.stream_config;

        #[cfg(debug_assertions)]
        self.cleanup_existing_stream(jetstream, &stream_config.name)
            .await;

        tracing::debug!("creating jetstream stream with config: {:?}", stream_config);
        let stream = jetstream
            .create_stream(async_nats::jetstream::stream::Config {
                name: stream_config.name.clone(),
                retention: stream_config.retention,
                subjects: stream_config.subjects.clone(),
                max_messages: stream_config.max_messages,
                max_messages_per_subject: stream_config.max_messages_per_subject,
                ..Default::default()
            })
            .await
            .context("failed to create JetStream")?;

        Ok(stream)
    }

    #[cfg(debug_assertions)]
    async fn cleanup_existing_stream(
        &self,
        jetstream: &async_nats::jetstream::Context,
        stream_name: &str,
    ) {
        if let Err(e) = jetstream.delete_stream(stream_name).await {
            tracing::warn!("could not delete existing stream '{}': {}", stream_name, e);
        } else {
            tracing::info!("deleted existing stream '{}'", stream_name);
        }
    }
}

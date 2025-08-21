use std::{
    net::SocketAddr,
    sync::{Arc, atomic::AtomicU8},
};

use actix_cors::Cors;
use actix_web::{http::header, middleware, web};
use dashmap::DashMap;
use eyre::Context;
use mongodb::bson::doc;
use share::{
    config::AppConfig,
    models::{database::VotingTopic, excel::CharacterInfo},
    snowflake::Snowflake,
};

use crate::{
    api::{
        audit_topic_fn, audit_topics_list_fn, ballot_create_fn, ballot_save_fn, ballot_skip_fn,
        results_1v1_matrix_fn, results_final_order_fn, topic_candidate_pool_fn, topic_create_fn,
        topic_info_fn, topic_list_active_fn,
    },
    constants::{
        LUA_SCRIPT_BATCH_IP_COUNTER_SCRIPT, LUA_SCRIPT_BATCH_SCORE_UPDATE_SCRIPT,
        LUA_SCRIPT_GET_FINAL_ORDER,
    },
    proc::BallotProcessor,
    state::{AppDatabase, AppState, RedisService},
    topic::TopicService,
};

mod api;
mod constants;
mod error;
mod proc;
mod state;
mod topic;
mod utils;

static WORKER_COUNTER: AtomicU8 = AtomicU8::new(0);

pub struct PortableService {
    config: Arc<AppConfig>,
}

impl PortableService {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    async fn setup_database(config: &AppConfig) -> eyre::Result<Arc<AppDatabase>> {
        let database_config = &config.database;

        let redis_client = redis::Client::open(&*database_config.redis_url)
            .context("failed to create Redis client")?;
        let connection = redis_client.get_multiplexed_async_connection().await?;

        let mongodb_client = mongodb::Client::with_uri_str(&database_config.mongodb_url)
            .await
            .context("failed to connect to MongoDB")?;

        let mongo_database = mongodb_client.database(&database_config.mongodb_database);

        Ok(Arc::new(AppDatabase {
            redis: RedisService {
                client: redis_client,
                connection,

                final_order_script: redis::Script::new(LUA_SCRIPT_GET_FINAL_ORDER),

                batch_ip_counter_script: redis::Script::new(LUA_SCRIPT_BATCH_IP_COUNTER_SCRIPT),
                batch_score_update_script: redis::Script::new(LUA_SCRIPT_BATCH_SCORE_UPDATE_SCRIPT),
            },
            mongo_database,
        }))
    }

    pub async fn run(self, _shutdown_rx: share::signal::ShutdownRx) -> eyre::Result<()> {
        let database = Self::setup_database(&self.config).await?;

        let collection = database.mongo_database.collection::<VotingTopic>("topics");

        for preset_topic in &self.config.vote.preset_vote_topic {
            let filter = doc! { "id": &preset_topic.id };

            match collection.find_one(filter).await {
                Ok(Some(_)) => {
                    let query = doc! { "id": &preset_topic.id };
                    collection.replace_one(query, preset_topic).await?;
                    tracing::info!("updated preset voting topic: {}", preset_topic.id);
                }
                Ok(None) => {
                    tracing::info!("inserting preset voting topic: {}", preset_topic.id);
                    collection.insert_one(preset_topic).await?;
                }
                Err(_) => {
                    // maybe data structure has changed, we force replace
                    let query = doc! { "id": &preset_topic.id };
                    collection.replace_one(query, preset_topic).await?;
                }
            }
        }

        let character_infos: Vec<CharacterInfo> = utils::load_character_table()?
            .into_iter()
            .filter_map(|(name, data)| {
                if !name.starts_with("char_") {
                    return None;
                }
                let mut parts = name["char_".len()..].splitn(2, '_');
                let charid = parts.next()?.parse::<i32>().ok()?;
                Some(CharacterInfo {
                    id: charid,
                    name: data.name,
                    rarity: data.rarity,
                    profession: data.profession,
                    sub_profession_id: data.sub_profession_id,
                    is_not_obtainable: data.is_not_obtainable,
                })
            })
            .collect();
        tracing::debug!("Character infos loaded: {}", character_infos.len());

        let character_portraits = utils::fetch_portrait_image_url().await?;
        tracing::debug!("Character portraits fetched");

        let topic_service = Arc::new(TopicService::new(database.mongo_database.clone()));
        tracing::debug!("TopicService initialized");

        let ballot_processor =
            Arc::new(BallotProcessor::new(database.clone(), self.config.clone()).await);
        tracing::debug!("BallotProcessor initialized");

        let ballot_store = Arc::new(DashMap::new());
        tracing::debug!("Ballot store initialized");

        let bind_addr = self
            .config
            .server
            .address()
            .parse::<SocketAddr>()
            .context("invalid bind address")?;
        tracing::debug!("Parsed bind address: {}", bind_addr);

        actix_web::HttpServer::new(move || {
            let worker_id = WORKER_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let snowflake = Snowflake::new(
                self.config.snowflake.datacenter_id,
                worker_id,
                self.config.snowflake.epoch,
            );

            let state = AppState {
                database: database.clone(),
                snowflake,
                character_infos: character_infos.clone(),
                character_portraits: character_portraits.clone(),
                topic_service: topic_service.clone(),
                ballot_cache_store: ballot_store.clone(),
                ballot_processor: ballot_processor.clone(),
            };

            let state = web::Data::new(state);

            let mut cors = Cors::default();
            if self.config.cors.allow_origin.iter().any(|o| o == "*") {
                cors = cors.send_wildcard();
            } else {
                for origin in &self.config.cors.allow_origin {
                    cors = cors.allowed_origin(origin);
                }
            }
            cors = cors.allowed_methods(self.config.cors.allow_methods.iter().map(|m| m.as_str()));
            cors = cors.allowed_headers(vec![header::AUTHORIZATION, header::ACCEPT]);
            cors = cors.allowed_header(header::CONTENT_TYPE);
            cors = cors.supports_credentials();
            cors = cors.max_age(3600);

            actix_web::App::new()
                .route("/", actix_web::web::get().to(|| async { "Hello, Actix!" }))
                .service(ballot_create_fn)
                .service(ballot_save_fn)
                .service(ballot_skip_fn)
                .service(results_1v1_matrix_fn)
                .service(results_final_order_fn)
                .service(topic_candidate_pool_fn)
                .service(topic_create_fn)
                .service(topic_info_fn)
                .service(topic_list_active_fn)
                .service(audit_topic_fn)
                .service(audit_topics_list_fn)
                .app_data(state)
                .wrap(cors)
                .wrap(middleware::Compress::default())
                .wrap(middleware::NormalizePath::trim())
                .wrap(middleware::Logger::default())
        })
        .bind(bind_addr)?
        .run()
        .await?;

        Ok(())
    }
}

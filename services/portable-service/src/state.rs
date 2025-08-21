use std::{collections::HashMap, sync::Arc};

use dashmap::DashMap;
use share::{
    models::{api::CharacterPortrait, excel::CharacterInfo},
    snowflake::Snowflake,
};

use crate::{proc::BallotProcessor, topic::TopicService};

#[derive(Clone)]
pub struct RedisService {
    pub client: redis::Client,
    pub connection: redis::aio::MultiplexedConnection,

    pub final_order_script: redis::Script,

    pub batch_ip_counter_script: redis::Script,
    pub batch_score_update_script: redis::Script,
}

#[derive(Clone)]
pub struct AppDatabase {
    pub redis: RedisService,
    pub mongo_database: mongodb::Database,
}

pub struct AppState {
    pub database: Arc<AppDatabase>,
    pub snowflake: Snowflake,

    pub character_infos: Vec<CharacterInfo>,
    pub character_portraits: HashMap<i32, CharacterPortrait>,

    pub topic_service: Arc<TopicService>,
    pub ballot_cache_store: Arc<DashMap<String, String>>,
    pub ballot_processor: Arc<BallotProcessor>,
}

use std::{collections::HashMap, sync::Arc};

use moka::future::Cache;
use share::{
    models::{
        api::{CharacterPortrait, Results1v1MatrixResponse, ResultsFinalOrderResponse},
        excel::CharacterInfo,
    },
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
    pub batch_record_1v1_script: redis::Script,
}

#[derive(Clone)]
pub struct AppDatabase {
    pub redis: RedisService,
    pub mongo_database: mongodb::Database,
}

#[derive(Clone, Default)]
pub struct ResultsCacheValue {
    pub final_order: Option<Arc<ResultsFinalOrderResponse>>,
    pub matrix: Option<Arc<Results1v1MatrixResponse>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResultsType {
    FinalOrder,
    Matrix1v1,
}

pub struct AppState {
    pub database: AppDatabase,
    pub snowflake: Snowflake,

    pub character_infos: Vec<CharacterInfo>,
    pub character_portraits: HashMap<i32, CharacterPortrait>,

    pub topic_service: Arc<TopicService>,
    pub ballot_cache_store: Cache<String, (i32, i32), ahash::RandomState>,
    pub results_cache_store: Cache<(String, ResultsType), ResultsCacheValue, ahash::RandomState>,
    pub ballot_processor: Arc<BallotProcessor>,
}

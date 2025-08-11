use std::collections::HashMap;

use share::{
    models::{api::CharacterPortrait, excel::CharacterInfo},
    snowflake::Snowflake,
};

use crate::service::TopicService;

#[derive(Clone)]
pub struct RedisService {
    pub _client: redis::Client,
    pub connection: redis::aio::MultiplexedConnection,
    pub final_order_script: redis::Script,
}

#[derive(Clone)]
pub struct AppState {
    pub redis: RedisService,
    pub _mongodb: mongodb::Database,
    pub jetstream: async_nats::jetstream::Context,
    pub snowflake: Snowflake,

    pub character_infos: Vec<CharacterInfo>,
    pub character_portraits: HashMap<i32, CharacterPortrait>,

    pub topic_service: TopicService,
}

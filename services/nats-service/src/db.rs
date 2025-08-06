#[derive(Clone)]
pub struct RedisService {
    pub client: redis::Client,
    pub score_update_script: redis::Script,
    pub ip_counter_script: redis::Script,
    pub batch_ip_counter_script: redis::Script,
    pub batch_score_update_script: redis::Script,
    pub get_del_many_script: redis::Script,
    pub del_multiple_script: redis::Script,
}

#[derive(Clone)]
pub struct AppDatabase {
    pub redis: RedisService,
    pub mongo_database: mongodb::Database,
    pub jetstream: async_nats::jetstream::Context,
}

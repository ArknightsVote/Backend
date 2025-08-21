use actix_web::ResponseError;
use redis::RedisError;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("redis error: {0}")]
    Redis(#[from] RedisError),
    #[error("snowflake error: {0}")]
    Snowflake(#[from] share::snowflake::SnowflakeError),
    #[error("serde json error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("insufficient operators available for comparison")]
    InsufficientOperators,
    #[error("mongo db error: {0}")]
    MongoDb(#[from] mongodb::error::Error),
    #[error("missing character table json file")]
    MissingCharacterTableJson,
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
}

impl ResponseError for AppError {}

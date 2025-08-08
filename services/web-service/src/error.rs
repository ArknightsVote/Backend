use axum::{Json, http::StatusCode};
use redis::RedisError;
use share::models::api::{ApiMsg, ApiResponse};

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("redis error: {0}")]
    Redis(#[from] RedisError),
    #[error("winner cannot be loser")]
    SameParticipant,
    #[error("snowflake error: {0}")]
    Snowflake(#[from] share::snowflake::SnowflakeError),
    #[error("jetstream error: {0}")]
    JetStream(#[from] async_nats::error::Error<async_nats::jetstream::context::PublishErrorKind>),
    #[error("serde json error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("internal error: {0}")]
    InternalError(String),
    #[error("insufficient operators available for comparison")]
    InsufficientOperators,
    #[error("mongo db error: {0}")]
    MongoDb(#[from] mongodb::error::Error),
    #[error("missing character table json file")]
    MissingCharacterTableJson,
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            AppError::SameParticipant => (
                StatusCode::BAD_REQUEST,
                ApiResponse::<()> {
                    status: 500,
                    data: None,
                    message: ApiMsg::BallotWinnerCannotBeLoser,
                },
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiResponse::<()> {
                    status: 500,
                    data: None,
                    message: ApiMsg::InternalError,
                },
            ),
        };

        (status, Json(message)).into_response()
    }
}

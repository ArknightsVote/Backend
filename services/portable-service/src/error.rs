use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use redis::RedisError;
use share::models::api::{ApiData, ApiMsg, ApiResponse};

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

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        let (status_code, error_msg) = match self {
            AppError::Redis(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database connection error",
            ),
            AppError::Snowflake(_) => (StatusCode::INTERNAL_SERVER_ERROR, "ID generation error"),
            AppError::SerdeJson(_) => (StatusCode::BAD_REQUEST, "Invalid JSON format"),
            AppError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "File system error"),
            AppError::InsufficientOperators => (
                StatusCode::BAD_REQUEST,
                "Insufficient operators available for comparison",
            ),
            AppError::MongoDb(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database operation failed",
            ),
            AppError::MissingCharacterTableJson => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Missing character table configuration",
            ),
            AppError::Reqwest(_) => (StatusCode::BAD_GATEWAY, "External service unavailable"),
        };

        let error_response = ApiResponse {
            status: status_code.as_u16() as i32,
            data: ApiData::Empty::<()>,
            message: ApiMsg::Error(error_msg.to_string()),
        };

        HttpResponse::build(status_code).json(error_response)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Redis(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Snowflake(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SerdeJson(_) => StatusCode::BAD_REQUEST,
            AppError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::InsufficientOperators => StatusCode::BAD_REQUEST,
            AppError::MongoDb(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::MissingCharacterTableJson => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Reqwest(_) => StatusCode::BAD_GATEWAY,
        }
    }
}

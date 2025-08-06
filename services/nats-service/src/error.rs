use redis::RedisError;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("redis error: {0}")]
    Redis(#[from] RedisError),
    #[error("nats error: {0}")]
    Nats(#[from] async_nats::Error),
    #[error("invalid ballot code: {0}")]
    InvalidBallotCode(String),
    #[error("invalid ballot format")]
    InvalidBallotFormat(String),
    #[error("invalid match participants")]
    InvalidParticipants,
    #[error("jetStream error: {0}")]
    JetStream(#[from] async_nats::error::Error<async_nats::jetstream::context::PublishErrorKind>),
    #[error("serde JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("nats consumer error: {0}")]
    NatsConsumer(
        #[from] async_nats::error::Error<async_nats::jetstream::stream::ConsumerErrorKind>,
    ),
    #[error("nats batch error: {0}")]
    NatsBatch(
        #[from] async_nats::error::Error<async_nats::jetstream::consumer::pull::BatchErrorKind>,
    ),
    #[error("nats create stream error: {0}")]
    NatsCreateStream(
        #[from] async_nats::error::Error<async_nats::jetstream::context::CreateStreamErrorKind>,
    ),
    #[error("mongodb error: {0}")]
    MongoDB(#[from] mongodb::error::Error),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

impl AppError {
    pub fn is_need_send_to_dlq(&self) -> bool {
        !matches!(
            self,
            AppError::InvalidParticipants
                | AppError::InvalidBallotCode(_)
                | AppError::InvalidBallotFormat(_)
        )
    }
}

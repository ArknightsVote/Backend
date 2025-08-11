use std::{borrow::Cow, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose};
use futures::StreamExt as _;
use serde::{Deserialize, Serialize};
use share::config::AppConfig;

use crate::{constants::CONSUMER_RETRY_DELAY, db::AppDatabase, error::AppError};

use super::normalize_subject;

#[derive(Debug, Deserialize, Serialize)]
pub struct DeadLetterMessage {
    pub original_payload: String, // base64 encoded
    pub error_message: String,
    pub retry_count: u32,
    pub first_error_timestamp: i64,
    pub last_error_timestamp: i64,
    pub subject: async_nats::Subject,
}

pub async fn dlq_consumer(
    filter_subject: Cow<'_, str>,
    stream: async_nats::jetstream::stream::Stream,
    database: Arc<AppDatabase>,
    _app_config: Arc<AppConfig>,
) -> Result<(), AppError> {
    let normalized_subject = normalize_subject(&filter_subject);
    let process_name = format!("{normalized_subject}-consumer");

    let consumer = stream
        .create_consumer(async_nats::jetstream::consumer::pull::Config {
            durable_name: Some(normalized_subject),
            filter_subject: filter_subject.to_string(),
            inactive_threshold: Duration::from_secs(60),
            ..Default::default()
        })
        .await?;

    std::thread::Builder::new()
        .name(process_name.to_string())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name_fn(move || {
                    static ATOMIC_ID: std::sync::atomic::AtomicUsize =
                        std::sync::atomic::AtomicUsize::new(0);
                    let id = ATOMIC_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    format!("{process_name}-{id}")
                })
                .build()
                .unwrap();

            runtime.block_on(async {
                tokio::select! {
                    res = process_dead_letter_queue(&consumer, &database) => {
                        if let Err(e) = res {
                            tracing::error!("error in process_dead_letter_queue: {}", e);
                            tokio::time::sleep(CONSUMER_RETRY_DELAY).await;
                        }
                    },
                }
            });
        })?;

    Ok(())
}

async fn process_dead_letter_queue(
    consumer: &async_nats::jetstream::consumer::Consumer<
        async_nats::jetstream::consumer::pull::Config,
    >,
    database: &AppDatabase,
) -> Result<(), AppError> {
    let mut messages = consumer.fetch().max_messages(10).messages().await?;

    while let Some(message) = messages.next().await {
        match message {
            Ok(msg) => match process_dead_letter_message(&msg.payload, database).await {
                Ok(_) => {
                    tracing::info!("successfully processed DLQ message");
                    if let Err(e) = msg.double_ack().await {
                        tracing::error!("failed to acknowledge DLQ message: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "failed to process DLQ message: {}. acknowledging to prevent infinite loop.",
                        e
                    );
                    if let Err(ack_err) = msg.double_ack().await {
                        tracing::error!("failed to acknowledge failed DLQ message: {}", ack_err);
                    }
                }
            },
            Err(e) => {
                tracing::error!("error getting DLQ message: {}", e);
                continue;
            }
        }
    }

    Ok(())
}

async fn process_dead_letter_message(
    payload: &[u8],
    database: &AppDatabase,
) -> Result<(), AppError> {
    let dlq_message: DeadLetterMessage = serde_json::from_slice(payload)?;

    let dlq_collection = database
        .mongo_database
        .collection::<DeadLetterMessage>("dead_letter_queue");

    dlq_collection.insert_one(&dlq_message).await?;

    tracing::info!("dead letter message logged to MongoDB for analysis");

    let original_payload = general_purpose::STANDARD
        .decode(&dlq_message.original_payload)
        .map_err(|e| AppError::InvalidBallotFormat(format!("failed to decode DLQ payload: {e}")))?;

    tracing::warn!(
        "dead letter message processed: subject={}, error={}, payload={:?}",
        dlq_message.subject,
        dlq_message.error_message,
        original_payload
    );

    Ok(())
}

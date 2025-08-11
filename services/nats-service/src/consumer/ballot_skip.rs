use std::{borrow::Cow, sync::Arc, time::Duration};

use futures::StreamExt as _;
use share::{config::AppConfig, models::api::BallotSkipRequest};

use crate::{
    AppDatabase,
    constants::{CONSUMER_BATCH_SIZE, CONSUMER_RETRY_DELAY},
    error::AppError,
};

use super::normalize_subject;

pub async fn ballot_skip_consumer(
    filter_subject: Cow<'static, str>,
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

    let mut conn = database
        .redis
        .client
        .get_multiplexed_async_connection()
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
                    res = process_ballot_skip(&consumer, &mut conn, &database.redis.del_multiple_script) => {
                        if let Err(e) = res {
                            tracing::error!("error in process_ballot_skip: {}", e);
                            tokio::time::sleep(CONSUMER_RETRY_DELAY).await;
                        }
                    },
                }
            });
        })?;

    Ok(())
}

async fn process_ballot_skip(
    consumer: &async_nats::jetstream::consumer::Consumer<
        async_nats::jetstream::consumer::pull::Config,
    >,
    conn: &mut redis::aio::MultiplexedConnection,
    del_multiple_script: &redis::Script,
) -> Result<(), AppError> {
    let mut count = 0;
    let mut batch_messages = Vec::with_capacity(CONSUMER_BATCH_SIZE);

    loop {
        let mut messages = consumer
            .fetch()
            .max_messages(CONSUMER_BATCH_SIZE)
            .messages()
            .await?;

        while let Some(message) = messages.next().await {
            match message {
                Ok(msg) => {
                    let data: BallotSkipRequest = match serde_json::from_slice(&msg.payload) {
                        Ok(data) => data,
                        Err(e) => {
                            tracing::error!(
                                "error deserializing ballot skip request message: {}",
                                e
                            );
                            msg.double_ack().await?;
                            continue;
                        }
                    };
                    batch_messages.push(data);
                }
                Err(e) => {
                    tracing::error!("error fetching ballot skip request message: {}", e);
                    continue;
                }
            }
        }

        if batch_messages.is_empty() {
            continue;
        }

        let processed_count = batch_messages.len();
        count += processed_count;

        let ballot_keys = batch_messages
            .iter()
            .map(|msg| format!("{}:ballot:{}", msg.topic_id, msg.ballot_id))
            .collect::<Vec<_>>();

        let _: () = del_multiple_script
            .key(ballot_keys)
            .invoke_async(conn)
            .await?;

        if count % 1000 == 0 || processed_count > 0 {
            tracing::debug!("processed {} ballot skip request messages", count);
        }

        batch_messages.clear();
    }
}

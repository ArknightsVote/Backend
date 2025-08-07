use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use base64::{Engine as _, engine::general_purpose};
use futures::StreamExt as _;
use redis::AsyncCommands as _;
use share::{
    config::{AppConfig, VoteConfig},
    models::database::{
        Ballot, GroupwiseBallot, PairwiseBallot, PluralityBallot, SetwiseBallot, StoredBallot,
    },
};

use crate::{
    AppDatabase,
    constants::{CONSUMER_BATCH_SIZE, CONSUMER_RETRY_DELAY, DLQ_MAX_RETRIES, DLQ_RETRY_DELAY},
    consumer::dlq::DeadLetterMessage,
    error::AppError,
};

use super::normalize_subject;

#[derive(Debug, Default)]
struct BatchProcessResult {
    success_count: usize,
    failed_messages: Vec<async_nats::jetstream::Message>,
}

pub async fn save_score_consumer(
    filter_subject: Cow<'static, str>,
    stream: async_nats::jetstream::stream::Stream,
    database: Arc<AppDatabase>,
    app_config: Arc<AppConfig>,
) -> Result<(), AppError> {
    let normalized_subject = normalize_subject(&filter_subject);
    let process_name = format!("{normalized_subject}-consumer");

    let consumer = stream
        .create_consumer(async_nats::jetstream::consumer::pull::Config {
            durable_name: Some(normalized_subject),
            filter_subject: filter_subject.to_string(),
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
                    res = process_save_score_messages(&consumer, &mut conn, &database, &app_config) => {
                        if let Err(e) = res {
                            tracing::error!("error in process_save_score_messages: {}", e);
                            tokio::time::sleep(CONSUMER_RETRY_DELAY).await;
                        }
                    },
                }
            });
        })?;

    Ok(())
}

struct PairwiseBallotItem<'a> {
    ballot: PairwiseBallot<'a>,
    message: async_nats::jetstream::Message,
}

struct SetwiseBallotItem<'a> {
    ballot: SetwiseBallot<'a>,
    _message: async_nats::jetstream::Message,
}

struct GroupwiseBallotItem<'a> {
    ballot: GroupwiseBallot<'a>,
    _message: async_nats::jetstream::Message,
}

struct PluralityBallotItem<'a> {
    ballot: PluralityBallot<'a>,
    _message: async_nats::jetstream::Message,
}

struct BallotMessageGroup<'a> {
    pairwise: Vec<PairwiseBallotItem<'a>>,
    setwise: Vec<SetwiseBallotItem<'a>>,
    groupwise: Vec<GroupwiseBallotItem<'a>>,
    plurality: Vec<PluralityBallotItem<'a>>,
}

impl<'a> BallotMessageGroup<'a> {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            pairwise: Vec::with_capacity(capacity),
            setwise: Vec::with_capacity(capacity),
            groupwise: Vec::with_capacity(capacity),
            plurality: Vec::with_capacity(capacity),
        }
    }

    fn add(
        &mut self,
        message: async_nats::jetstream::Message,
    ) -> Option<async_nats::jetstream::Message> {
        match serde_json::from_slice::<Ballot>(&message.payload) {
            Ok(Ballot::Pairwise(ballot)) => {
                self.pairwise.push(PairwiseBallotItem { ballot, message });
                None
            }
            Ok(Ballot::Setwise(ballot)) => {
                self.setwise.push(SetwiseBallotItem {
                    ballot,
                    _message: message,
                });
                None
            }
            Ok(Ballot::Groupwise(ballot)) => {
                self.groupwise.push(GroupwiseBallotItem {
                    ballot,
                    _message: message,
                });
                None
            }
            Ok(Ballot::Plurality(ballot)) => {
                self.plurality.push(PluralityBallotItem {
                    ballot,
                    _message: message,
                });
                None
            }
            Err(e) => {
                tracing::warn!("invalid ballot format: {}. acknowledging message.", e);
                Some(message)
            }
        }
    }

    fn take_all(
        &mut self,
    ) -> (
        Vec<PairwiseBallotItem<'a>>,
        Vec<SetwiseBallotItem<'a>>,
        Vec<GroupwiseBallotItem<'a>>,
        Vec<PluralityBallotItem<'a>>,
    ) {
        (
            std::mem::take(&mut self.pairwise),
            std::mem::take(&mut self.setwise),
            std::mem::take(&mut self.groupwise),
            std::mem::take(&mut self.plurality),
        )
    }

    fn is_empty(&self) -> bool {
        self.pairwise.is_empty()
            && self.setwise.is_empty()
            && self.groupwise.is_empty()
            && self.plurality.is_empty()
    }
}

async fn process_save_score_messages(
    consumer: &async_nats::jetstream::consumer::Consumer<
        async_nats::jetstream::consumer::pull::Config,
    >,
    conn: &mut redis::aio::MultiplexedConnection,
    database: &AppDatabase,
    app_config: &AppConfig,
) -> Result<(), AppError> {
    let mut count = 0;
    let mut ballot_groups = BallotMessageGroup::with_capacity(CONSUMER_BATCH_SIZE);

    loop {
        let mut messages = consumer
            .fetch()
            .max_messages(CONSUMER_BATCH_SIZE)
            .messages()
            .await?;

        while let Some(message) = messages.next().await {
            match message {
                Ok(msg) => {
                    let invalid_message = ballot_groups.add(msg);
                    if let Some(invalid_msg) = invalid_message {
                        tracing::warn!("invalid ballot format in message, acknowledging.");
                        if let Err(e) = invalid_msg.double_ack().await {
                            tracing::error!("failed to double_ack invalid message: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("error getting message: {}", e);
                    continue;
                }
            }
        }

        if ballot_groups.is_empty() {
            continue;
        }

        let (pairwise, setwise, groupwise, plurality) = ballot_groups.take_all();

        if !setwise.is_empty() {
            let result = process_setwise_ballot_batch(&setwise, conn, database, app_config).await;
            if let Err(e) = result {
                tracing::error!("failed to process setwise ballots: {}", e);
            }
        }

        if !groupwise.is_empty() {
            let result =
                process_groupwise_ballot_batch(&groupwise, conn, database, app_config).await;
            if let Err(e) = result {
                tracing::error!("failed to process groupwise ballots: {}", e);
            }
        }

        if !plurality.is_empty() {
            let result =
                process_plurality_ballot_batch(&plurality, conn, database, app_config).await;
            if let Err(e) = result {
                tracing::error!("failed to process plurality ballots: {}", e);
            }
        }

        match process_pairwise_ballot_batch(&pairwise, conn, database, app_config).await {
            Ok(result) => {
                count += result.success_count;

                if count % 1000 == 0 || result.success_count > 0 {
                    tracing::debug!("processed {} save score messages", count);
                }

                for msg in result.failed_messages {
                    tracing::error!("failed to process ballot: {:?}. Sending to DLQ.", msg);
                    if let Err(e) = handle_failed_messages(
                        &database.jetstream,
                        (&msg, &AppError::InvalidParticipants),
                    )
                    .await
                    {
                        tracing::error!("failed to handle failed message: {}", e);
                    }
                    if let Err(e) = msg.double_ack().await {
                        tracing::error!("failed to double_ack failed message: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("batch processing failed: {}", e);
                for msg in pairwise.iter() {
                    if let Err(e) =
                        process_single_pairwise_fallback(msg, conn, database, app_config).await
                    {
                        tracing::error!("fallback processing failed: {}", e);
                    }
                }
            }
        }
    }
}

async fn process_pairwise_ballot_batch(
    ballots: &[PairwiseBallotItem<'_>],
    conn: &mut redis::aio::MultiplexedConnection,
    database: &AppDatabase,
    app_config: &AppConfig,
) -> Result<BatchProcessResult, AppError> {
    if ballots.is_empty() {
        return Ok(BatchProcessResult::default());
    }

    let vote_config = &app_config.vote;
    let mut failed_messages = Vec::new();

    // 第一步：批量验证ballot codes
    let validation_results =
        validate_pairwise_ballots(ballots, &database.redis.get_del_many_script, conn).await?;

    // 第二步：批量计算IP倍数
    let ip_multipliers = calculate_pairwise_multipliers(
        ballots,
        vote_config,
        &database.redis.batch_ip_counter_script,
        conn,
    )
    .await?;

    // 第三步：过滤有效的ballot并准备批量操作
    let mut valid_ballots = Vec::new();
    let mut score_updates = HashMap::new(); // (win_id, lose_id) -> total_multiplier

    for item in ballots.iter() {
        // 验证ballot code
        let (ballot_left, ballot_right) =
            match validation_results.get(item.ballot.info.ballot_id.as_ref()) {
                Some(Ok((left, right))) => (*left, *right),
                Some(Err(_)) | None => {
                    failed_messages.push(item.message.clone());
                    continue;
                }
            };

        let valid_ids = [ballot_left, ballot_right];
        if !valid_ids.contains(&item.ballot.win)
            || !valid_ids.contains(&item.ballot.lose)
            || item.ballot.win == item.ballot.lose
        {
            tracing::warn!(
                "invalid ballot participants: win={}, lose={} for code={}",
                item.ballot.win,
                item.ballot.lose,
                item.ballot.info.ballot_id
            );
            failed_messages.push(item.message.clone());
            continue;
        }

        let multiplier = ip_multipliers
            .get(item.ballot.info.ip.as_ref())
            .copied()
            .unwrap_or(vote_config.low_multiplier);

        *score_updates
            .entry((
                item.ballot.info.topic_id.to_string(),
                item.ballot.win,
                item.ballot.lose,
            ))
            .or_insert(0) += multiplier;

        valid_ballots.push(item);
    }

    // 第四步：批量执行分数更新
    batch_update_scores(
        score_updates,
        &database.redis.batch_score_update_script,
        conn,
    )
    .await?;

    // 第五步：批量插入MongoDB
    // 先按照topic_id分组
    let mut grouped_ballots: HashMap<String, Vec<StoredBallot>> = HashMap::new();

    for item in valid_ballots.iter() {
        let topic_id = item.ballot.info.topic_id.to_string();
        let multiplier = ip_multipliers
            .get(item.ballot.info.ip.as_ref())
            .copied()
            .unwrap_or(vote_config.low_multiplier);

        let stored_ballot = StoredBallot {
            ballot: Ballot::Pairwise(item.ballot.clone()),
            multiplier,
        };

        grouped_ballots
            .entry(topic_id)
            .or_default()
            .push(stored_ballot);
    }

    for (topic_id, ballots) in grouped_ballots.into_iter() {
        let ballot_collection = database
            .mongo_database
            .collection::<StoredBallot>(&format!("ballots_{}", topic_id));

        ballot_collection.insert_many(&ballots).await?;
    }

    // 第六步：确认所有成功处理的消息
    for msg in valid_ballots.iter() {
        if let Err(e) = msg.message.double_ack().await {
            tracing::error!("failed to double_ack successful message: {}", e);
        }
    }

    Ok(BatchProcessResult {
        success_count: valid_ballots.len(),
        failed_messages,
    })
}

async fn validate_pairwise_ballots(
    ballots: &[PairwiseBallotItem<'_>],
    get_del_many_script: &redis::Script,
    conn: &mut redis::aio::MultiplexedConnection,
) -> Result<HashMap<String, Result<(i32, i32), AppError>>, AppError> {
    if ballots.is_empty() {
        return Ok(HashMap::new());
    }

    let code_keys: Vec<(String, &str)> = ballots
        .iter()
        .map(|item| {
            let info = &item.ballot.info;

            (
                format!("{}:ballot:{}", info.topic_id, info.ballot_id),
                info.ballot_id.as_ref(),
            )
        })
        .collect();

    let keys: Vec<String> = code_keys.iter().map(|(k, _)| k.clone()).collect();
    let values: Vec<Option<String>> = get_del_many_script.key(&keys).invoke_async(conn).await?;

    fn parse_ballot_info(info: &str) -> Result<(i32, i32), AppError> {
        let parts: Vec<&str> = info.split(',').collect();
        if parts.len() != 2 {
            return Err(AppError::InvalidBallotFormat(
                "expected format: left,right".to_string(),
            ));
        }

        let left = parts[0].parse().map_err(|_| {
            AppError::InvalidBallotFormat("left part is not a valid integer".to_string())
        })?;

        let right = parts[1].parse().map_err(|_| {
            AppError::InvalidBallotFormat("right part is not a valid integer".to_string())
        })?;

        Ok((left, right))
    }

    let results: HashMap<String, Result<(i32, i32), AppError>> = code_keys
        .into_iter()
        .zip(values.into_iter())
        .map(|((_, code), value)| {
            let result = match value {
                Some(info) => parse_ballot_info(&info),
                None => Err(AppError::InvalidBallotCode(code.to_string())),
            };
            (code.to_string(), result)
        })
        .collect();

    Ok(results)
}

async fn calculate_pairwise_multipliers(
    ballots: &[PairwiseBallotItem<'_>],
    vote_config: &VoteConfig,
    batch_ip_counter_script: &redis::Script,
    conn: &mut redis::aio::MultiplexedConnection,
) -> Result<HashMap<String, i32>, AppError> {
    if ballots.is_empty() {
        return Ok(HashMap::new());
    }

    let mut results = HashMap::new();

    let ips: HashSet<&str> = ballots.iter().map(|b| b.ballot.info.ip.as_ref()).collect();

    let keys: Vec<String> = ballots
        .iter()
        .map(|b| {
            let info = &b.ballot.info;
            format!("{}:ip_counter:{}", info.topic_id.as_ref(), info.ip.as_ref())
        })
        .collect();

    let ips_vec: Vec<&str> = ips.into_iter().collect();

    let script_results: Vec<i32> = batch_ip_counter_script
        .key(&keys)
        .arg(vote_config.ip_counter_expire_seconds)
        .arg(vote_config.max_ip_limit)
        .arg(vote_config.base_multiplier)
        .arg(vote_config.low_multiplier)
        .invoke_async(conn)
        .await?;

    for (ip, multiplier) in ips_vec.into_iter().zip(script_results.into_iter()) {
        results.insert(ip.to_string(), multiplier);
    }

    Ok(results)
}

async fn batch_update_scores(
    updates: HashMap<(String, i32, i32), i32>, // ((topic_id, win_id, lose_id), total_multiplier)
    batch_score_update_script: &redis::Script,
    conn: &mut redis::aio::MultiplexedConnection,
) -> Result<(), AppError> {
    if updates.is_empty() {
        return Ok(());
    }

    // 准备参数：topic_id1, win_id1, lose_id1, multiplier1, topic_id2, win_id2, lose_id2, multiplier2, ...
    let mut args = Vec::with_capacity(updates.len() * 4);
    for ((topic_id, win_id, lose_id), multiplier) in updates {
        args.push(topic_id);
        args.push(win_id.to_string());
        args.push(lose_id.to_string());
        args.push(multiplier.to_string());
    }

    // 执行批量分数更新脚本
    let _results: () = batch_score_update_script
        .arg(&args)
        .invoke_async(conn)
        .await?;

    Ok(())
}

async fn process_setwise_ballot_batch(
    ballots: &[SetwiseBallotItem<'_>],
    _conn: &mut redis::aio::MultiplexedConnection,
    database: &AppDatabase,
    _app_config: &AppConfig,
) -> Result<BatchProcessResult, AppError> {
    tracing::debug!("Processing setwise ballot batch, but this feature is not implemented yet.");

    // only save the ballot to mongoDB for now
    let mut grouped_ballots: HashMap<String, Vec<StoredBallot>> = HashMap::new();
    for item in ballots.iter() {
        let topic_id = item.ballot.info.topic_id.to_string();
        let stored_ballot = StoredBallot {
            ballot: Ballot::Setwise(item.ballot.clone()),
            multiplier: 1, // Placeholder multiplier, adjust as needed
        };

        grouped_ballots
            .entry(topic_id)
            .or_default()
            .push(stored_ballot);
    }

    for (topic_id, ballots) in grouped_ballots.into_iter() {
        let ballot_collection = database
            .mongo_database
            .collection::<StoredBallot>(&format!("ballots_{}", topic_id));

        ballot_collection.insert_many(&ballots).await?;
    }

    tracing::debug!(
        "Processed {} setwise ballots, but no score updates were made.",
        ballots.len()
    );

    Ok(BatchProcessResult {
        success_count: ballots.len(),
        failed_messages: Vec::new(), // No failed messages in this case
    })
}

async fn process_groupwise_ballot_batch(
    ballots: &[GroupwiseBallotItem<'_>],
    _conn: &mut redis::aio::MultiplexedConnection,
    database: &AppDatabase,
    _app_config: &AppConfig,
) -> Result<BatchProcessResult, AppError> {
    tracing::debug!("Processing groupwise ballot batch, but this feature is not implemented yet.");

    // only save the ballot to mongoDB for now
    let mut grouped_ballots: HashMap<String, Vec<StoredBallot>> = HashMap::new();
    for item in ballots.iter() {
        let topic_id = item.ballot.info.topic_id.to_string();
        let stored_ballot = StoredBallot {
            ballot: Ballot::Groupwise(item.ballot.clone()),
            multiplier: 1, // Placeholder multiplier, adjust as needed
        };

        grouped_ballots
            .entry(topic_id)
            .or_default()
            .push(stored_ballot);
    }

    for (topic_id, ballots) in grouped_ballots.into_iter() {
        let ballot_collection = database
            .mongo_database
            .collection::<StoredBallot>(&format!("ballots_{}", topic_id));

        ballot_collection.insert_many(&ballots).await?;
    }

    tracing::debug!(
        "Processed {} groupwise ballots, but no score updates were made.",
        ballots.len()
    );

    Ok(BatchProcessResult {
        success_count: ballots.len(),
        failed_messages: Vec::new(), // No failed messages in this case
    })
}

async fn process_plurality_ballot_batch(
    ballots: &[PluralityBallotItem<'_>],
    _conn: &mut redis::aio::MultiplexedConnection,
    database: &AppDatabase,
    _app_config: &AppConfig,
) -> Result<BatchProcessResult, AppError> {
    tracing::debug!("Processing plurality ballot batch, but this feature is not implemented yet.");

    // only save the ballot to mongoDB for now
    let mut grouped_ballots: HashMap<String, Vec<StoredBallot>> = HashMap::new();
    for item in ballots.iter() {
        let topic_id = item.ballot.info.topic_id.to_string();
        let stored_ballot = StoredBallot {
            ballot: Ballot::Plurality(item.ballot.clone()),
            multiplier: 1, // Placeholder multiplier, adjust as needed
        };

        grouped_ballots
            .entry(topic_id)
            .or_default()
            .push(stored_ballot);
    }

    for (topic_id, ballots) in grouped_ballots.into_iter() {
        let ballot_collection = database
            .mongo_database
            .collection::<StoredBallot>(&format!("ballots_{}", topic_id));

        ballot_collection.insert_many(&ballots).await?;
    }

    tracing::debug!(
        "Processed {} plurality ballots, but no score updates were made.",
        ballots.len()
    );

    Ok(BatchProcessResult {
        success_count: ballots.len(),
        failed_messages: Vec::new(), // No failed messages in this case
    })
}

// 回退处理单个消息（当批量处理失败时使用）
async fn process_single_pairwise_fallback(
    msg: &PairwiseBallotItem<'_>,
    conn: &mut redis::aio::MultiplexedConnection,
    database: &AppDatabase,
    app_config: &AppConfig,
) -> Result<(), AppError> {
    match process_single_ballot(&msg.ballot, conn, database, app_config).await {
        Ok(_) => {
            msg.message.double_ack().await?;
        }
        Err(e) => {
            if e.is_need_send_to_dlq() {
                tracing::error!("failed to process ballot: {}. Sending to DLQ.", e);
                handle_failed_messages(&database.jetstream, (&msg.message, &e)).await?;
            } else {
                tracing::warn!(
                    "invalid ballot format or participants: {}. acknowledging message.",
                    e
                );
            }
            msg.message.double_ack().await?;
        }
    }
    Ok(())
}

async fn handle_failed_messages(
    jetstream: &async_nats::jetstream::Context,
    message: (&async_nats::jetstream::Message, &AppError),
) -> Result<(), AppError> {
    let (message, error_info) = message;

    let headers = message.headers.as_ref();
    let retry_count: u32 = headers
        .and_then(|h| h.get("X-Retry-Count"))
        .and_then(|v| v.as_str().parse().ok())
        .unwrap_or(0);

    let first_error_timestamp: i64 = headers
        .and_then(|h| h.get("X-First-error-Timestamp"))
        .and_then(|v| v.as_str().parse().ok())
        .unwrap_or_else(|| chrono::Utc::now().timestamp());

    let current_timestamp = chrono::Utc::now().timestamp();

    if retry_count >= DLQ_MAX_RETRIES {
        let dlq_message = DeadLetterMessage {
            original_payload: general_purpose::STANDARD.encode(&message.payload),
            error_message: format!("max retries exceeded. Last error: {error_info}"),
            retry_count,
            first_error_timestamp,
            last_error_timestamp: current_timestamp,
            subject: message.subject.clone(),
        };

        let dlq_payload = serde_json::to_vec(&dlq_message)?;

        if let Err(e) = jetstream.publish("ark-vote.dlq", dlq_payload.into()).await {
            tracing::error!("failed to publish message to DLQ: {}", e);
        } else {
            tracing::info!("message sent to DLQ after {} retries", retry_count);
        }

        if let Err(e) = message.double_ack().await {
            tracing::error!("failed to acknowledge DLQ message: {}", e);
        }
    } else {
        let mut headers = async_nats::HeaderMap::new();
        headers.insert("X-Retry-Count", (retry_count + 1).to_string().as_str());
        headers.insert(
            "X-First-error-Timestamp",
            first_error_timestamp.to_string().as_str(),
        );
        headers.insert("X-Last-error", error_info.to_string().as_str());

        tokio::time::sleep(DLQ_RETRY_DELAY).await;

        if let Err(e) = jetstream
            .publish_with_headers("ark-vote.save_score", headers, message.payload.clone())
            .await
        {
            tracing::error!("failed to republish message for retry: {}", e);
            let _ = message.double_ack().await;
        } else {
            tracing::debug!("message republished for retry #{}", retry_count + 1);
            if let Err(e) = message.double_ack().await {
                tracing::error!("failed to acknowledge retried message: {}", e);
            }
        }
    }

    Ok(())
}

async fn process_single_ballot(
    ballot: &PairwiseBallot<'_>,
    conn: &mut redis::aio::MultiplexedConnection,
    database: &AppDatabase,
    app_config: &AppConfig,
) -> Result<(), AppError> {
    let vote_config = &app_config.vote;

    let (ballot_left, ballot_right) = validate_ballot(ballot.info.ballot_id.as_ref(), conn).await?;

    let valid_ids = [ballot_left, ballot_right];
    if !valid_ids.contains(&ballot.win)
        || !valid_ids.contains(&ballot.lose)
        || ballot.win == ballot.lose
    {
        tracing::warn!(
            "invalid ballot participants: win={}, lose={} for code={}",
            ballot.win,
            ballot.lose,
            ballot.info.ballot_id
        );
        return Err(AppError::InvalidParticipants);
    }

    let multiplier = calculate_multiplier(
        ballot.info.ip.as_ref(),
        vote_config,
        &database.redis.ip_counter_script,
        conn,
    )
    .await?;

    let _: () = database
        .redis
        .score_update_script
        .arg(ballot.win)
        .arg(ballot.lose)
        .arg(multiplier)
        .invoke_async(conn)
        .await?;

    let ballot = Ballot::Pairwise(ballot.clone());
    let stored_ballot = StoredBallot { ballot, multiplier };

    let ballot_collection = database
        .mongo_database
        .collection::<StoredBallot>("ballots");
    ballot_collection.insert_one(&stored_ballot).await?;

    Ok(())
}

async fn validate_ballot(
    code: &str,
    conn: &mut redis::aio::MultiplexedConnection,
) -> Result<(i32, i32), AppError> {
    let ballot_key = format!("ballot:{code}");
    let value: Option<String> = conn.get_del(&ballot_key).await?;

    match value {
        Some(info) => {
            let parts: Vec<&str> = info.split(',').collect();
            if parts.len() != 2 {
                return Err(AppError::InvalidBallotFormat(
                    "expected format: left,right".to_string(),
                ));
            }

            let left = parts[0].parse().map_err(|_| {
                AppError::InvalidBallotFormat("expected format: left,right".to_string())
            })?;
            let right = parts[1].parse().map_err(|_| {
                AppError::InvalidBallotFormat("expected format: left,right".to_string())
            })?;

            Ok((left, right))
        }
        None => Err(AppError::InvalidBallotCode(code.to_string())),
    }
}

async fn calculate_multiplier(
    ip: &str,
    vote_config: &VoteConfig,
    ip_counter_script: &redis::Script,
    conn: &mut redis::aio::MultiplexedConnection,
) -> Result<i32, AppError> {
    let counter_key = format!("ip_counter:{ip}");

    let multiplier: i32 = ip_counter_script
        .key(&counter_key)
        .arg(vote_config.ip_counter_expire_seconds)
        .arg(vote_config.max_ip_limit)
        .arg(vote_config.base_multiplier)
        .arg(vote_config.low_multiplier)
        .invoke_async(conn)
        .await?;

    Ok(multiplier)
}

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
    models::database::{Ballot, StoredBallot},
};

use crate::{
    AppDatabase,
    constants::{CONSUMER_BATCH_SIZE, CONSUMER_RETRY_DELAY, DLQ_MAX_RETRIES, DLQ_RETRY_DELAY},
    consumer::dlq::DeadLetterMessage,
    error::AppError,
};

use super::normalize_subject;

#[derive(Debug)]
struct BatchProcessResult {
    success_count: usize,
    failed_messages: Vec<async_nats::jetstream::Message>,
    invalid_messages: Vec<async_nats::jetstream::Message>,
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

async fn process_save_score_messages(
    consumer: &async_nats::jetstream::consumer::Consumer<
        async_nats::jetstream::consumer::pull::Config,
    >,
    conn: &mut redis::aio::MultiplexedConnection,
    database: &AppDatabase,
    app_config: &AppConfig,
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
                Ok(msg) => batch_messages.push(msg),
                Err(e) => {
                    tracing::error!("error getting message: {}", e);
                    continue;
                }
            }
        }

        if batch_messages.is_empty() {
            continue;
        }

        match process_ballot_batch(&batch_messages, conn, database, app_config).await {
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

                // 处理无效消息（直接确认）
                for msg in result.invalid_messages {
                    tracing::warn!(
                        "invalid ballot format or participants: {:?}. acknowledging message.",
                        msg
                    );
                    if let Err(e) = msg.double_ack().await {
                        tracing::error!("failed to double_ack invalid message: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("batch processing failed: {}", e);
                // 如果批量处理失败，回退到单个处理
                for msg in batch_messages.iter() {
                    if let Err(e) =
                        process_single_message_fallback(msg, conn, database, app_config).await
                    {
                        tracing::error!("fallback processing failed: {}", e);
                    }
                }
            }
        }

        batch_messages.clear();
    }
}

async fn process_ballot_batch(
    messages: &[async_nats::jetstream::Message],
    conn: &mut redis::aio::MultiplexedConnection,
    database: &AppDatabase,
    app_config: &AppConfig,
) -> Result<BatchProcessResult, AppError> {
    let vote_config = &app_config.vote;

    let mut ballots = Vec::new();
    let mut message_ballot_map = HashMap::new();
    let mut invalid_messages = Vec::new();
    let mut failed_messages = Vec::new();

    // 第一步：解析所有ballot
    for (idx, msg) in messages.iter().enumerate() {
        match serde_json::from_slice::<Ballot>(&msg.payload) {
            Ok(ballot) => {
                let ballot = match ballot {
                    Ballot::Pairwise(ballot) => ballot,
                    _ => {
                        tracing::warn!("unsupported ballot type in batch processing");
                        continue;
                    }
                };
                ballots.push((idx, ballot));
                message_ballot_map.insert(idx, msg);
            }
            Err(e) => {
                tracing::warn!("invalid ballot format: {}. acknowledging message.", e);
                invalid_messages.push(msg.clone());
            }
        }
    }

    if ballots.is_empty() {
        return Ok(BatchProcessResult {
            success_count: 0,
            failed_messages,
            invalid_messages,
        });
    }

    // 第二步：批量验证ballot codes
    let ballot_codes: HashSet<&str> = ballots
        .iter()
        .map(|(_, b)| b.info.ballot_id.as_ref())
        .collect();
    let validation_results =
        batch_validate_ballots(ballot_codes, &database.redis.get_del_many_script, conn).await?;

    // 第三步：批量计算IP倍数
    let ips: HashSet<&str> = ballots.iter().map(|(_, b)| b.info.ip.as_ref()).collect();
    let ip_multipliers = batch_calculate_multipliers(
        ips,
        vote_config,
        &database.redis.batch_ip_counter_script,
        conn,
    )
    .await?;

    // 第四步：过滤有效的ballot并准备批量操作
    let mut valid_ballots = Vec::new();
    let mut score_updates = HashMap::new(); // (win_id, lose_id) -> total_multiplier
    let mut stored_ballots = Vec::new();

    for (idx, ballot) in ballots {
        let msg = message_ballot_map.remove(&idx).unwrap();

        // 验证ballot code
        let (ballot_left, ballot_right) =
            match validation_results.get(ballot.info.ballot_id.as_ref()) {
                Some(Ok((left, right))) => (*left, *right),
                Some(Err(_)) | None => {
                    failed_messages.push(msg.clone());
                    continue;
                }
            };

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
            invalid_messages.push(msg.clone());
            continue;
        }

        let multiplier = ip_multipliers
            .get(ballot.info.ip.as_ref())
            .copied()
            .unwrap_or(vote_config.low_multiplier);

        *score_updates.entry((ballot.win, ballot.lose)).or_insert(0) += multiplier;

        let stored_ballot = StoredBallot {
            ballot: Ballot::Pairwise(ballot),
            multiplier,
        };
        stored_ballots.push(stored_ballot);

        valid_ballots.push(msg);
    }
    let valid_ballots_count = valid_ballots.len();

    // 第五步：批量执行分数更新
    if !score_updates.is_empty() {
        batch_update_scores(
            score_updates,
            valid_ballots_count,
            &database.redis.batch_score_update_script,
            conn,
        )
        .await?;
    }

    // 第六步：批量插入MongoDB
    if !stored_ballots.is_empty() {
        let ballot_collection = database
            .mongo_database
            .collection::<StoredBallot>("ballots");
        ballot_collection.insert_many(&stored_ballots).await?;
    }

    // 第七步：确认所有成功处理的消息
    for msg in valid_ballots {
        if let Err(e) = msg.double_ack().await {
            tracing::error!("failed to double_ack successful message: {}", e);
        }
    }

    Ok(BatchProcessResult {
        success_count: valid_ballots_count,
        failed_messages,
        invalid_messages,
    })
}

async fn batch_validate_ballots(
    codes: HashSet<&str>,
    get_del_many_script: &redis::Script,
    conn: &mut redis::aio::MultiplexedConnection,
) -> Result<HashMap<String, Result<(i32, i32), AppError>>, AppError> {
    if codes.is_empty() {
        return Ok(HashMap::new());
    }

    let code_keys: Vec<(String, &str)> = codes
        .iter()
        .map(|code| (format!("ballot:{code}"), *code))
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

async fn batch_calculate_multipliers(
    ips: HashSet<&str>,
    vote_config: &VoteConfig,
    batch_ip_counter_script: &redis::Script,
    conn: &mut redis::aio::MultiplexedConnection,
) -> Result<HashMap<String, i32>, AppError> {
    let mut results = HashMap::new();

    if ips.is_empty() {
        return Ok(results);
    }

    let keys: Vec<String> = ips.iter().map(|ip| format!("ip_counter:{ip}")).collect();
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
    updates: HashMap<(i32, i32), i32>, // ((win_id, lose_id), total_multiplier)
    valid_ballots_count: usize,
    batch_score_update_script: &redis::Script,
    conn: &mut redis::aio::MultiplexedConnection,
) -> Result<(), AppError> {
    if updates.is_empty() {
        return Ok(());
    }

    // 准备参数：win_id1, lose_id1, multiplier1, win_id2, lose_id2, multiplier2, ...
    let mut args = Vec::with_capacity(updates.len() * 3);
    for ((win_id, lose_id), multiplier) in updates {
        args.push(win_id);
        args.push(lose_id);
        args.push(multiplier);
    }

    // 执行批量分数更新脚本
    let _results: () = batch_score_update_script
        .key(valid_ballots_count as i32) // 传递总有效投票数
        .arg(&args)
        .invoke_async(conn)
        .await?;

    Ok(())
}

// 回退处理单个消息（当批量处理失败时使用）
async fn process_single_message_fallback(
    msg: &async_nats::jetstream::Message,
    conn: &mut redis::aio::MultiplexedConnection,
    database: &AppDatabase,
    app_config: &AppConfig,
) -> Result<(), AppError> {
    match process_single_ballot(&msg.payload, conn, database, app_config).await {
        Ok(_) => {
            msg.double_ack().await?;
        }
        Err(e) => {
            if e.is_need_send_to_dlq() {
                tracing::error!("failed to process ballot: {}. Sending to DLQ.", e);
                handle_failed_messages(&database.jetstream, (msg, &e)).await?;
            } else {
                tracing::warn!(
                    "invalid ballot format or participants: {}. acknowledging message.",
                    e
                );
            }
            msg.double_ack().await?;
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
    payload: &[u8],
    conn: &mut redis::aio::MultiplexedConnection,
    database: &AppDatabase,
    app_config: &AppConfig,
) -> Result<(), AppError> {
    let vote_config = &app_config.vote;
    let ballot: Ballot = serde_json::from_slice(payload)?;
    let ballot = match ballot {
        Ballot::Pairwise(ballot) => ballot,
        _ => {
            tracing::warn!("unsupported ballot type in single processing");
            return Err(AppError::InvalidBallotFormat(
                "unsupported ballot type".to_string(),
            ));
        }
    };

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

    let ballot = Ballot::Pairwise(ballot);
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

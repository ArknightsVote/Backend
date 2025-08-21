use std::{
    collections::{HashMap, HashSet},
    io::Write,
    sync::Arc,
    thread,
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender, unbounded};
use share::{
    config::{AppConfig, VoteConfig},
    models::database::{
        Ballot, GroupwiseBallot, PairwiseBallot, PluralityBallot, SetwiseBallot, StoredBallot,
    },
};

use crate::{error::AppError, state::AppDatabase};

struct BallotMessageGroup<'a> {
    pairwise: Vec<PairwiseBallot<'a>>,
    setwise: Vec<SetwiseBallot<'a>>,
    groupwise: Vec<GroupwiseBallot<'a>>,
    plurality: Vec<PluralityBallot<'a>>,

    capacity: usize,
}

impl<'a> BallotMessageGroup<'a> {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            pairwise: Vec::with_capacity(capacity),
            setwise: Vec::with_capacity(capacity),
            groupwise: Vec::with_capacity(capacity),
            plurality: Vec::with_capacity(capacity),

            capacity,
        }
    }

    fn add(&mut self, message: Ballot<'a>) {
        match message {
            Ballot::Pairwise(ballot) => {
                self.pairwise.push(ballot);
            }
            Ballot::Setwise(ballot) => {
                self.setwise.push(ballot);
            }
            Ballot::Groupwise(ballot) => {
                self.groupwise.push(ballot);
            }
            Ballot::Plurality(ballot) => {
                self.plurality.push(ballot);
            }
        }
    }

    fn take_all(
        &mut self,
    ) -> (
        Vec<PairwiseBallot<'a>>,
        Vec<SetwiseBallot<'a>>,
        Vec<GroupwiseBallot<'a>>,
        Vec<PluralityBallot<'a>>,
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

    fn need_process(&self) -> bool {
        let limit = std::cmp::min(self.capacity, 150);
        self.pairwise.len() >= limit
            || self.setwise.len() >= limit
            || self.groupwise.len() >= limit
            || self.plurality.len() >= limit
    }
}

pub struct BallotProcessor {
    sender: Sender<Ballot<'static>>,
}

impl BallotProcessor {
    pub async fn new(database: Arc<AppDatabase>, app_config: Arc<AppConfig>) -> Self {
        let (sender, receiver) = unbounded::<Ballot>();

        let mut conn = database
            .redis
            .client
            .get_multiplexed_async_connection()
            .await
            .expect("failed to create Redis connection");

        thread::spawn(move || {
            Self::batch_processor_thread(receiver, &mut conn, &database, &app_config);
        });

        Self { sender }
    }

    #[allow(clippy::result_large_err)]
    pub fn submit_ballot(
        &'_ self,
        ballot: Ballot<'static>,
    ) -> Result<(), crossbeam_channel::SendError<Ballot<'_>>> {
        self.sender.send(ballot)
    }

    fn batch_processor_thread(
        receiver: Receiver<Ballot<'static>>,
        conn: &mut redis::aio::MultiplexedConnection,
        database: &AppDatabase,
        app_config: &AppConfig,
    ) {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        rt.block_on(async {
            let mut ballot_groups = BallotMessageGroup::with_capacity(500);
            let mut last_flush = std::time::Instant::now();
            const FLUSH_INTERVAL: Duration = Duration::from_millis(500); // 500ms 超时

            loop {
                let mut count = 0;
                match receiver.recv_timeout(FLUSH_INTERVAL) {
                    Ok(ballot) => {
                        ballot_groups.add(ballot);

                        if ballot_groups.need_process() {
                            let (pairwise, _, _, _) = ballot_groups.take_all();

                            match Self::process_pairwise_ballot_batch(
                                &pairwise, conn, database, app_config,
                            )
                            .await
                            {
                                Ok(_) => {
                                    last_flush = std::time::Instant::now();
                                    count += pairwise.len();
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to process pairwise ballot batch: {}",
                                        e
                                    );
                                    save_ballot_to_disk(&pairwise);
                                }
                            }
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        if !ballot_groups.is_empty() && last_flush.elapsed() >= FLUSH_INTERVAL {
                            let (pairwise, _, _, _) = ballot_groups.take_all();
                            match Self::process_pairwise_ballot_batch(
                                &pairwise, conn, database, app_config,
                            )
                            .await
                            {
                                Ok(_) => {
                                    last_flush = std::time::Instant::now();
                                    count += pairwise.len();
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to process pairwise ballot batch: {}",
                                        e
                                    );
                                    save_ballot_to_disk(&pairwise);
                                }
                            }
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                        if !ballot_groups.is_empty() {
                            let (pairwise, _, _, _) = ballot_groups.take_all();
                            match Self::process_pairwise_ballot_batch(
                                &pairwise, conn, database, app_config,
                            )
                            .await
                            {
                                Ok(_) => {}
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to process pairwise ballot batch: {}",
                                        e
                                    );
                                    save_ballot_to_disk(&pairwise);
                                }
                            }
                        }
                        tracing::info!("Ballot processor thread shutting down");
                        break;
                    }
                }

                if count > 0 {
                    tracing::info!("Processed {} ballots in this batch", count);
                }
            }
        });
    }

    async fn process_pairwise_ballot_batch(
        ballots: &[PairwiseBallot<'_>],
        conn: &mut redis::aio::MultiplexedConnection,
        database: &AppDatabase,
        app_config: &AppConfig,
    ) -> Result<(), AppError> {
        if ballots.is_empty() {
            return Ok(());
        }

        let vote_config = &app_config.vote;
        let mut score_updates: Vec<((String, i32, i32), i32)> = Vec::with_capacity(ballots.len()); // ((topic_id, win_id, lose_id), total_multiplier)

        // 第一步：批量计算IP倍数
        let ip_multipliers = calculate_pairwise_multipliers(
            ballots,
            vote_config,
            &database.redis.batch_ip_counter_script,
            conn,
        )
        .await?;

        for item in ballots.iter() {
            let multiplier = ip_multipliers
                .get(item.info.ip.as_ref())
                .copied()
                .unwrap_or(vote_config.low_multiplier);

            score_updates.push((
                (item.info.topic_id.to_string(), item.win, item.lose),
                multiplier,
            ));
        }

        // 第二步：批量执行分数更新
        batch_update_scores(
            score_updates,
            &database.redis.batch_score_update_script,
            conn,
        )
        .await?;

        // 第三步：批量插入MongoDB
        // 先按照topic_id分组
        let mut grouped_ballots: HashMap<String, Vec<StoredBallot>> = HashMap::new();

        for item in ballots.iter() {
            let topic_id = item.info.topic_id.to_string();
            let multiplier = ip_multipliers
                .get(item.info.ip.as_ref())
                .copied()
                .unwrap_or(vote_config.low_multiplier);

            let stored_ballot = StoredBallot {
                ballot: Ballot::Pairwise(item.clone()),
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

        Ok(())
    }
}

async fn calculate_pairwise_multipliers(
    ballots: &[PairwiseBallot<'_>],
    vote_config: &VoteConfig,
    batch_ip_counter_script: &redis::Script,
    conn: &mut redis::aio::MultiplexedConnection,
) -> Result<HashMap<String, i32>, AppError> {
    if ballots.is_empty() {
        return Ok(HashMap::new());
    }

    let mut results = HashMap::new();

    let ips: HashSet<&str> = ballots.iter().map(|b| b.info.ip.as_ref()).collect();

    let keys: Vec<String> = ballots
        .iter()
        .map(|b| {
            let info = &b.info;
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
    updates: Vec<((String, i32, i32), i32)>, // ((topic_id, win_id, lose_id), total_multiplier)
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

fn save_ballot_to_disk(ballot: &[PairwiseBallot<'_>]) {
    let target_file = "./ballots.log".to_string();

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target_file)
        .unwrap();

    for item in ballot.iter() {
        let data = serde_json::to_string(item).unwrap();
        let line = format!("{}\n", data);
        file.write_all(line.as_bytes()).unwrap();
    }
}

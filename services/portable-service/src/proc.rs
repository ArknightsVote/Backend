use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::Write,
    sync::Arc,
    thread,
    time::Duration,
};

use once_cell::sync::Lazy;
use prometheus::{
    Histogram, HistogramOpts, IntCounter, IntCounterVec, opts,
    register_int_counter_vec_with_registry,
};
use share::{
    config::{AppConfig, VoteConfig},
    models::database::{
        Ballot, GroupwiseBallot, PairwiseBallot, PluralityBallot, SetwiseBallot, StoredBallot,
    },
};

use crate::{error::AppError, registry, state::AppDatabase};
use prometheus::{IntGaugeVec, register_int_gauge_vec_with_registry};

const BUCKET_START: f64 = 0.002;
const BUCKET_FACTOR: f64 = 2.0;
const BUCKET_COUNT: usize = 10;

enum ProcessingStatsEnum {
    TotalProcessed,
    SuccessfulBatches,
    FailedBatches,
}

impl ProcessingStatsEnum {
    const LABEL: &'static str = "processing_stats";

    #[inline]
    pub fn label(self) -> &'static str {
        match self {
            Self::TotalProcessed => "total_processed",
            Self::SuccessfulBatches => "successful_batches",
            Self::FailedBatches => "failed_batches",
        }
    }
}

fn processing_stats_total(label: ProcessingStatsEnum) -> IntCounter {
    static METRIC: Lazy<IntCounterVec> = Lazy::new(|| {
        register_int_counter_vec_with_registry!(
            opts!(
                "processing_stats_total",
                "ProcessingStats counters for total_processed, successful_batches, failed_batches",
            ),
            &[ProcessingStatsEnum::LABEL], // type: "total_processed", "successful_batches", "failed_batches"
            registry()
        )
        .unwrap()
    });

    METRIC.with_label_values(&[label.label()])
}

fn set_pending_processing_stats(p: usize, s: usize, g: usize, pl: usize) {
    static METRIC: Lazy<IntGaugeVec> = Lazy::new(|| {
        register_int_gauge_vec_with_registry!(
            opts!(
                "processing_stats_pending",
                "ProcessingStats gauges for pending pairwise, setwise, groupwise, plurality ballots",
            ),
            &[ProcessingStatsEnum::LABEL],
            registry()
        )
        .unwrap()
    });

    METRIC.with_label_values(&["pairwise"]).set(p as i64);
    METRIC.with_label_values(&["setwise"]).set(s as i64);
    METRIC.with_label_values(&["groupwise"]).set(g as i64);
    METRIC.with_label_values(&["plurality"]).set(pl as i64);
}

fn batch_process_time() -> &'static Histogram {
    static PROCESSING_TIME: Lazy<Histogram> = Lazy::new(|| {
        let opts = HistogramOpts::new(
            "processing_stats_batch_process_time",
            "Processing time for batch processing",
        )
        .buckets(
            prometheus::exponential_buckets(BUCKET_START, BUCKET_FACTOR, BUCKET_COUNT).unwrap(),
        );

        let hist = Histogram::with_opts(opts).unwrap();
        registry().register(Box::new(hist.clone())).unwrap();
        hist
    });

    &PROCESSING_TIME
}

fn batch_total_process_time() -> &'static IntCounter {
    static PROCESSING_TIME: Lazy<IntCounter> = Lazy::new(|| {
        let counter = IntCounter::new(
            "processing_stats_batch_total_process_time",
            "Processing time for all batch processing in microseconds",
        )
        .unwrap();

        registry().register(Box::new(counter.clone())).unwrap();
        counter
    });

    &PROCESSING_TIME
}

fn inc_batch_total_process_time(duration: Duration) {
    batch_total_process_time().inc_by(duration.as_micros() as u64);
}

fn inc_total_processed(count: usize) {
    processing_stats_total(ProcessingStatsEnum::TotalProcessed).inc_by(count as u64);
}

fn inc_successful_batches() {
    processing_stats_total(ProcessingStatsEnum::SuccessfulBatches).inc();
}

fn inc_failed_batches() {
    processing_stats_total(ProcessingStatsEnum::FailedBatches).inc();
}

struct BallotMessageGroup<'a> {
    pairwise: VecDeque<PairwiseBallot<'a>>,
    setwise: VecDeque<SetwiseBallot<'a>>,
    groupwise: VecDeque<GroupwiseBallot<'a>>,
    plurality: VecDeque<PluralityBallot<'a>>,

    capacity: usize,
    total_count: usize,
}

impl<'a> BallotMessageGroup<'a> {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            pairwise: VecDeque::with_capacity(capacity),
            setwise: VecDeque::with_capacity(capacity),
            groupwise: VecDeque::with_capacity(capacity),
            plurality: VecDeque::with_capacity(capacity),

            capacity,
            total_count: 0,
        }
    }

    fn add(&mut self, message: Ballot<'a>) {
        match message {
            Ballot::Pairwise(ballot) => self.pairwise.push_back(ballot),
            Ballot::Setwise(ballot) => self.setwise.push_back(ballot),
            Ballot::Groupwise(ballot) => self.groupwise.push_back(ballot),
            Ballot::Plurality(ballot) => self.plurality.push_back(ballot),
        }
        self.total_count += 1;
    }

    fn take_all(
        &mut self,
    ) -> (
        VecDeque<PairwiseBallot<'a>>,
        VecDeque<SetwiseBallot<'a>>,
        VecDeque<GroupwiseBallot<'a>>,
        VecDeque<PluralityBallot<'a>>,
    ) {
        (
            std::mem::take(&mut self.pairwise),
            std::mem::take(&mut self.setwise),
            std::mem::take(&mut self.groupwise),
            std::mem::take(&mut self.plurality),
        )
    }

    fn is_empty(&self) -> bool {
        self.total_count == 0
    }

    fn need_process(&self) -> bool {
        let limit = std::cmp::min(self.capacity, 150);
        self.pairwise.len() >= limit
            || self.setwise.len() >= limit
            || self.groupwise.len() >= limit
            || self.plurality.len() >= limit
    }

    fn get_counts(&self) -> (usize, usize, usize, usize) {
        (
            self.pairwise.len(),
            self.setwise.len(),
            self.groupwise.len(),
            self.plurality.len(),
        )
    }
}

#[derive(Debug)]
struct ProcessingStats {
    total_processed: usize,
    successful_batches: usize,
    failed_batches: usize,
    last_flush_time: std::time::Instant,
    total_batch_time: std::time::Duration,
}

impl Default for ProcessingStats {
    fn default() -> Self {
        Self {
            total_processed: 0,
            successful_batches: 0,
            failed_batches: 0,
            last_flush_time: std::time::Instant::now(),
            total_batch_time: std::time::Duration::ZERO,
        }
    }
}

pub struct BallotProcessor {
    sender: tokio::sync::mpsc::UnboundedSender<Ballot<'static>>,
}

impl BallotProcessor {
    pub async fn new(database: AppDatabase, app_config: Arc<AppConfig>) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<Ballot<'static>>();

        let conn = database
            .redis
            .client
            .get_multiplexed_async_connection()
            .await
            .expect("failed to create Redis connection");

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async move {
                Self::batch_processor_task(receiver, conn, &database, &app_config).await;
            });
        });

        Self { sender }
    }

    pub fn submit_ballot(&'_ self, ballot: Ballot<'static>) -> eyre::Result<()> {
        Ok(self.sender.send(ballot)?)
    }

    async fn batch_processor_task(
        mut receiver: tokio::sync::mpsc::UnboundedReceiver<Ballot<'static>>,
        mut conn: redis::aio::MultiplexedConnection,
        database: &AppDatabase,
        config: &AppConfig,
    ) {
        let mut ballot_groups = BallotMessageGroup::with_capacity(1000);
        let mut stats = ProcessingStats::default();

        let flush_interval = Duration::from_millis(500);
        let stats_log_interval = Duration::from_secs(5);
        let max_wait = Duration::from_secs(5);

        let mut last_total_processed = 0;
        let mut last_log = std::time::Instant::now();
        let mut last_flush = std::time::Instant::now();

        let mut ticker = tokio::time::interval(flush_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                maybe_ballot = receiver.recv() => {
                    match maybe_ballot {
                        Some(ballot) => {
                            ballot_groups.add(ballot);
                            if ballot_groups.need_process() || last_flush.elapsed() >= max_wait {
                                Self::process_batch_with_retry(
                                    &mut ballot_groups,
                                    &mut conn,
                                    database,
                                    config,
                                    &mut stats,
                                ).await;
                                last_flush = std::time::Instant::now();
                            }
                        }
                        None => {
                            // graceful shutdown
                            if !ballot_groups.is_empty() {
                                Self::process_batch_with_retry(
                                    &mut ballot_groups,
                                    &mut conn,
                                    database,
                                    config,
                                    &mut stats,
                                ).await;
                            }
                            tracing::info!("Ballot processor shutting down");
                            break;
                        }
                    }
                }
                _ = ticker.tick() => {
                    if !ballot_groups.is_empty() && last_flush.elapsed() >= flush_interval {
                        Self::process_batch_with_retry(
                            &mut ballot_groups,
                            &mut conn,
                            database,
                            config,
                            &mut stats,
                        ).await;
                        last_flush = std::time::Instant::now();
                    }
                }
            }

            let (p, s, g, pl) = ballot_groups.get_counts();
            set_pending_processing_stats(p, s, g, pl);

            if stats.total_processed > 0
                && stats.total_processed != last_total_processed
                && last_log.elapsed() >= stats_log_interval
            {
                let avg_batch_time = if stats.successful_batches > 0 {
                    stats.total_batch_time / stats.successful_batches as u32
                } else {
                    std::time::Duration::ZERO
                };

                tracing::info!(
                    "Processing stats: total={}, successful_batches={}, failed_batches={}, pending=[p:{}, s:{}, g:{}, pl:{}], avg_batch_time={:?}",
                    stats.total_processed,
                    stats.successful_batches,
                    stats.failed_batches,
                    p,
                    s,
                    g,
                    pl,
                    avg_batch_time
                );
                last_total_processed = stats.total_processed;
                last_log = std::time::Instant::now();
            }
        }
    }

    async fn process_batch_with_retry(
        ballot_groups: &mut BallotMessageGroup<'_>,
        conn: &mut redis::aio::MultiplexedConnection,
        database: &AppDatabase,
        app_config: &AppConfig,
        stats: &mut ProcessingStats,
    ) {
        let (mut pairwise, mut setwise, mut groupwise, mut plurality) = ballot_groups.take_all();
        let total_count = pairwise.len() + setwise.len() + groupwise.len() + plurality.len();

        if total_count == 0 {
            return;
        }

        let timer = batch_process_time().start_timer();
        let start_time = tokio::time::Instant::now();
        for attempt in 1..=3 {
            match Self::process_all_ballot_types(
                pairwise.make_contiguous(),
                setwise.make_contiguous(),
                groupwise.make_contiguous(),
                plurality.make_contiguous(),
                conn,
                database,
                app_config,
            )
            .await
            {
                Ok(_) => {
                    let duration_secs = timer.stop_and_record();
                    stats.total_batch_time += start_time.elapsed();
                    stats.total_processed += total_count;
                    stats.successful_batches += 1;
                    stats.last_flush_time = std::time::Instant::now();
                    inc_total_processed(total_count);
                    inc_successful_batches();
                    inc_batch_total_process_time(Duration::from_secs_f64(duration_secs));

                    tracing::debug!(
                        "Successfully processed {} ballots in batch, duration={:?}",
                        total_count,
                        start_time.elapsed()
                    );
                    return;
                }
                Err(e) => {
                    tracing::warn!(
                        "Attempt {}/{} failed to process batch of {} ballots: {}",
                        attempt,
                        3,
                        total_count,
                        e
                    );

                    if attempt == 3 {
                        stats.failed_batches += 1;
                        inc_failed_batches();
                        save_failed_ballots(
                            pairwise.make_contiguous(),
                            setwise.make_contiguous(),
                            groupwise.make_contiguous(),
                            plurality.make_contiguous(),
                        );
                    } else {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        }
    }

    async fn process_all_ballot_types(
        pairwise: &[PairwiseBallot<'_>],
        _setwise: &[SetwiseBallot<'_>],
        _groupwise: &[GroupwiseBallot<'_>],
        _plurality: &[PluralityBallot<'_>],
        conn: &mut redis::aio::MultiplexedConnection,
        database: &AppDatabase,
        app_config: &AppConfig,
    ) -> Result<(), AppError> {
        Self::process_pairwise_ballot_batch(pairwise, conn, database, app_config).await?;
        // Self::process_setwise_ballot_batch(setwise, conn, database, app_config).await?;
        // Self::process_groupwise_ballot_batch(groupwise, conn, database, app_config).await?;
        // Self::process_plurality_ballot_batch(plurality, conn, database, app_config).await?;

        Ok(())
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

        // 第一步：批量计算IP倍数
        let start_time = tokio::time::Instant::now();
        let ip_multipliers = calculate_pairwise_multipliers(
            ballots,
            vote_config,
            &database.redis.batch_ip_counter_script,
            conn,
        )
        .await?;
        tracing::debug!(
            "Calculated IP multipliers for {} ballots, duration={:?}",
            ballots.len(),
            start_time.elapsed()
        );

        let start_time = tokio::time::Instant::now();
        let (score_updates, grouped_ballots) = {
            let ballots = ballots.to_vec();
            let ip_multipliers = ip_multipliers.clone();
            let low_multiplier = vote_config.low_multiplier;

            let mut score_updates = Vec::with_capacity(ballots.len()); // ((topic_id, win_id, lose_id), total_multiplier)
            let mut grouped_ballots: HashMap<String, Vec<StoredBallot>> = HashMap::new();

            for item in ballots.iter() {
                let multiplier = ip_multipliers
                    .get(item.info.ip.as_ref())
                    .copied()
                    .unwrap_or(low_multiplier);

                score_updates.push((
                    (item.info.topic_id.to_string(), item.win, item.lose),
                    multiplier,
                ));

                let stored_ballot = StoredBallot {
                    ballot: Ballot::Pairwise(item.clone()),
                    multiplier,
                };

                grouped_ballots
                    .entry(item.info.topic_id.to_string())
                    .or_default()
                    .push(stored_ballot);
            }

            (score_updates, grouped_ballots)
        };
        tracing::debug!(
            "Processed pairwise ballot batch, duration={:?}, score_updates.len={}, grouped_ballots.len={}",
            start_time.elapsed(),
            score_updates.len(),
            grouped_ballots.len()
        );

        // 第二步：批量执行分数更新
        let start_time = tokio::time::Instant::now();
        batch_update_scores(
            score_updates,
            &database.redis.batch_score_update_script,
            &database.redis.batch_record_1v1_script,
            conn,
        )
        .await?;
        tracing::debug!(
            "Batch score updates completed, duration={:?}",
            start_time.elapsed()
        );

        let start_time = tokio::time::Instant::now();
        for (topic_id, ballots) in grouped_ballots.iter() {
            let ballot_collection = database
                .mongo_database
                .collection::<StoredBallot>(&format!("ballots_{}", topic_id));

            ballot_collection.insert_many(ballots).await?;
        }
        tracing::debug!(
            "Inserted {} ballots into MongoDB, duration={:?}",
            grouped_ballots.values().map(|v| v.len()).sum::<usize>(),
            start_time.elapsed()
        );

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
    batch_record_1v1_script: &redis::Script,
    conn: &mut redis::aio::MultiplexedConnection,
) -> Result<(), AppError> {
    if updates.is_empty() {
        return Ok(());
    }

    // 准备参数：topic_id1, win_id1, lose_id1, multiplier1, topic_id2, win_id2, lose_id2, multiplier2, ...
    let mut args = Vec::with_capacity(updates.len() * 4);
    let mut args2 = Vec::with_capacity(updates.len() * 3);
    for ((topic_id, win_id, lose_id), multiplier) in updates {
        args.push(topic_id.clone());
        args.push(win_id.to_string());
        args.push(lose_id.to_string());
        args.push(multiplier.to_string());

        args2.push(topic_id.clone());
        let (min_id, max_id) = if win_id < lose_id {
            (win_id, lose_id)
        } else {
            (lose_id, win_id)
        };
        args2.push(min_id.to_string());
        args2.push(max_id.to_string());
    }

    // 执行批量分数更新脚本
    let _results: () = batch_score_update_script
        .arg(&args)
        .invoke_async(conn)
        .await?;

    let _results: () = batch_record_1v1_script
        .arg(&args2)
        .invoke_async(conn)
        .await?;

    Ok(())
}

fn save_failed_ballots(
    pairwise: &[PairwiseBallot<'_>],
    setwise: &[SetwiseBallot<'_>],
    groupwise: &[GroupwiseBallot<'_>],
    plurality: &[PluralityBallot<'_>],
) {
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");

    if !pairwise.is_empty() {
        save_ballots_to_file(
            pairwise,
            &format!("./failed_pairwise_ballots_{}.log", timestamp),
        );
    }
    if !setwise.is_empty() {
        save_ballots_to_file(
            setwise,
            &format!("./failed_setwise_ballots_{}.log", timestamp),
        );
    }
    if !groupwise.is_empty() {
        save_ballots_to_file(
            groupwise,
            &format!("./failed_groupwise_ballots_{}.log", timestamp),
        );
    }
    if !plurality.is_empty() {
        save_ballots_to_file(
            plurality,
            &format!("./failed_plurality_ballots_{}.log", timestamp),
        );
    }
}

fn save_ballots_to_file<T: serde::Serialize>(ballots: &[T], filename: &str) {
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(filename)
    {
        Ok(mut file) => {
            for ballot in ballots {
                if let Ok(data) = serde_json::to_string(ballot)
                    && let Err(e) = writeln!(file, "{}", data)
                {
                    tracing::error!("Failed to write to {}: {}", filename, e);
                    break;
                }
            }
            tracing::info!("Saved {} failed ballots to {}", ballots.len(), filename);
        }
        Err(e) => {
            tracing::error!("Failed to create backup file {}: {}", filename, e);
        }
    }
}

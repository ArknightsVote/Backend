use eyre::{Context, ContextCompat as _, Result};
use futures::{StreamExt, stream::FuturesUnordered};
use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use hdrhistogram::Histogram;
use reqwest::Client;
use share::config::AppConfig;
use share::models::api::{
    ApiData, ApiResponse, BallotCreateRequest, BallotCreateResponse, BallotSaveRequest,
    PairwiseSaveScore, Results1v1MatrixRequest, Results1v1MatrixResponse, ResultsFinalOrderRequest,
    ResultsFinalOrderResponse,
};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Instant;
use tokio::sync::{Semaphore, mpsc};

#[derive(Debug)]
enum StatEvent {
    Success {
        win: i32,
        lose: i32,
        latency_us: u64,
    },
}

#[derive(Clone)]
pub struct ServiceTester {
    base_url: String,
    total_requests: usize,
    concurrency_limit: usize,
    max_retry: usize,
    qps_limit: u32,
}

impl ServiceTester {
    pub fn new(config: AppConfig) -> Self {
        let test_config = &config.test;
        Self {
            base_url: test_config.base_url.clone(),
            total_requests: test_config.total_requests,
            concurrency_limit: test_config.concurrency_limit,
            max_retry: test_config.max_retry,
            qps_limit: test_config.qps_limit,
        }
    }

    pub async fn run(self) -> Result<()> {
        tracing::info!("checking endpoint availability...");
        self.check_endpoints_available().await?;
        tracing::info!("all endpoints are available.");

        let client = Client::new();
        let limiter = if self.qps_limit > 0 {
            Some(Arc::new(RateLimiter::direct(Quota::per_second(
                NonZeroU32::new(self.qps_limit).unwrap(),
            ))))
        } else {
            None
        };

        let data = ResultsFinalOrderRequest {
            topic_id: "crisis_v2_season_4_1_benchtest".to_string(),
        };
        let init_data = self.results_final_order(&client, &data).await?;
        let init_score: i64 = init_data.items.iter().map(|i| i.win + i.lose).sum();

        let semaphore = Arc::new(Semaphore::new(self.concurrency_limit));
        let success_count = Arc::new(AtomicUsize::new(0));
        let (tx, mut rx) = mpsc::channel::<StatEvent>(self.total_requests);
        let histogram = Arc::new(tokio::sync::Mutex::new(Histogram::<u64>::new(3)?));
        let mut result_map: HashMap<i32, (i64, i64)> = HashMap::new();

        // Spawn stats collector
        let hist_clone = Arc::clone(&histogram);
        let success_clone = Arc::clone(&success_count);
        let stats_handle = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let StatEvent::Success {
                    win,
                    lose,
                    latency_us,
                } = event;

                success_clone.fetch_add(1, Ordering::Relaxed);
                result_map.entry(win).or_default().0 += 1;
                result_map.entry(lose).or_default().1 += 1;
                let mut h = hist_clone.lock().await;
                let _ = h.record(latency_us);
            }
            result_map
        });

        tracing::info!(
            "starting load test with {} requests...",
            self.total_requests
        );
        let mut futures = FuturesUnordered::new();

        for _ in 0..self.total_requests {
            let permit = semaphore.clone().acquire_owned().await?;
            let this = self.clone();
            let client = client.clone();
            let tx = tx.clone();
            let limiter = limiter.clone();

            futures.push(tokio::spawn(async move {
                let _permit = permit;
                if let Err(e) = this.single_test_request(client, tx, limiter.as_ref()).await {
                    tracing::error!("request failed: {e}");
                }
            }));
        }

        while futures.next().await.is_some() {}
        drop(tx);
        let result_map = stats_handle.await?;

        tracing::info!("waiting 5 seconds for final data to stabilize...");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let success = success_count.load(Ordering::Relaxed);
        tracing::info!(
            total = self.total_requests,
            success,
            percent = (success as f64 / self.total_requests as f64 * 100.0),
            "requests completed"
        );

        let hist = histogram.lock().await;
        if !hist.is_empty() {
            tracing::info!(
                p50 = hist.value_at_quantile(0.50),
                p75 = hist.value_at_quantile(0.75),
                p90 = hist.value_at_quantile(0.90),
                p95 = hist.value_at_quantile(0.95),
                p99 = hist.value_at_quantile(0.99),
                p99_9 = hist.value_at_quantile(0.999),
                min = hist.min(),
                max = hist.max(),
                mean = hist.mean(),
                stddev = hist.stdev(),
                "latency statistics (μs)"
            );

            let under_1ms = hist.count_between(0, 1_000);
            let under_5ms = hist.count_between(1_001, 5_000);
            let under_10ms = hist.count_between(1_001, 10_000);
            let over_10ms = hist.count_between(10_001, u64::MAX);

            tracing::info!(
                under_1ms,
                under_5ms,
                under_10ms,
                over_10ms,
                "latency distribution count (μs)"
            );
        }

        let final_data = self
            .results_final_order(
                &client,
                &ResultsFinalOrderRequest {
                    topic_id: "crisis_v2_season_4_1_benchtest".to_string(),
                },
            )
            .await?;
        let final_score: i64 = final_data.items.iter().map(|i| i.win + i.lose).sum();

        assert_eq!(
            final_data.count,
            init_data.count + self.total_requests as i64
        );
        assert_eq!(final_score, init_score + self.total_requests as i64 * 2);

        for item in final_data.items.iter() {
            let (expected_win, expected_lose) = result_map
                .get(&item.id)
                .with_context(|| format!("operator ID {} not found in results", item.id))?;
            let init = init_data
                .items
                .iter()
                .find(|x| x.id == item.id)
                .with_context(|| format!("operator ID {} not found", item.id))?;

            assert_eq!(item.name, init.name);
            assert_eq!(item.win - init.win, *expected_win);
            assert_eq!(item.lose - init.lose, *expected_lose);
        }

        tracing::info!("combined results passed validation!");
        tracing::info!("validation all passed!");

        Ok(())
    }

    async fn single_test_request(
        &self,
        client: Client,
        tx: mpsc::Sender<StatEvent>,
        limiter: Option<&Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>>,
    ) -> Result<()> {
        if let Some(limiter) = &limiter {
            limiter.until_ready().await
        }

        let mut attempt = 0;

        loop {
            attempt += 1;
            let start = Instant::now();

            let data = BallotCreateRequest {
                topic_id: "crisis_v2_season_4_1_benchtest".to_string(),
            };
            let compare = match self.ballot_create(&client, &data).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("new compare failed: {e}");
                    continue;
                }
            };

            let (left, right, ballot_id) = match compare {
                BallotCreateResponse::Pairwise {
                    left,
                    right,
                    ballot_id,
                    ..
                } => (left, right, ballot_id),
                _ => {
                    tracing::warn!("unexpected compare response type");
                    continue;
                }
            };
            assert!(left != right, "left and right should not be the same");
            assert!(left > 0 && right > 0, "left and right should be positive");

            let data = BallotSaveRequest::Pairwise(PairwiseSaveScore {
                topic_id: "crisis_v2_season_4_1_benchtest".to_string(),
                ballot_id,
                winner: left,
                loser: right,
            });

            match self.ballot_save(&client, &data).await {
                Ok(_) => {
                    let latency = start.elapsed().as_micros() as u64;
                    let _ = tx
                        .send(StatEvent::Success {
                            win: left,
                            lose: right,
                            latency_us: latency,
                        })
                        .await;
                    return Ok(());
                }
                Err(e) if attempt < self.max_retry => {
                    tracing::warn!("save score failed (retry {attempt}): {e}");
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn check_endpoints_available(&self) -> Result<()> {
        let client = Client::new();

        let data = BallotCreateRequest {
            topic_id: "crisis_v2_season_4_1_benchtest".to_string(),
        };
        let data = self.ballot_create(&client, &data).await?;
        let (left, right, ballot_id) = match data {
            BallotCreateResponse::Pairwise {
                left,
                right,
                ballot_id,
                ..
            } => (left, right, ballot_id),
            _ => {
                tracing::warn!("unexpected compare response type");
                return Err(eyre::eyre!("unexpected compare response type"));
            }
        };

        let data = BallotSaveRequest::Pairwise(PairwiseSaveScore {
            topic_id: "crisis_v2_season_4_1_benchtest".to_string(),
            ballot_id,
            winner: left,
            loser: right,
        });
        self.ballot_save(&client, &data).await?;

        self.results_final_order(
            &client,
            &ResultsFinalOrderRequest {
                topic_id: "crisis_v2_season_4_1_benchtest".to_string(),
            },
        )
        .await?;
        self.results_1v1_matrix(
            &client,
            &Results1v1MatrixRequest {
                topic_id: "crisis_v2_season_4_1_benchtest".to_string(),
            },
        )
        .await?;

        Ok(())
    }

    async fn results_final_order(
        &self,
        client: &Client,
        data: &ResultsFinalOrderRequest,
    ) -> Result<ResultsFinalOrderResponse> {
        let res = client
            .post(format!("{}/results/final_order", self.base_url))
            .json(data)
            .send()
            .await
            .context("get results_final_order failed")?;

        let response = res
            .json::<ApiResponse<ResultsFinalOrderResponse>>()
            .await
            .context("parsing results_final_order failed")?;

        match response.data {
            ApiData::Data(data) => Ok(data),
            ApiData::Empty => Err(eyre::eyre!("results_final_order response data is missing")),
        }
    }

    async fn results_1v1_matrix(
        &self,
        client: &Client,
        data: &Results1v1MatrixRequest,
    ) -> Result<Results1v1MatrixResponse> {
        let res = client
            .post(format!("{}/results/1v1_matrix", self.base_url))
            .json(data)
            .send()
            .await
            .context("get results_1v1_matrix failed")?;

        let response = res
            .json::<ApiResponse<Results1v1MatrixResponse>>()
            .await
            .context("parsing results_1v1_matrix failed")?;

        match response.data {
            ApiData::Data(data) => Ok(data),
            ApiData::Empty => Err(eyre::eyre!("results_1v1_matrix response data is missing")),
        }
    }

    async fn ballot_create(
        &self,
        client: &Client,
        data: &BallotCreateRequest,
    ) -> Result<BallotCreateResponse> {
        let res = client
            .post(format!("{}/ballot/new", self.base_url))
            .json(data)
            .send()
            .await
            .context("post ballot_create failed")?;

        let response = res
            .json::<ApiResponse<BallotCreateResponse>>()
            .await
            .context("parsing ballot_create response failed")?;

        match response.data {
            ApiData::Data(data) => Ok(data),
            ApiData::Empty => Err(eyre::eyre!("ballot_create response data is missing")),
        }
    }

    async fn ballot_save(&self, client: &Client, data: &BallotSaveRequest) -> Result<()> {
        let res = client
            .post(format!("{}/ballot/save", self.base_url))
            .json(&data)
            .send()
            .await
            .context("post ballot_save failed")?;

        if res.status().is_success() {
            Ok(())
        } else {
            Err(eyre::eyre!("ballot_save failed: status {}", res.status()))
        }
    }
}

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::time::{Duration, interval};

use crate::{api::OperatorsInfo, topic::TopicService};

#[derive(Debug, Deserialize, Serialize)]
pub struct OperatorStatistics {
    pub ts: mongodb::bson::DateTime,
    pub operator_id: i32,

    pub win: i64,
    pub lose: i64,
    pub rate: f64,
}

impl OperatorStatistics {
    fn new(operator_id: i32, win: i64, lose: i64, ts: mongodb::bson::DateTime) -> Self {
        let total = win + lose;
        let rate = match total {
            t if t > 0 => win as f64 * 100.0 / t as f64,
            _ => 0.0,
        };

        Self {
            ts,
            win,
            lose,
            operator_id,
            rate,
        }
    }
}

pub async fn update_operator_statistics(
    topic_service: Arc<TopicService>,
    db: mongodb::Database,
    connection: redis::aio::MultiplexedConnection,
    final_order_script: redis::Script,
    operators_info: OperatorsInfo,
) -> eyre::Result<()> {
    const TARGET_TOPIC: &str = "crisis_v2_season_4_1";
    const COLLECTION_NAME: &str = "operator_rates";
    const TICK_INTERVAL_SECS: u64 = 1;

    initialize_timeseries_collection(&db, COLLECTION_NAME).await?;

    let target_collection = db.collection::<OperatorStatistics>(COLLECTION_NAME);
    let mut ticker = interval(Duration::from_secs(TICK_INTERVAL_SECS));

    let num_operators = operators_info.num_operators;
    let operator_ids = Arc::new(operators_info.operator_ids);
    let op_stats_all_fields = Arc::new(operators_info.op_stats_all_fields);
    let final_order_script = Arc::new(final_order_script);

    tracing::info!(
        "Starting operator statistics update loop for topic: {}",
        TARGET_TOPIC
    );

    loop {
        ticker.tick().await;

        if !topic_service
            .is_topic_active(TARGET_TOPIC)
            .await
            .unwrap_or(false)
        {
            continue;
        }

        let connection = connection.clone();
        let script = Arc::clone(&final_order_script);
        let operator_ids = Arc::clone(&operator_ids);
        let op_stats_fields = Arc::clone(&op_stats_all_fields);
        let collection = target_collection.clone();

        tokio::spawn(async move {
            if let Err(e) = update_single_batch(
                connection,
                &script,
                TARGET_TOPIC,
                &op_stats_fields,
                &operator_ids,
                num_operators,
                collection,
            )
            .await
            {
                tracing::error!("Failed to update operator statistics batch: {}", e);
            }
        });
    }
}

async fn initialize_timeseries_collection(
    db: &mongodb::Database,
    collection_name: &str,
) -> eyre::Result<()> {
    let ts_opts = mongodb::options::TimeseriesOptions::builder()
        .time_field("ts".to_string())
        .meta_field(Some("operator_id".to_string()))
        .granularity(Some(mongodb::options::TimeseriesGranularity::Seconds))
        .build();

    match db
        .create_collection(collection_name)
        .timeseries(ts_opts)
        .await
    {
        Ok(_) => {
            tracing::info!("Created timeseries collection: {}", collection_name);
        }
        Err(e) => {
            tracing::warn!("Collection creation result: {}", e);
        }
    }

    Ok(())
}

async fn update_single_batch(
    mut connection: redis::aio::MultiplexedConnection,
    script: &redis::Script,
    topic: &str,
    op_stats_fields: &[String],
    operator_ids: &[i32],
    num_operators: usize,
    collection: mongodb::Collection<OperatorStatistics>,
) -> eyre::Result<()> {
    let (operator_values, _): (Vec<Option<String>>, Option<i64>) = script
        .key(topic)
        .arg(op_stats_fields)
        .invoke_async(&mut connection)
        .await
        .map_err(|e| eyre::eyre!("Redis script execution failed: {}", e))?;

    let (win_counts, lose_counts) = parse_operator_counts(&operator_values, num_operators);

    let now = mongodb::bson::DateTime::now();
    let results = build_operator_results(operator_ids, &win_counts, &lose_counts, now);

    if !results.is_empty() {
        collection
            .insert_many(results)
            .await
            .map_err(|e| eyre::eyre!("Failed to insert operator rates: {}", e))?;
    }

    Ok(())
}

fn parse_operator_counts(values: &[Option<String>], num_operators: usize) -> (Vec<i64>, Vec<i64>) {
    let win_counts: Vec<i64> = values[..num_operators]
        .iter()
        .map(|v| v.as_ref().and_then(|s| s.parse().ok()).unwrap_or(0))
        .collect();

    let lose_counts: Vec<i64> = values[num_operators..]
        .iter()
        .map(|v| v.as_ref().and_then(|s| s.parse().ok()).unwrap_or(0))
        .collect();

    (win_counts, lose_counts)
}

fn build_operator_results(
    operator_ids: &[i32],
    win_counts: &[i64],
    lose_counts: &[i64],
    now: mongodb::bson::DateTime,
) -> Vec<OperatorStatistics> {
    operator_ids
        .iter()
        .enumerate()
        .map(|(i, &oid)| OperatorStatistics::new(oid, win_counts[i], lose_counts[i], now))
        .collect()
}

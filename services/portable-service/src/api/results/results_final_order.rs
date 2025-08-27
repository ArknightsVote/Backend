use std::{collections::HashMap, sync::Arc};

use actix_web::{Responder, post, web};
use ordered_float::OrderedFloat;
use share::models::{
    api::{
        ApiData, ApiMsg, ApiResponse, FinalOrderItem, ResultsFinalOrderRequest,
        ResultsFinalOrderResponse,
    },
    excel::CharacterInfo,
};

use crate::{AppState, state::ResultsType};

#[derive(Debug)]
struct OperatorResult {
    id: i32,
    win: i64,
    lose: i64,

    name: String,
    score: f64,
    rate: f64,
}

impl OperatorResult {
    fn new(name: String, id: i32, win: i64, lose: i64) -> Self {
        let total = win + lose;
        let rate = match total {
            t if t > 0 => win as f64 * 100.0 / t as f64,
            _ => 0.0,
        };
        let score = (win - lose) as f64 / 100.0;

        Self {
            name,
            id,
            win,
            lose,
            score,
            rate,
        }
    }
}

#[derive(Clone)]
struct OperatorsInfo {
    operator_ids: Vec<i32>,
    reverse_operators_id_dict: HashMap<i32, String>,
    num_operators: usize,
    op_stats_all_fields: Vec<String>,
}

fn generate_operators_info(
    operator_ids: &[i32],
    character_infos: &[CharacterInfo],
) -> OperatorsInfo {
    let num_operators = operator_ids.len();
    let reverse_operators_id_dict: HashMap<i32, String> = operator_ids
        .iter()
        .filter_map(|&id| {
            character_infos
                .iter()
                .find(|op| op.id == id)
                .map(|op| (id, op.name.clone()))
                .or_else(|| Some((id, format!("Unknown Operator {}", id))))
        })
        .collect();
    let win_fields: Vec<String> = operator_ids
        .iter()
        .map(|oid| format!("{oid}:win"))
        .collect();
    let lose_fields: Vec<String> = operator_ids
        .iter()
        .map(|oid| format!("{oid}:lose"))
        .collect();
    let op_stats_all_fields = [&win_fields[..], &lose_fields[..]].concat();

    OperatorsInfo {
        operator_ids: operator_ids.to_vec(),
        num_operators,
        reverse_operators_id_dict,
        op_stats_all_fields,
    }
}

#[post("/results/final_order")]
pub async fn results_final_order_fn(
    state: web::Data<AppState>,
    web::Json(req): web::Json<ResultsFinalOrderRequest>,
) -> actix_web::Result<impl Responder> {
    let target_topic = match state.topic_service.get_topic(&req.topic_id).await {
        Ok(Some(topic)) if topic.topic_type.supports_final_order() => topic,
        Ok(_) => {
            tracing::debug!("Topic {} does not support final order", req.topic_id);
            return Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::CurTopicNotSupportFinalOrder,
            }));
        }
        Err(_) => {
            tracing::debug!("Topic {} not found", req.topic_id);
            return Ok(web::Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };

    let cache_key = (target_topic.id, ResultsType::FinalOrder);
    if let Some(cached) = state.results_cache_store.get(&cache_key).await
        && let Some(final_order) = &cached.final_order
    {
        tracing::debug!("Cache hit for final order of topic {}", req.topic_id);
        return Ok(web::Json(ApiResponse {
            status: 0,
            data: ApiData::Data(final_order.clone()),
            message: ApiMsg::OK,
        }));
    }

    let candidate_pool = match state
        .topic_service
        .get_candidate_pool(&cache_key.0, &state.character_infos)
        .await
    {
        Some(pool) => pool,
        None => {
            return Ok(web::Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };
    let operators_info = generate_operators_info(&candidate_pool, &state.character_infos);
    let num_operators = operators_info.num_operators;

    let mut conn = state.database.redis.connection.clone();

    let (operator_values, total_valid_ballots): (Vec<Option<String>>, Option<i64>) = match state
        .database
        .redis
        .final_order_script
        .key(&req.topic_id)
        .arg(&operators_info.op_stats_all_fields)
        .invoke_async(&mut conn)
        .await
    {
        Ok(result) => result,
        Err(err) => {
            tracing::error!("Failed to execute Lua script for final order: {}", err);
            return Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::InternalError,
            }));
        }
    };

    let (win_counts, lose_counts) = parse_operator_counts(&operator_values, num_operators);

    let mut results = build_operator_results(
        &operators_info.operator_ids,
        &operators_info.reverse_operators_id_dict,
        &win_counts,
        &lose_counts,
    );

    results.sort_by(|a, b| {
        (OrderedFloat(b.rate), OrderedFloat(b.score), b.win, a.id).cmp(&(
            OrderedFloat(a.rate),
            OrderedFloat(a.score),
            a.win,
            b.id,
        ))
    });

    let response = Arc::new(ResultsFinalOrderResponse {
        topic_id: req.topic_id,
        items: results
            .into_iter()
            .map(|r| FinalOrderItem {
                name: r.name,
                id: r.id,
                win: r.win,
                lose: r.lose,
                score: format!("{:.2}", r.score),
                rate: format!("{:.1}%", r.rate),
            })
            .collect(),
        count: total_valid_ballots.unwrap_or(0),
    });

    let mut cached = state
        .results_cache_store
        .get(&cache_key)
        .await
        .unwrap_or_default();

    cached.final_order = Some(response.clone());
    state.results_cache_store.insert(cache_key, cached).await;

    Ok(web::Json(ApiResponse {
        status: 0,
        data: ApiData::Data(response),
        message: ApiMsg::OK,
    }))
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
    reverse_dict: &std::collections::HashMap<i32, String>,
    win_counts: &[i64],
    lose_counts: &[i64],
) -> Vec<OperatorResult> {
    operator_ids
        .iter()
        .enumerate()
        .map(|(i, &oid)| {
            let name = reverse_dict
                .get(&oid)
                .cloned()
                .unwrap_or_else(|| oid.to_string());

            OperatorResult::new(name, oid, win_counts[i], lose_counts[i])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_operator_result_new() {
        let result = OperatorResult::new("Test".to_string(), 1, 70, 30);
        assert_eq!(result.name, "Test");
        assert_eq!(result.score, 0.4);
        assert_eq!(result.rate, 70.0);

        let result_zero = OperatorResult::new("Zero".to_string(), 2, 0, 0);
        assert_eq!(result_zero.score, 0.0);
        assert_eq!(result_zero.rate, 0.0);
    }

    #[test]
    fn test_parse_operator_counts() {
        let values = vec![
            Some("10".to_string()),
            Some("20".to_string()),
            Some("5".to_string()),
            Some("15".to_string()),
        ];
        let (wins, losses) = parse_operator_counts(&values, 2);

        assert_eq!(wins, vec![10, 20]);
        assert_eq!(losses, vec![5, 15]);
    }

    #[test]
    fn test_build_operator_results() {
        let operator_ids = vec![101, 102];
        let mut reverse_dict = HashMap::new();
        reverse_dict.insert(101, "Amiya".to_string());
        reverse_dict.insert(102, "SilverAsh".to_string());

        let win_counts = vec![20, 10];
        let lose_counts = vec![5, 15];

        let results =
            build_operator_results(&operator_ids, &reverse_dict, &win_counts, &lose_counts);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "Amiya");
        assert_eq!(results[0].rate, 80.0);
        assert_eq!(results[0].score, 0.15);
    }
}

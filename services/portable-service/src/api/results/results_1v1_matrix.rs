use std::{collections::HashMap, sync::Arc};

use actix_web::{Responder, post, web};
use redis::AsyncCommands;
use share::models::api::{
    ApiData, ApiMsg, ApiResponse, Results1v1MatrixItem, Results1v1MatrixRequest,
    Results1v1MatrixResponse,
};

use crate::{AppState, state::ResultsType};

#[post("/results/1v1_matrix")]
pub async fn results_1v1_matrix_fn(
    state: web::Data<AppState>,
    web::Json(req): web::Json<Results1v1MatrixRequest>,
) -> actix_web::Result<impl Responder> {
    let target_topic = match state.topic_service.get_topic(&req.topic_id).await {
        Ok(Some(topic)) if topic.topic_type.supports_1v1_matrix() => topic,
        Ok(_) => {
            return Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::CurTopicNotSupport1v1Matrix,
            }));
        }
        Err(_) => {
            return Ok(web::Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };

    let cache_key = (target_topic.id, ResultsType::Matrix1v1);
    if let Some(cached) = state.results_cache_store.get(&cache_key).await
        && let Some(matrix) = cached.matrix
    {
        return Ok(web::Json(ApiResponse {
            status: 0,
            data: ApiData::Data(matrix.clone()),
            message: ApiMsg::OK,
        }));
    }

    let mut conn = state.database.redis.connection.clone();

    let target_key = format!("{}:op_matrix", cache_key.0);
    let data: HashMap<String, i64> = match conn.hgetall(target_key).await {
        Ok(data) => data,
        Err(_) => {
            return Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::InternalError,
            }));
        }
    };

    let target_key = format!("{}:op_counter", cache_key.0);
    let counter_data: HashMap<String, i64> = match conn.hgetall(target_key).await {
        Ok(data) => data,
        Err(_) => {
            return Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::InternalError,
            }));
        }
    };

    let mut rsp = HashMap::new();
    for (key, value) in data {
        // key format: 1233:201, order not guaranteed
        let parts: Vec<&str> = key.split(':').collect();
        let count = if parts.len() == 2 {
            let (a, b) = (parts[0], parts[1]);
            if let (Ok(a_num), Ok(b_num)) = (a.parse::<i64>(), b.parse::<i64>()) {
                let (min, max) = if a_num < b_num {
                    (a_num, b_num)
                } else {
                    (b_num, a_num)
                };
                let counter_key = format!("{}:{}", min, max);
                counter_data.get(&counter_key).cloned().unwrap_or(0)
            } else {
                let (min, max) = if a < b { (a, b) } else { (b, a) };
                let counter_key = format!("{}:{}", min, max);
                counter_data.get(&counter_key).cloned().unwrap_or(0)
            }
        } else {
            0
        };

        rsp.insert(
            key,
            Results1v1MatrixItem {
                score: value,
                count,
            },
        );
    }

    let mut cached = state
        .results_cache_store
        .get(&cache_key)
        .await
        .unwrap_or_default();

    let response = Arc::new(Results1v1MatrixResponse(rsp));

    cached.matrix = Some(response.clone());
    state.results_cache_store.insert(cache_key, cached).await;

    Ok(web::Json(ApiResponse {
        status: 0,
        data: ApiData::Data(response),
        message: ApiMsg::OK,
    }))
}

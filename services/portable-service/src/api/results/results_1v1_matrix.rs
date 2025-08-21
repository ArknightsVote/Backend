use std::collections::HashMap;

use actix_web::{Responder, post, web};
use redis::AsyncCommands;
use share::models::api::{
    ApiData, ApiMsg, ApiResponse, Results1v1MatrixItem, Results1v1MatrixRequest,
    Results1v1MatrixResponse,
};

use crate::AppState;

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

    let mut conn = state.database.redis.connection.clone();

    let target_key = format!("{}:op_matrix", target_topic.id);
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

    let target_key = format!("{}:op_counter", target_topic.id);
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

    Ok(web::Json(ApiResponse {
        status: 0,
        data: ApiData::Data(Results1v1MatrixResponse(rsp)),
        message: ApiMsg::OK,
    }))
}

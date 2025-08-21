use std::collections::HashMap;

use actix_web::{Responder, post, web};
use redis::AsyncCommands;
use share::models::api::{
    ApiData, ApiMsg, ApiResponse, Results1v1MatrixRequest, Results1v1MatrixResponse,
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

    Ok(web::Json(ApiResponse {
        status: 0,
        data: ApiData::Data(Results1v1MatrixResponse(data)),
        message: ApiMsg::OK,
    }))
}

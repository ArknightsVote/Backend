use std::{collections::HashMap, sync::Arc};

use axum::{Json, extract::State};
use redis::AsyncCommands;
use share::models::api::{
    ApiData, ApiMsg, ApiResponse, Results1v1MatrixItem, Results1v1MatrixRequest,
    Results1v1MatrixResponse,
};

use crate::{AppState, error::AppError};

#[utoipa::path(
    post,
    path = "/results/1v1_matrix",
    request_body = Results1v1MatrixRequest,
    responses(
        (status = 200, description = "Get operators 1v1 matrix for a topic", body = ApiResponse<Results1v1MatrixResponse>),
        (status = 400, description = "Bad request", body = ApiResponse<String>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    tag = "Results",
    operation_id = "results1v1Matrix"
)]
#[axum::debug_handler]
pub async fn results_1v1_matrix(
    State(state): State<Arc<AppState>>,
    Json(req): Json<Results1v1MatrixRequest>,
) -> Result<Json<ApiResponse<Results1v1MatrixResponse>>, AppError> {
    let target_topic = match state.topic_service.get_topic(&req.topic_id).await {
        Ok(Some(topic)) if topic.topic_type.supports_1v1_matrix() => topic,
        Ok(_) => {
            return Ok(Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::CurTopicNotSupport1v1Matrix,
            }));
        }
        Err(_) => {
            return Ok(Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };

    let mut conn = state.redis.connection.clone();

    let target_key = format!("{}:op_matrix", target_topic.id);
    let data: HashMap<String, i64> = conn.hgetall(target_key).await?;

    let mut rsp = HashMap::new();
    for (key, value) in data {
        rsp.insert(
            key,
            Results1v1MatrixItem {
                score: value,
                count: 1,
            },
        );
    }

    Ok(Json(ApiResponse {
        status: 0,
        data: ApiData::Data(Results1v1MatrixResponse(rsp)),
        message: ApiMsg::OK,
    }))
}

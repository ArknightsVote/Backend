use std::{collections::HashMap, sync::Arc};

use axum::{Json, extract::State};
use redis::AsyncCommands;
use share::models::api::{
    ApiMsg, ApiResponse, Operators1v1MatrixRequest, Operators1v1MatrixResponse,
};

use crate::{AppState, error::AppError};

#[utoipa::path(
    get,
    path = "/operators_1v1_matrix",
    responses(
        (status = 200, description = "Get operators 1v1 matrix", body = Operators1v1MatrixResponse),
        (status = 400, description = "Bad request", body = ApiResponse<String>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    params(
        ("topic_id" = String, description = "ID of the topic for which to get the 1v1 matrix")
    )
)]
#[axum::debug_handler]
pub async fn operators_1v1_matrix(
    State(state): State<Arc<AppState>>,
    Json(req): Json<Operators1v1MatrixRequest>,
) -> Result<Json<ApiResponse<Operators1v1MatrixResponse>>, AppError> {
    let _target_topic = match state.topic_service.get_topic(&req.topic_id).await {
        Ok(Some(topic)) if topic.topic_type.supports_1v1_matrix() => topic,
        Ok(_) => {
            return Ok(Json(ApiResponse {
                status: 500,
                data: None,
                message: ApiMsg::CurTopicNotSupport1v1Matrix,
            }));
        }
        Err(_) => {
            return Ok(Json(ApiResponse {
                status: 404,
                data: None,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };

    let mut conn = state.redis.connection.clone();

    let data: HashMap<String, i64> = conn.hgetall("op_matrix").await?;

    Ok(Json(ApiResponse {
        status: 0,
        data: Some(Operators1v1MatrixResponse { data }),
        message: ApiMsg::OK,
    }))
}

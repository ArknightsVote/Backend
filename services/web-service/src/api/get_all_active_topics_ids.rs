use std::sync::Arc;

use axum::{Json, extract::State};
use mongodb::bson::doc;
use share::models::api::{ApiMsg, ApiResponse, GetAllTopicsIdsResponse};

use crate::{AppState, error::AppError};

#[utoipa::path(
    get,
    path = "/topics",
    responses(
        (status = 200, description = "Get all active topic IDs", body = ApiResponse<GetAllTopicsIdsResponse>),
        (status = 404, description = "Topics not found", body = ApiResponse<String>)
    ),
    tag = "Topics",
    operation_id = "getAllActiveTopicsIds"
)]
#[axum::debug_handler]
pub async fn get_all_active_topics_ids(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<GetAllTopicsIdsResponse>>, AppError> {
    let topic_ids = state.topic_service.get_active_topic_ids().await?;

    Ok(Json(ApiResponse {
        status: 0,
        data: Some(GetAllTopicsIdsResponse { topic_ids }),
        message: ApiMsg::OK,
    }))
}

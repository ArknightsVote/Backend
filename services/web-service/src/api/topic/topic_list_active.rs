use std::sync::Arc;

use axum::{Json, extract::State};
use mongodb::bson::doc;
use share::models::api::{ApiMsg, ApiResponse, TopicsListActiveResponse};

use crate::{AppState, error::AppError};

#[utoipa::path(
    post,
    path = "/topic/list",
    responses(
        (status = 200, description = "List all active topics", body = ApiResponse<TopicsListActiveResponse>),
        (status = 404, description = "No topics found", body = ApiResponse<String>)
    ),
    tag = "Topic",
    operation_id = "topicsListActive"
)]
#[axum::debug_handler]
pub async fn topic_list_active(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<TopicsListActiveResponse>>, AppError> {
    let topic_ids = state.topic_service.get_active_topic_ids().await?;

    Ok(Json(ApiResponse {
        status: 0,
        data: Some(TopicsListActiveResponse { topic_ids }),
        message: ApiMsg::OK,
    }))
}

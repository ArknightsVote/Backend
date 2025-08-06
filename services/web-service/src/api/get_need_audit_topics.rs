use std::sync::Arc;

use axum::{Json, extract::State};
use share::models::api::{ApiMsg, ApiResponse, GetAuditTopicsResponse};

use crate::{AppState, error::AppError};

#[utoipa::path(
    get,
    path = "/topics/need_audit",
    responses(
        (status = 200, description = "Get topics that need audit", body = ApiResponse<GetAuditTopicsResponse>),
        (status = 404, description = "No topics found", body = ApiResponse<String>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    tag = "Topics",
    operation_id = "getNeedAuditTopics"
)]
#[axum::debug_handler]
pub async fn get_need_audit_topics(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<GetAuditTopicsResponse>>, AppError> {
    let audit_topics = state.topic_service.get_need_audit_topics().await?;

    Ok(Json(ApiResponse {
        status: 0,
        data: Some(GetAuditTopicsResponse {
            topics: audit_topics,
        }),
        message: ApiMsg::OK,
    }))
}

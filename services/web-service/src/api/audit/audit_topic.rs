use std::sync::Arc;

use axum::{Json, extract::State};
use share::models::api::{ApiData, ApiMsg, ApiResponse, AuditTopicRequest};

use crate::{AppState, error::AppError};

#[utoipa::path(
    post,
    path = "/audit/topic",
    request_body = AuditTopicRequest,
    responses(
        (status = 200, description = "Audit topic successfully", body = ApiResponse<String>),
        (status = 404, description = "Topic not found", body = ApiResponse<String>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    tag = "Audit",
    operation_id = "auditTopic"
)]
#[axum::debug_handler]
pub async fn audit_topic(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuditTopicRequest>,
) -> Result<Json<ApiResponse<ApiData<String>>>, AppError> {
    let topic_id = req.topic_id;
    state
        .topic_service
        .audit_topic(&topic_id, req.audit_info)
        .await?;

    Ok(Json(ApiResponse {
        status: 0,
        data: ApiData::Empty,
        message: ApiMsg::OK,
    }))
}

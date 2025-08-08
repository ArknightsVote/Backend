use std::sync::Arc;

use axum::{Json, extract::State};
use share::models::api::{ApiData, ApiMsg, ApiResponse, AuditTopicsListResponse};

use crate::{AppState, error::AppError};

#[utoipa::path(
    post,
    path = "/audit/need_audit_topics",
    responses(
        (status = 200, description = "List topics that need audit", body = ApiResponse<AuditTopicsListResponse>),
        (status = 404, description = "No topics found", body = ApiResponse<String>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    tag = "Audit",
    operation_id = "auditTopicsList"
)]
#[axum::debug_handler]
pub async fn audit_topics_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<AuditTopicsListResponse>>, AppError> {
    let audit_topics = state.topic_service.get_need_audit_topics().await?;

    Ok(Json(ApiResponse {
        status: 0,
        data: ApiData::Data(AuditTopicsListResponse {
            topics: audit_topics,
        }),
        message: ApiMsg::OK,
    }))
}

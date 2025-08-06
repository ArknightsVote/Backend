use std::sync::Arc;

use axum::{Json, extract::State};
use chrono::Utc;
use share::models::{
    api::{ApiMsg, ApiResponse, CreateTopicRequest, CreateTopicResponse},
    database::{CreateTopicStatus, VotingTopic},
};
use uuid::Uuid;

use crate::{AppState, error::AppError};

#[utoipa::path(
    post,
    path = "/topics/create",
    request_body = CreateTopicRequest,
    responses(
        (status = 200, description = "Create a new topic", body = ApiResponse<CreateTopicResponse>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    tag = "Topics",
    operation_id = "createTopic"
)]
#[axum::debug_handler]
pub async fn create_topic(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTopicRequest>,
) -> Result<Json<ApiResponse<CreateTopicResponse>>, AppError> {
    let topic = VotingTopic {
        id: if req.id.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            req.id
        },
        name: req.name,
        title: req.title,
        description: req.description,
        topic_type: req.topic_type,
        candidate_pool: req.candidate_pool,
        created_at: Utc::now(),
        updated_at: None,
        open_time: req.open_time,
        close_time: req.close_time,
        is_active: false,
        status: CreateTopicStatus::WaitingAudit,
    };

    match state.topic_service.create_topic(&topic).await {
        Ok(_) => Ok(Json(ApiResponse {
            status: 0,
            data: Some(CreateTopicResponse {
                id: topic.id,
                is_active: topic.is_active,
                status: topic.status,
            }),
            message: ApiMsg::OK,
        })),
        Err(e) => {
            tracing::error!("Failed to create topic: {}", e);
            Ok(Json(ApiResponse {
                status: 500,
                data: None,
                message: ApiMsg::TopicCreateFailed,
            }))
        }
    }
}

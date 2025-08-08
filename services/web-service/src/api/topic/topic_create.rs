use std::sync::Arc;

use axum::{Json, extract::State};
use chrono::Utc;
use share::models::{
    api::{ApiData, ApiMsg, ApiResponse, TopicCreateRequest, TopicCreateResponse},
    database::{CreateTopicStatus, VotingTopic},
};
use uuid::Uuid;

use crate::{AppState, error::AppError};

#[utoipa::path(
    post,
    path = "/topic/create",
    request_body = TopicCreateRequest,
    responses(
        (status = 200, description = "Create a new topic", body = ApiResponse<TopicCreateResponse>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    tag = "Topic",
    operation_id = "topicCreate"
)]
#[axum::debug_handler]
pub async fn topic_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TopicCreateRequest>,
) -> Result<Json<ApiResponse<TopicCreateResponse>>, AppError> {
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
            data: ApiData::Data(TopicCreateResponse {
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
                data: ApiData::Empty,
                message: ApiMsg::TopicCreateFailed,
            }))
        }
    }
}

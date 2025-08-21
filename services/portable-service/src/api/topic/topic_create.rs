use actix_web::{post, web};
use chrono::Utc;
use share::models::{
    api::{ApiData, ApiMsg, ApiResponse, TopicCreateRequest, TopicCreateResponse},
    database::{CreateTopicStatus, VotingTopic},
};
use uuid::Uuid;

use crate::{AppState, error::AppError};

#[post("/topic/create")]
pub async fn topic_create_fn(
    state: web::Data<AppState>,
    web::Json(req): web::Json<TopicCreateRequest>,
) -> Result<web::Json<ApiResponse<TopicCreateResponse>>, AppError> {
    if true {
        return Ok(web::Json(ApiResponse {
            status: 403,
            data: ApiData::Empty,
            message: ApiMsg::EndpointForbidden,
        }));
    }

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
        Ok(_) => Ok(web::Json(ApiResponse {
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
            Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::TopicCreateFailed,
            }))
        }
    }
}

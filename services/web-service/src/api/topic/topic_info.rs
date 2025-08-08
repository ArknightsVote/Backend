use std::sync::Arc;

use axum::{Json, extract::State};
use mongodb::bson::doc;
use share::models::api::{ApiData, ApiMsg, ApiResponse, TopicInfoRequest, TopicInfoResponse};

use crate::{AppState, error::AppError};

#[utoipa::path(
    post,
    path = "/topic/info",
    request_body = TopicInfoRequest,
    responses(
        (status = 200, description = "Get topic information", body = ApiResponse<TopicInfoResponse>),
        (status = 404, description = "Topic not found", body = ApiResponse<String>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    tag = "Topic",
    operation_id = "topicInfo"
)]
#[axum::debug_handler]
pub async fn topic_info(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TopicInfoRequest>,
) -> Result<Json<ApiResponse<TopicInfoResponse>>, AppError> {
    match state.topic_service.get_topic(&req.topic_id).await {
        Ok(Some(topic)) => Ok(Json(ApiResponse {
            status: 0,
            data: ApiData::Data(TopicInfoResponse {
                id: topic.id,
                name: topic.name,
                title: topic.title,
                description: topic.description,
                topic_type: topic.topic_type,
                open_time: topic.open_time,
                close_time: topic.close_time,
            }),
            message: ApiMsg::OK,
        })),
        Ok(None) => Ok(Json(ApiResponse {
            status: 404,
            data: ApiData::Empty,
            message: ApiMsg::TargetTopicNotFound,
        })),
        Err(_) => Ok(Json(ApiResponse {
            status: 500,
            data: ApiData::Empty,
            message: ApiMsg::InternalError,
        })),
    }
}

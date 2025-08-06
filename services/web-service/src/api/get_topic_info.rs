use std::sync::Arc;

use axum::{Json, extract::State};
use mongodb::bson::doc;
use share::models::api::{ApiMsg, ApiResponse, GetTopicInfoRequest, GetTopicInfoResponse};

use crate::{AppState, error::AppError};

#[utoipa::path(
    post,
    path = "/topics/info",
    request_body = GetTopicInfoRequest,
    responses(
        (status = 200, description = "Get topic information", body = ApiResponse<GetTopicInfoResponse>),
        (status = 404, description = "Topic not found", body = ApiResponse<String>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    tag = "Topics",
    operation_id = "getTopicInfo"
)]
#[axum::debug_handler]
pub async fn get_topic_info(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GetTopicInfoRequest>,
) -> Result<Json<ApiResponse<GetTopicInfoResponse>>, AppError> {
    match state.topic_service.get_topic(&req.topic_id).await {
        Ok(Some(topic)) => Ok(Json(ApiResponse {
            status: 0,
            data: Some(GetTopicInfoResponse {
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
            data: None,
            message: ApiMsg::TargetTopicNotFound,
        })),
        Err(_) => Ok(Json(ApiResponse {
            status: 500,
            data: None,
            message: ApiMsg::InternalError,
        })),
    }
}

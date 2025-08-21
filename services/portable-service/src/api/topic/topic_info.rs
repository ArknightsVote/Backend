use actix_web::{post, web};
use share::models::api::{ApiData, ApiMsg, ApiResponse, TopicInfoRequest, TopicInfoResponse};

use crate::{AppState, error::AppError};

#[post("/topic/info")]
pub async fn topic_info_fn(
    state: web::Data<AppState>,
    web::Json(req): web::Json<TopicInfoRequest>,
) -> Result<web::Json<ApiResponse<TopicInfoResponse>>, AppError> {
    match state.topic_service.get_topic(&req.topic_id).await {
        Ok(Some(topic)) => Ok(web::Json(ApiResponse {
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
        Ok(None) => Ok(web::Json(ApiResponse {
            status: 404,
            data: ApiData::Empty,
            message: ApiMsg::TargetTopicNotFound,
        })),
        Err(_) => Ok(web::Json(ApiResponse {
            status: 500,
            data: ApiData::Empty,
            message: ApiMsg::InternalError,
        })),
    }
}

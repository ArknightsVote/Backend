use std::sync::Arc;

use axum::{Json, extract::State};
use mongodb::bson::doc;
use share::models::{
    api::{GetTopicInfoRequest, GetTopicInfoResponse},
    database::VotingTopic,
};

use crate::{AppState, error::AppError};

use super::{ApiMsg, ApiResponse};

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
    let mongo_collection = state.mongodb.collection::<VotingTopic>("topics");
    let topic = mongo_collection.find_one(doc! { "id": req.topic_id }).await;

    let rsp = match topic {
        Ok(Some(topic)) => ApiResponse {
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
        },
        Ok(None) => ApiResponse {
            status: 404,
            data: None,
            message: ApiMsg::TargetTopicNotFound,
        },
        Err(_) => ApiResponse {
            status: 500,
            data: None,
            message: ApiMsg::InternalError,
        },
    };

    Ok(Json(rsp))
}

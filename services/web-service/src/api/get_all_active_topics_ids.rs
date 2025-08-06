use std::sync::Arc;

use axum::{Json, extract::State};
use futures::StreamExt as _;
use mongodb::bson::doc;
use share::models::{api::GetAllTopicsIdsResponse, database::VotingTopic};

use crate::{AppState, error::AppError};

use super::{ApiMsg, ApiResponse};

#[utoipa::path(
    get,
    path = "/topics",
    responses(
        (status = 200, description = "Get all active topic IDs", body = ApiResponse<GetAllTopicsIdsResponse>),
        (status = 404, description = "Topics not found", body = ApiResponse<String>)
    ),
    tag = "Topics",
    operation_id = "getAllActiveTopicsIds"
)]
#[axum::debug_handler]
pub async fn get_all_active_topics_ids(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<GetAllTopicsIdsResponse>>, AppError> {
    let collection = state.mongodb.collection::<VotingTopic>("topics");
    let topics = match collection.find(doc! { "is_active": true }).await {
        Ok(cursor) => cursor,
        Err(_) => {
            return Ok(Json(ApiResponse {
                status: 404,
                data: None,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };

    let topic_results: Vec<Result<VotingTopic, _>> = topics.collect().await;
    let topic_ids: Vec<String> = topic_results
        .into_iter()
        .filter_map(|result| result.ok().map(|topic| topic.id))
        .collect();

    Ok(Json(ApiResponse {
        status: 0,
        data: Some(GetAllTopicsIdsResponse { topic_ids }),
        message: ApiMsg::OK,
    }))
}

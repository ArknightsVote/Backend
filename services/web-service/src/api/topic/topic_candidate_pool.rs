use std::sync::Arc;

use axum::{Json, extract::State};
use share::models::api::{
    ApiData, ApiMsg, ApiResponse, CharacterPortrait, TopicCandidatePoolRequest, TopicCandidatePoolResponse
};

use crate::{AppState, error::AppError};

#[utoipa::path(
    post,
    path = "/topic/candidate_pool",
    request_body = TopicCandidatePoolRequest,
    responses(
        (status = 200, description = "Get candidate pool for a topic", body = ApiResponse<TopicCandidatePoolResponse>),
        (status = 404, description = "Topic not found", body = ApiResponse<String>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    tag = "Topic",
    operation_id = "topicCandidatePool"
)]
#[axum::debug_handler]
pub async fn topic_candidate_pool(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TopicCandidatePoolRequest>,
) -> Result<Json<ApiResponse<TopicCandidatePoolResponse>>, AppError> {
    let candidate_pool = state
        .topic_service
        .get_candidate_pool(&payload.topic_id, &state.character_infos)
        .await;

    match candidate_pool {
        Some(candidate_pool) => {
            let mut pool: Vec<CharacterPortrait> = candidate_pool
                .into_iter()
                .filter_map(|char_id| state.character_portraits.get(&char_id).cloned())
                .collect();

            pool.sort_unstable_by_key(|info| info.id);

            Ok(Json(ApiResponse {
                status: 0,
                data: ApiData::Data(TopicCandidatePoolResponse {
                    topic_id: payload.topic_id,
                    pool,
                }),
                message: ApiMsg::OK,
            }))
        }
        None => Ok(Json(ApiResponse {
            status: 404,
            data: ApiData::Empty,
            message: ApiMsg::TargetTopicNotFound,
        })),
    }
}

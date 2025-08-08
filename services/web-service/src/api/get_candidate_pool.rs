use std::sync::Arc;

use axum::{Json, extract::State};
use share::models::api::{
    ApiMsg, ApiResponse, CharacterPortrait, GetCandidatePoolRequest, GetCandidatePoolResponse,
};

use crate::{AppState, error::AppError};

#[utoipa::path(
    post,
    path = "/topics/candidate_pool",
    request_body = GetCandidatePoolRequest,
    responses(
        (status = 200, description = "Get candidate pool", body = ApiResponse<GetCandidatePoolResponse>),
        (status = 404, description = "Topic not found", body = ApiResponse<String>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    tag = "Topics",
    operation_id = "getCandidatePool"
)]
#[axum::debug_handler]
pub async fn get_candidate_pool(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GetCandidatePoolRequest>,
) -> Result<Json<ApiResponse<GetCandidatePoolResponse>>, AppError> {
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
                data: Some(GetCandidatePoolResponse {
                    topic_id: payload.topic_id,
                    pool,
                }),
                message: ApiMsg::OK,
            }))
        }
        None => Ok(Json(ApiResponse {
            status: 404,
            data: None,
            message: ApiMsg::TargetTopicNotFound,
        })),
    }
}

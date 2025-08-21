use actix_web::{post, web};
use share::models::api::{
    ApiData, ApiMsg, ApiResponse, CharacterPortrait, TopicCandidatePoolRequest,
    TopicCandidatePoolResponse,
};

use crate::{AppState, error::AppError};

#[post("/topic/candidate_pool")]
pub async fn topic_candidate_pool_fn(
    state: web::Data<AppState>,
    web::Json(payload): web::Json<TopicCandidatePoolRequest>,
) -> Result<web::Json<ApiResponse<TopicCandidatePoolResponse>>, AppError> {
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

            Ok(web::Json(ApiResponse {
                status: 0,
                data: ApiData::Data(TopicCandidatePoolResponse {
                    topic_id: payload.topic_id,
                    pool,
                }),
                message: ApiMsg::OK,
            }))
        }
        None => Ok(web::Json(ApiResponse {
            status: 404,
            data: ApiData::Empty,
            message: ApiMsg::TargetTopicNotFound,
        })),
    }
}

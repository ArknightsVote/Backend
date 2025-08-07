use std::sync::Arc;

use axum::{Json, extract::State};
use rand::seq::IndexedRandom as _;
use redis::AsyncCommands as _;
use share::models::{
    api::{ApiMsg, ApiResponse, NewCompareRequest, NewCompareResponse},
    database::VotingTopicType,
};

use crate::{
    AppState,
    api::utils::{generate_random_string, publish_and_ack},
    constants::BALLOT_CODE_RANDOM_LENGTH,
    error::AppError,
};

fn select_operators(operator_ids: &[i32]) -> Result<(i32, i32), AppError> {
    if operator_ids.len() < 2 {
        return Err(AppError::InsufficientOperators);
    }

    let mut rng = rand::rng();
    let selected: [i32; 2] = operator_ids.choose_multiple_array(&mut rng).unwrap();

    Ok((selected[0], selected[1]))
}

#[utoipa::path(
    post,
    path = "/new_compare",
    request_body = NewCompareRequest,
    responses(
        (status = 200, description = "New comparison created", body = ApiResponse<NewCompareResponse>),
        (status = 404, description = "Topic not found or not active", body = ApiResponse<String>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    tag = "Comparison",
    operation_id = "newCompare"
)]
#[axum::debug_handler]
pub async fn new_compare(
    State(state): State<Arc<AppState>>,
    Json(req): Json<NewCompareRequest>,
) -> Result<Json<ApiResponse<NewCompareResponse>>, AppError> {
    let topic = match state.topic_service.get_topic(&req.topic_id).await {
        Ok(Some(topic)) if topic.is_topic_active() => topic,
        Ok(_) => {
            return Ok(Json(ApiResponse {
                status: 500,
                data: None,
                message: ApiMsg::TargetTopicNotActive,
            }));
        }
        Err(_) => {
            return Ok(Json(ApiResponse {
                status: 404,
                data: None,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };
    let topic_id = topic.id;

    match topic.topic_type {
        VotingTopicType::Pairwise => {
            let target_candidate_pool = topic.candidate_pool.generate_pool(&state.character_infos);

            let (left, right) = select_operators(&target_candidate_pool)?;

            let id = state.snowflake.next_id()?;
            let random_string = generate_random_string(BALLOT_CODE_RANDOM_LENGTH);
            let ballot_id = format!("{id}-{random_string}");

            let mut conn = state.redis.connection.clone();
            let ballot_key = format!("{topic_id}:ballot:{ballot_id}");
            let ballot_value = format!("{left},{right}");
            let _: () = conn.set_ex(&ballot_key, &ballot_value, 86400).await?; // 24 hours expiration

            let rsp = NewCompareResponse::Pairwise {
                topic_id,
                ballot_id,
                left,
                right,
            };

            if !req.ballot_id.is_empty() {
                let req_data = serde_json::to_vec(&req).map_err(AppError::from)?;
                publish_and_ack(&state.jetstream, "ark-vote.new_compare_request", req_data).await?;
            }

            Ok(Json(ApiResponse {
                status: 0,
                data: Some(rsp),
                message: ApiMsg::OK,
            }))
        }
        _ => Ok(Json(ApiResponse {
            status: 1,
            data: None,
            message: ApiMsg::UnsupportedTopicType,
        })),
    }
}

#[cfg(test)]
mod tests {
    use crate::api::utils::generate_random_string;

    use super::*;

    #[test]
    fn test_generate_random_string() {
        let s = generate_random_string(10);
        assert_eq!(s.len(), 10);
        assert!(s.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_select_operators() {
        let operators = vec![1, 2, 3, 4, 5];
        let (left, right) = select_operators(&operators).unwrap();
        assert_ne!(left, right);
        assert!(operators.contains(&left));
        assert!(operators.contains(&right));
    }

    #[test]
    fn test_select_operators_insufficient() {
        let operators = vec![1];
        assert!(select_operators(&operators).is_err());
    }
}

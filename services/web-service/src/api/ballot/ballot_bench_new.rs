use std::sync::Arc;

use axum::{Json, extract::State};
use rand::seq::IndexedRandom as _;
use redis::AsyncCommands as _;
use share::models::{
    api::{
        ApiData, ApiMsg, ApiResponse, BallotCreateResponse, BallotSaveRequest, PairwiseSaveScore,
    },
    database::VotingTopicType,
};

use crate::{
    AppState, api::utils::generate_random_string, constants::BALLOT_CODE_RANDOM_LENGTH,
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

#[axum::debug_handler]
pub async fn ballot_bench_new(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<BallotCreateResponse>>, AppError> {
    let topic = match state
        .topic_service
        .get_topic("crisis_v2_season_4_1_benchtest")
        .await
    {
        Ok(Some(topic)) if topic.is_topic_active() => topic,
        Ok(_) => {
            return Ok(Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotActive,
            }));
        }
        Err(_) => {
            return Ok(Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };
    let topic_id = topic.id;
    let candidate_pool = match state
        .topic_service
        .get_candidate_pool(&topic_id, &state.character_infos)
        .await
    {
        Some(pool) => pool,
        None => {
            return Ok(Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };

    match topic.topic_type {
        VotingTopicType::Pairwise => {
            let (left, right) = select_operators(&candidate_pool)?;

            let id = state.snowflake.next_id()?;
            let random_string = generate_random_string(BALLOT_CODE_RANDOM_LENGTH);
            let ballot_id = format!("{id}-{random_string}");

            let mut conn = state.redis.connection.clone();
            let ballot_key = format!("{topic_id}:ballot:{ballot_id}");
            let ballot_value = format!("{left},{right}");
            let _: () = conn.set_ex(&ballot_key, &ballot_value, 86400).await?; // 24 hours expiration

            state.bench_ballot_store.insert(
                ballot_key,
                BallotSaveRequest::Pairwise(PairwiseSaveScore {
                    topic_id: topic_id.clone(),
                    ballot_id: ballot_id.clone(),
                    winner: left,
                    loser: right,
                }),
            );

            let rsp = BallotCreateResponse::Pairwise {
                topic_id,
                ballot_id,
                left,
                right,
            };

            Ok(Json(ApiResponse {
                status: 0,
                data: ApiData::Data(rsp),
                message: ApiMsg::OK,
            }))
        }
        _ => Ok(Json(ApiResponse {
            status: 1,
            data: ApiData::Empty,
            message: ApiMsg::UnsupportedTopicType,
        })),
    }
}

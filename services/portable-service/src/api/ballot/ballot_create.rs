use actix_web::{Responder, post, web};
use rand::{Rng, distr::Alphanumeric, seq::IndexedRandom as _};
use share::models::{
    api::{ApiData, ApiMsg, ApiResponse, BallotCreateRequest, BallotCreateResponse},
    database::VotingTopicType,
};

use crate::{AppState, constants::BALLOT_CODE_RANDOM_LENGTH, error::AppError};

fn generate_random_string(length: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

fn select_operators(operator_ids: &[i32]) -> Result<(i32, i32), AppError> {
    if operator_ids.len() < 2 {
        return Err(AppError::InsufficientOperators);
    }

    let mut rng = rand::rng();
    let selected: [i32; 2] = operator_ids.choose_multiple_array(&mut rng).unwrap();

    Ok((selected[0], selected[1]))
}

#[post("/ballot/new")]
pub async fn ballot_create_fn(
    state: web::Data<AppState>,
    web::Json(req): web::Json<BallotCreateRequest>,
) -> actix_web::Result<impl Responder> {
    let topic = match state.topic_service.get_topic(&req.topic_id).await {
        Ok(Some(topic)) if topic.is_topic_active() => topic,
        Ok(None) => {
            return Ok(web::Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
        Ok(_) => {
            return Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotActive,
            }));
        }
        Err(_) => {
            return Ok(web::Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };

    let candidate_pool = match state
        .topic_service
        .get_candidate_pool(&topic.id, &state.character_infos)
        .await
    {
        Some(pool) => pool,
        None => {
            return Ok(web::Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };

    match topic.topic_type {
        VotingTopicType::Pairwise => {
            let (left, right) = select_operators(&candidate_pool)?;

            let id = state.snowflake.next_id().unwrap();
            let random_string = generate_random_string(BALLOT_CODE_RANDOM_LENGTH);
            let ballot_id = format!("{id}-{random_string}");

            let ballot_key = format!("{}:ballot:{ballot_id}", topic.id);

            state
                .ballot_cache_store
                .insert(ballot_key, (left, right))
                .await;

            let rsp = BallotCreateResponse::Pairwise {
                topic_id: topic.id,
                ballot_id,
                left,
                right,
            };

            Ok(web::Json(ApiResponse {
                status: 0,
                data: ApiData::Data(rsp),
                message: ApiMsg::OK,
            }))
        }
        _ => Ok(web::Json(ApiResponse {
            status: 1,
            data: ApiData::Empty,
            message: ApiMsg::UnsupportedTopicType,
        })),
    }
}

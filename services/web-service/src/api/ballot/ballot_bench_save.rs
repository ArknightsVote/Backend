use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
};
use share::models::{
    api::{ApiData, ApiMsg, ApiResponse, BallotSaveRequest, BallotSaveResponse, PairwiseSaveScore},
    database::{Ballot, BallotInfo, PairwiseBallot},
};

use crate::{AppState, api::utils::publish_and_ack, error::AppError};

#[axum::debug_handler]
pub async fn ballot_bench_save(
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<BallotSaveResponse>>, AppError> {
    let req = state.bench_ballot_store.iter().next().unwrap().clone();

    let _target_topic = match state.topic_service.get_topic(req.topic_id()).await {
        Ok(Some(topic)) if topic.is_topic_active() && topic.topic_type.matches_request(&req) => {
            topic
        }
        Ok(Some(topic)) if !topic.topic_type.matches_request(&req) => {
            return Ok(Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::RequestTopicTypeMismatch,
            }));
        }
        Ok(Some(topic)) if !topic.is_topic_active() => {
            return Ok(Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotActive,
            }));
        }
        Ok(None) => {
            return Ok(Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
        Ok(Some(_)) => {
            return Ok(Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::InternalError,
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

    let ip = addr.ip().to_string();
    let user_agent = headers
        .get("User-Agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    match req {
        BallotSaveRequest::Pairwise(PairwiseSaveScore {
            topic_id,
            ballot_id,
            winner,
            loser,
        }) => {
            if winner == loser {
                return Err(AppError::SameParticipant);
            }

            let ballot = Ballot::Pairwise(PairwiseBallot {
                info: BallotInfo {
                    topic_id: topic_id.into(),
                    ballot_id: ballot_id.into(),
                    ip: ip.into(),
                    user_agent: user_agent.into(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                },
                win: winner,
                lose: loser,
            });

            publish_and_ack(
                &state.jetstream,
                "ark-vote.save_score",
                serde_json::to_vec(&ballot)?,
            )
            .await?;

            Ok(Json(ApiResponse {
                status: 0,
                data: ApiData::Data(BallotSaveResponse { code: 0 }),
                message: ApiMsg::OK,
            }))
        }
        _ => Err(AppError::InternalError(
            "Unsupported request type".to_string(),
        )),
    }
}

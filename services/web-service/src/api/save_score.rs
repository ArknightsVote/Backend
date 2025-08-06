use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
};
use share::models::{
    api::{PairwiseSaveScore, SaveScoreRequest, SaveScoreResponse},
    database::{Ballot, BallotInfo, PairwiseBallot},
};

use crate::{AppState, api::utils::publish_and_ack, error::AppError};

use super::{ApiMsg, ApiResponse};

#[utoipa::path(
    post,
    path = "/save_score",
    tag = "Voting",
    request_body = SaveScoreRequest,
    responses(
        (status = 200, description = "Score saved successfully", body = ApiResponse<SaveScoreResponse>),
        (status = 400, description = "Bad Request", body = ApiMsg),
        (status = 404, description = "Topic not found", body = ApiMsg),
        (status = 500, description = "Internal Server Error", body = ApiMsg)
    )
)]
#[axum::debug_handler]
pub async fn save_score(
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveScoreRequest>,
) -> Result<Json<ApiResponse<SaveScoreResponse>>, AppError> {
    let _target_topic = match state.voting_topics_cache.get(req.topic_id()) {
        Some(topic) if topic.is_topic_active() && topic.topic_type.matches_request(&req) => {
            topic.clone()
        }
        Some(topic) if !topic.topic_type.matches_request(&req) => {
            return Ok(Json(ApiResponse {
                status: 1,
                data: None,
                message: ApiMsg::RequestTopicTypeMismatch,
            }));
        }
        Some(_) => {
            return Ok(Json(ApiResponse {
                status: 1,
                data: None,
                message: ApiMsg::TargetTopicNotActive,
            }));
        }
        None => {
            return Ok(Json(ApiResponse {
                status: 1,
                data: None,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };

    match req {
        SaveScoreRequest::Pairwise(PairwiseSaveScore {
            topic_id,
            ballot_id,
            winner,
            loser,
        }) => {
            if winner == loser {
                return Err(AppError::SameParticipant);
            }

            let ip = addr.ip().to_string();
            let user_agent = headers
                .get("User-Agent")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("unknown");

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
                data: None,
                message: ApiMsg::OK,
            }))
        }
        _ => Err(AppError::InternalError(
            "Unsupported request type".to_string(),
        )),
    }
}

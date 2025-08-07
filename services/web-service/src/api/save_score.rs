use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
};
use share::models::{
    api::{ApiMsg, ApiResponse, PairwiseSaveScore, SaveScoreRequest, SaveScoreResponse},
    database::{Ballot, BallotInfo, PairwiseBallot},
};

use crate::{AppState, api::utils::publish_and_ack, error::AppError};

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
    let _target_topic = match state.topic_service.get_topic(req.topic_id()).await {
        Ok(Some(topic)) if topic.is_topic_active() && topic.topic_type.matches_request(&req) => {
            topic
        }
        Ok(Some(topic)) if !topic.topic_type.matches_request(&req) => {
            return Ok(Json(ApiResponse {
                status: 500,
                data: None,
                message: ApiMsg::RequestTopicTypeMismatch,
            }));
        }
        Ok(Some(topic)) if !topic.is_topic_active() => {
            return Ok(Json(ApiResponse {
                status: 500,
                data: None,
                message: ApiMsg::TargetTopicNotActive,
            }));
        }
        Ok(None) => {
            return Ok(Json(ApiResponse {
                status: 404,
                data: None,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
        Ok(Some(_)) => {
            return Ok(Json(ApiResponse {
                status: 500,
                data: None,
                message: ApiMsg::InternalError,
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

    let ip = addr.ip().to_string();
    let user_agent = headers
        .get("User-Agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

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

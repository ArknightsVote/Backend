use std::sync::Arc;

use axum::{Json, extract::State};
use share::models::api::{ApiData, ApiMsg, ApiResponse, BallotSkipRequest, BallotSkipResponse};

use crate::{AppState, api::utils::publish_and_ack, error::AppError};

#[utoipa::path(
    post,
    path = "/ballot/skip",
    request_body = BallotSkipRequest,
    responses(
        (status = 200, description = "Skip a ballot", body = ApiResponse<BallotSkipResponse>),
        (status = 404, description = "Topic not found or inactive", body = ApiResponse<String>),
        (status = 500, description = "Internal server error", body = ApiResponse<String>)
    ),
    tag = "Ballot",
    operation_id = "ballotSkip"
)]
#[axum::debug_handler]
pub async fn ballot_skip(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BallotSkipRequest>,
) -> Result<Json<ApiResponse<BallotSkipResponse>>, AppError> {
    let _ = match state.topic_service.get_topic(&req.topic_id).await {
        Ok(Some(topic)) if topic.is_topic_active() => topic,
        Ok(None) => {
            return Ok(Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
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

    let req_data = serde_json::to_vec(&req).map_err(AppError::from)?;
    publish_and_ack(&state.jetstream, "ark-vote.ballot_skip", req_data).await?;

    Ok(Json(ApiResponse {
        status: 200,
        data: ApiData::Data(BallotSkipResponse { code: 0 }),
        message: ApiMsg::OK,
    }))
}

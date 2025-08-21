use actix_web::{HttpRequest, Responder, post, web};
use share::models::{
    api::{ApiData, ApiMsg, ApiResponse, BallotSaveRequest, BallotSaveResponse, PairwiseSaveScore},
    database::{Ballot, BallotInfo, PairwiseBallot},
};

use crate::AppState;

#[post("/ballot/save")]
pub async fn ballot_save_fn(
    state: web::Data<AppState>,
    req2: HttpRequest,
    web::Json(req): web::Json<BallotSaveRequest>,
) -> actix_web::Result<impl Responder> {
    let _target_topic = match state.topic_service.get_topic(req.topic_id()).await {
        Ok(Some(topic)) if topic.is_topic_active() && topic.topic_type.matches_request(&req) => {
            topic
        }
        Ok(Some(topic)) if !topic.topic_type.matches_request(&req) => {
            tracing::error!(
                "Request topic type mismatch: expected {:?}, got {:?}",
                topic.topic_type,
                req
            );
            return Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::RequestTopicTypeMismatch,
            }));
        }
        Ok(Some(topic)) if !topic.is_topic_active() => {
            tracing::error!("Target topic is not active: {:?}", topic);
            return Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotActive,
            }));
        }
        Ok(None) => {
            tracing::error!("Target topic not found: {:?}", req.topic_id());
            return Ok(web::Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
        Ok(Some(_)) => {
            tracing::error!(
                "Unexpected error while fetching topic: {:?}",
                req.topic_id()
            );
            return Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::InternalError,
            }));
        }
        Err(_) => {
            tracing::error!("Target topic not found: {:?}", req.topic_id());
            return Ok(web::Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };

    let ballot_key = format!("{}:ballot:{}", req.topic_id(), req.ballot_id());
    let removed = { state.ballot_cache_store.remove(&ballot_key) };
    let store_value = match removed {
        Some((_, value)) => value,
        None => {
            tracing::error!("Ballot not found in cache: {}", ballot_key);
            return Ok(web::Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::BallotNotFound,
            }));
        }
    };

    let realip_remote_addr = match req2.connection_info().realip_remote_addr() {
        Some(addr) => addr.to_owned(),
        None => "unknown".into(),
    };

    let user_agent = req2
        .headers()
        .get("User-Agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    match req {
        BallotSaveRequest::Pairwise(PairwiseSaveScore {
            topic_id,
            ballot_id,
            winner,
            loser,
        }) => {
            if winner == loser {
                tracing::error!(
                    "Winner and loser cannot be the same: winner={}, loser={}",
                    winner,
                    loser
                );
                return Ok(web::Json(ApiResponse {
                    status: 400,
                    data: ApiData::Empty,
                    message: ApiMsg::BallotWinnerCannotBeLoser,
                }));
            }

            // let (ballot_left, ballot_right) = parse_ballot_info(&store_value).map_err(|e| {
            //     tracing::error!("Failed to parse ballot info: {}", e);
            //     AppError::InvalidBallotFormat(e.to_string())
            // })?;
            let (ballot_left, ballot_right) = (store_value.0, store_value.1);

            let valid_ids = [ballot_left, ballot_right];
            if !valid_ids.contains(&winner) || !valid_ids.contains(&loser) {
                tracing::error!("Invalid winner or loser ID: {} vs {}", winner, loser);
                return Ok(web::Json(ApiResponse {
                    status: 400,
                    data: ApiData::Empty,
                    message: ApiMsg::InvalidBallotCode(format!(
                        "Invalid winner or loser ID: {} vs {}",
                        winner, loser
                    )),
                }));
            }

            let ballot = Ballot::Pairwise(PairwiseBallot {
                info: BallotInfo {
                    topic_id: topic_id.into(),
                    ballot_id: ballot_id.into(),
                    ip: realip_remote_addr.into(),
                    user_agent: user_agent.into(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                },
                win: winner,
                lose: loser,
            });

            if let Err(e) = state.ballot_processor.submit_ballot(ballot) {
                tracing::error!("Failed to submit ballot to processor: {}", e);
                return Ok(web::Json(ApiResponse {
                    status: 500,
                    data: ApiData::Empty,
                    message: ApiMsg::InternalError,
                }));
            }

            Ok(web::Json(ApiResponse {
                status: 0,
                data: ApiData::Data(BallotSaveResponse { code: 0 }),
                message: ApiMsg::OK,
            }))
        }
        _ => Ok(web::Json(ApiResponse {
            status: 400,
            data: ApiData::Empty,
            message: ApiMsg::InternalError,
        })),
    }
}

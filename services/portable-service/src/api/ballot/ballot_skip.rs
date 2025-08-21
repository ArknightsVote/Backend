use actix_web::{Responder, post, web};
use share::models::api::{ApiData, ApiMsg, ApiResponse, BallotSkipRequest, BallotSkipResponse};

use crate::AppState;

#[post("/ballot/skip")]
pub async fn ballot_skip_fn(
    state: web::Data<AppState>,
    web::Json(req): web::Json<BallotSkipRequest>,
) -> actix_web::Result<impl Responder> {
    let _ = match state.topic_service.get_topic(&req.topic_id).await {
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

    state
        .ballot_cache_store
        .remove(&format!("{}:ballot:{}", req.topic_id, req.ballot_id));

    Ok(web::Json(ApiResponse {
        status: 200,
        data: ApiData::Data(BallotSkipResponse { code: 0 }),
        message: ApiMsg::OK,
    }))
}

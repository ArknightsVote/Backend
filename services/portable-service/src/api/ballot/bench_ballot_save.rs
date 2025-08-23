use actix_web::{HttpRequest, Responder, dev::ConnectionInfo, get, web};
use share::models::{
    api::{ApiData, ApiMsg, ApiResponse, BallotSaveResponse},
    database::{Ballot, BallotInfo, PairwiseBallot},
};

use crate::AppState;

#[get("/bench/ballot/save")]
pub async fn bench_ballot_save_fn(
    state: web::Data<AppState>,
    conn: ConnectionInfo,
    req2: HttpRequest,
) -> actix_web::Result<impl Responder> {
    let _target_topic = match state
        .topic_service
        .get_topic("crisis_v2_season_4_1_benchtest")
        .await
    {
        Ok(Some(topic)) if topic.is_topic_active() => topic,
        Ok(Some(topic)) if !topic.is_topic_active() => {
            return Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotActive,
            }));
        }
        Ok(None) => {
            return Ok(web::Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
        Ok(Some(_)) => {
            return Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::InternalError,
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

    let (key, store_value) = match state.ballot_cache_store.iter().next() {
        Some((key, value)) => {
            state.ballot_cache_store.invalidate(&*key).await;
            (key, value)
        }
        None => {
            return Ok(web::Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::BallotNotFound,
            }));
        }
    };

    let realip_remote_addr = conn.realip_remote_addr().unwrap_or("unknown").to_string();

    let user_agent = req2
        .headers()
        .get("User-Agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let key = key.to_string();
    let ballot = Ballot::Pairwise(PairwiseBallot {
        info: BallotInfo {
            topic_id: std::borrow::Cow::Borrowed("crisis_v2_season_4_1_benchtest"),
            ballot_id: key.into(),
            ip: realip_remote_addr.into(),
            user_agent: user_agent.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        },
        win: store_value.0,
        lose: store_value.1,
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

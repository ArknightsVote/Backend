use actix_web::{post, web};
use share::models::api::{ApiData, ApiMsg, ApiResponse, AuditTopicRequest};

use crate::{AppState, error::AppError};

#[post("/audit/topic")]
pub async fn audit_topic_fn(
    state: web::Data<AppState>,
    web::Json(req): web::Json<AuditTopicRequest>,
) -> Result<web::Json<ApiResponse<ApiData<String>>>, AppError> {
    if true {
        return Ok(web::Json(ApiResponse {
            status: 403,
            data: ApiData::Empty,
            message: ApiMsg::EndpointForbidden,
        }));
    }

    let topic_id = req.topic_id;
    state
        .topic_service
        .audit_topic(&topic_id, req.audit_info)
        .await?;

    Ok(web::Json(ApiResponse {
        status: 0,
        data: ApiData::Empty,
        message: ApiMsg::OK,
    }))
}

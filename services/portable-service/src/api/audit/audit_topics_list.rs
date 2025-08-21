use actix_web::{post, web};
use share::models::api::{ApiData, ApiMsg, ApiResponse, AuditTopicsListResponse};

use crate::{AppState, error::AppError};

#[post("/audit/need_audit_topics")]
pub async fn audit_topics_list_fn(
    state: web::Data<AppState>,
) -> Result<web::Json<ApiResponse<AuditTopicsListResponse>>, AppError> {
    if true {
        return Ok(web::Json(ApiResponse {
            status: 403,
            data: ApiData::Empty,
            message: ApiMsg::EndpointForbidden,
        }));
    }

    let audit_topics = state.topic_service.get_need_audit_topics().await?;

    Ok(web::Json(ApiResponse {
        status: 0,
        data: ApiData::Data(AuditTopicsListResponse {
            topics: audit_topics,
        }),
        message: ApiMsg::OK,
    }))
}

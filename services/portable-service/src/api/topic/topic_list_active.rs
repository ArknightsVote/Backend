use actix_web::{post, web};
use share::models::api::{ApiData, ApiMsg, ApiResponse, TopicListActiveResponse};

use crate::{AppState, error::AppError};

#[post("/topic/list")]
pub async fn topic_list_active_fn(
    state: web::Data<AppState>,
) -> Result<web::Json<ApiResponse<TopicListActiveResponse>>, AppError> {
    let topic_ids = state.topic_service.get_active_topic_ids().await?;

    Ok(web::Json(ApiResponse {
        status: 0,
        data: ApiData::Data(TopicListActiveResponse { topic_ids }),
        message: ApiMsg::OK,
    }))
}

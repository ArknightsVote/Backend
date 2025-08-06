use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
use utoipa::OpenApi;

use crate::AppState;

mod audit_topic;
mod create_topic;
mod get_all_active_topics_ids;
mod get_need_audit_topics;
mod get_topic_info;
mod new_compare;
mod operators_1v1_matrix;
mod save_score;
mod utils;
mod view_final_order;

use audit_topic::*;
use create_topic::*;
use get_all_active_topics_ids::*;
use get_need_audit_topics::*;
use get_topic_info::*;
use new_compare::*;
use operators_1v1_matrix::*;
use save_score::*;
use view_final_order::*;

use share::models::api::{
    ApiMsg, CreateTopicRequest, CreateTopicResponse, GetAllTopicsIdsResponse,
    GetAuditTopicsResponse, GetTopicInfoRequest, GetTopicInfoResponse, NewCompareRequest,
    NewCompareResponse, Operators1v1MatrixResponse, SaveScoreRequest, SaveScoreResponse,
    ViewFinalOrderRequest, ViewFinalOrderResponse,
};

#[derive(OpenApi)]
#[openapi(
    paths(
        get_all_active_topics_ids,
        create_topic,
        get_topic_info,
        new_compare,
        operators_1v1_matrix,
        save_score,
        view_final_order,
        get_need_audit_topics,
        audit_topic
    ),
    components(schemas(
        GetAllTopicsIdsResponse,
        CreateTopicRequest,
        CreateTopicResponse,
        GetTopicInfoRequest,
        GetTopicInfoResponse,
        NewCompareRequest,
        NewCompareResponse,
        Operators1v1MatrixResponse,
        SaveScoreRequest,
        SaveScoreResponse,
        ViewFinalOrderRequest,
        ViewFinalOrderResponse,
        GetAuditTopicsResponse,
        ApiMsg
    ))
)]
pub struct ApiDoc;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/topics", get(get_all_active_topics_ids))
        .route("/topics/create", post(create_topic))
        .route("/topics/info", post(get_topic_info))
        .route("/topics/need_audit", get(get_need_audit_topics))
        .route("/topics/audit_topic", post(audit_topic))
        .route("/new_compare", post(new_compare))
        .route("/save_score", post(save_score))
        .route("/view_final_order", post(view_final_order))
        .route("/operators_1v1_matrix", post(operators_1v1_matrix))
}

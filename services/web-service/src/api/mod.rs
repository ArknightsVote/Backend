use std::{fmt, sync::Arc};

use axum::{
    Router,
    routing::{get, post},
};
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};

use crate::AppState;

mod create_topic;
mod get_all_active_topics_ids;
mod get_topic_info;
mod new_compare;
mod operators_1v1_matrix;
mod save_score;
mod utils;
mod view_final_order;

use create_topic::*;
use get_all_active_topics_ids::*;
use get_topic_info::*;
use new_compare::*;
use operators_1v1_matrix::*;
use save_score::*;
use view_final_order::*;

use share::models::api::{
    CreateTopicRequest, CreateTopicResponse, GetAllTopicsIdsResponse, GetTopicInfoRequest,
    GetTopicInfoResponse, NewCompareRequest, NewCompareResponse, Operators1v1MatrixResponse,
    SaveScoreRequest, SaveScoreResponse, ViewFinalOrderRequest, ViewFinalOrderResponse,
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
        view_final_order
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
        ApiMsg
    ))
)]
pub struct ApiDoc;

#[derive(Debug, Serialize, ToSchema)]
pub enum ApiMsg {
    OK,
    TargetTopicNotFound,
    TargetTopicNotActive,
    RequestTopicTypeMismatch,
    CurTopicNotSupportFinalOrder,
    CurTopicNotSupport1v1Matrix,
    InternalError,
    BallotWinnerCannotBeLoser,

    UnsupportedTopicType,
}

impl fmt::Display for ApiMsg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiMsg::OK => write!(f, "OK"),
            ApiMsg::TargetTopicNotFound => write!(f, "Target topic not found"),
            ApiMsg::TargetTopicNotActive => write!(f, "Target topic is not active"),
            ApiMsg::RequestTopicTypeMismatch => {
                write!(f, "Request topic type does not match topic type")
            }
            ApiMsg::CurTopicNotSupportFinalOrder => {
                write!(f, "Current topic type does not support final order")
            }
            ApiMsg::CurTopicNotSupport1v1Matrix => {
                write!(f, "Current topic type does not support 1v1 matrix")
            }
            ApiMsg::InternalError => write!(f, "Internal server error"),
            ApiMsg::BallotWinnerCannotBeLoser => write!(f, "Ballot winner cannot be loser"),

            ApiMsg::UnsupportedTopicType => write!(f, "Unsupported topic type"),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiResponse<T> {
    pub status: i32,
    pub data: Option<T>,
    pub message: ApiMsg,
}

impl Default for ApiResponse<()> {
    fn default() -> Self {
        Self {
            data: None,
            message: ApiMsg::OK,
            status: 0,
        }
    }
}

impl axum::response::IntoResponse for ApiResponse<()> {
    fn into_response(self) -> axum::response::Response {
        let status = match self.status {
            0 => axum::http::StatusCode::OK,
            404 => axum::http::StatusCode::NOT_FOUND,
            500 => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            _ if self.status < 500 => axum::http::StatusCode::BAD_REQUEST,
            _ => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, axum::Json(self)).into_response()
    }
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/topics", get(get_all_active_topics_ids))
        .route("/topics/create", post(create_topic))
        .route("/topics/info", post(get_topic_info))
        .route("/new_compare", post(new_compare))
        .route("/save_score", post(save_score))
        .route("/view_final_order", post(view_final_order))
        .route("/operators_1v1_matrix", post(operators_1v1_matrix))
}

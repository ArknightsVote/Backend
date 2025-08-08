use std::sync::Arc;

use axum::{Router, routing::post};

use crate::state::AppState;

pub mod topic_candidate_pool;
pub mod topic_create;
pub mod topic_info;
pub mod topic_list_active;

use topic_candidate_pool::topic_candidate_pool;
use topic_create::topic_create;
use topic_info::topic_info;
use topic_list_active::topic_list_active;

pub fn topic_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/list", post(topic_list_active)) // 获取所有活跃 topic
        .route("/create", post(topic_create)) // 创建新 topic
        .route("/info", post(topic_info)) // 获取 topic 详情
        .route("/candidate_pool", post(topic_candidate_pool)) // 获取候选池
}

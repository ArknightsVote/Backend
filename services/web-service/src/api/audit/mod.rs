use std::sync::Arc;

use axum::{Router, routing::post};

use crate::state::AppState;

pub mod audit_topic;
pub mod audit_topics_list;

use audit_topic::audit_topic;
use audit_topics_list::audit_topics_list;

pub fn audit_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/need_audit_topics", post(audit_topics_list))
        .route("/topic", post(audit_topic))
}

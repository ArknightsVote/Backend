use std::sync::Arc;

use axum::{Router, routing::post};

use crate::state::AppState;

pub mod ballot_create;
pub mod ballot_save;

use ballot_create::ballot_create;
use ballot_save::ballot_save;

pub fn ballot_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/new", post(ballot_create)) // 创建新 ballot
        .route("/save", post(ballot_save)) // 保存 ballot
}

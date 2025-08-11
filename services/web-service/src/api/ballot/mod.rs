use std::sync::Arc;

use axum::{Router, routing::post};

use crate::state::AppState;

pub mod ballot_bench_new;
pub mod ballot_bench_save;
pub mod ballot_create;
pub mod ballot_save;
pub mod ballot_skip;

use ballot_bench_new::ballot_bench_new;
use ballot_bench_save::ballot_bench_save;
use ballot_create::ballot_create;
use ballot_save::ballot_save;
use ballot_skip::ballot_skip;

pub fn ballot_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/new", post(ballot_create)) // 创建新 ballot
        .route("/save", post(ballot_save)) // 保存 ballot
        .route("/skip", post(ballot_skip)) // 跳过 ballot
        .route("/bench_new", post(ballot_bench_new))
        .route("/bench_save", post(ballot_bench_save))
}

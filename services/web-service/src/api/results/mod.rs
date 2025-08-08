use std::sync::Arc;

use axum::{Router, routing::post};

use crate::state::AppState;

pub mod results_1v1_matrix;
pub mod results_final_order;

use results_1v1_matrix::results_1v1_matrix;
use results_final_order::results_final_order;

pub fn results_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/1v1_matrix", post(results_1v1_matrix))
        .route("/final_order", post(results_final_order))
}

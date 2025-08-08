use std::sync::Arc;

use axum::Router;

use crate::AppState;

mod audit;
mod ballot;
mod openapi;
mod results;
mod topic;
mod utils;

use audit::audit_routes;
use ballot::ballot_routes;
use results::results_routes;
use topic::topic_routes;

pub use openapi::ApiDoc;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .nest("/topic", topic_routes())
        .nest("/ballot", ballot_routes())
        .nest("/audit", audit_routes())
        .nest("/results", results_routes())
}

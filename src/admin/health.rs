use std::sync::atomic::AtomicBool;

use std::panic;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

use axum::{
    extract::State,
    http::{Response, StatusCode},
};

use super::AdminState;

#[derive(Clone)]
pub struct Health {
    healthy: Arc<AtomicBool>,
    shutdown_tx: share::signal::ShutdownTx,
}

impl Health {
    pub fn new(shutdown_tx: share::signal::ShutdownTx) -> Self {
        let health = Self {
            healthy: Arc::new(AtomicBool::new(true)),
            shutdown_tx,
        };

        let healthy = health.healthy.clone();
        let shutdown_tx = health.shutdown_tx.clone();
        let default_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            tracing::error!(%panic_info, "panic has occurred. moving to Unhealthy");
            healthy.swap(false, Relaxed);
            let _ = shutdown_tx.send(share::signal::ShutdownKind::Normal);
            default_hook(panic_info);
        }));

        health
    }

    pub async fn check_liveness(&self) -> Response<String> {
        if self.healthy.load(Relaxed) {
            return Response::new("ok".into());
        };

        let mut response = Response::new("unhealthy".into());
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        response
    }
}

pub async fn check_health(State(state): State<AdminState>) -> Response<String> {
    state.health.check_liveness().await
}

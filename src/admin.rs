mod health;

use axum::{Router, routing::get};
use health::{Health, check_health};

pub const PORT: u16 = 8443;

#[derive(Clone)]
struct AdminState {
    health: Health,
}

pub fn server(
    shutdown_tx: share::signal::ShutdownTx,
    address: Option<std::net::SocketAddr>,
) -> std::thread::JoinHandle<Result<(), eyre::Error>> {
    let address = address.unwrap_or_else(|| (std::net::Ipv6Addr::UNSPECIFIED, PORT).into());
    let health = Health::new(shutdown_tx);
    tracing::info!(address = %address, "starting admin endpoint");

    std::thread::Builder::new()
        .name("admin-http".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_io()
                .enable_time()
                .thread_name("admin-http-worker")
                .build()
                .unwrap();

            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::bind(address).await?;

                let health = health.clone();

                let state = AdminState { health };

                let app = Router::new()
                    .route("/live", get(check_health))
                    .route("/livez", get(check_health))
                    .with_state(state);

                let http_task: tokio::task::JoinHandle<Result<(), eyre::Error>> =
                    tokio::task::spawn(async move {
                        axum::serve(listener, app.into_make_service())
                            .await
                            .map_err(eyre::Error::from)
                    });

                http_task.await?
            })
        })
        .expect("failed to spawn admin-http thread")
}

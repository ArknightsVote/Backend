mod health;

#[allow(unused_imports)]
use axum::{
    Router,
    body::Body,
    http::{self, Response},
    routing::get,
};
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

                // #[cfg(target_os = "linux")]
                // {
                //     app = app.route("/pprof", get(profile));
                // }

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

// #[allow(dead_code)]
// fn encode<M: prost::Message>(message: &M) -> Result<Vec<u8>, prost::EncodeError> {
//     let mut buf = Vec::with_capacity(message.encoded_len());
//     message.encode(&mut buf)?;
//     Ok(buf)
// }

// #[cfg(target_os = "linux")]
// async fn profile(request: axum::extract::Request<Body>) -> Response<Body> {
//     let duration = request.uri().query().and_then(|query| {
//         form_urlencoded::parse(query.as_bytes())
//             .find(|(k, _)| k == "seconds")
//             .and_then(|(_, v)| v.parse().ok())
//             .map(std::time::Duration::from_secs)
//     });

//     match collect_pprof(duration).await {
//         Ok(value) => value,
//         Err(error) => {
//             tracing::warn!(%error, "admin http server error");
//             Response::builder()
//                 .status(http::StatusCode::INTERNAL_SERVER_ERROR)
//                 .body(Body::from(axum::body::Bytes::from("internal error")))
//                 .unwrap()
//         }
//     }
// }

// #[cfg(target_os = "linux")]
// async fn collect_pprof(
//     duration: Option<std::time::Duration>,
// ) -> Result<Response<Body>, eyre::Error> {
//     let duration = duration.unwrap_or_else(|| std::time::Duration::from_secs(2));
//     tracing::debug!(duration_seconds = duration.as_secs(), "profiling");

//     let guard = pprof::ProfilerGuardBuilder::default()
//         .frequency(1000)
//         // From the pprof docs, this blocklist helps prevent deadlock with
//         // libgcc's unwind.
//         .blocklist(&["libc", "libgcc", "pthread", "vdso"])
//         .build()?;

//     tokio::time::sleep(duration).await;

//     let data = guard.report().build()?.pprof()?;
//     // let encoded_profile = encode(&guard.report().build()?.pprof()?)?;
//     let mut buf = Vec::with_capacity(data.encoded_len());
//     data.encode(&mut buf)?;

//     // gzip profile
//     let mut encoder = libflate::gzip::Encoder::new(Vec::new())?;
//     std::io::copy(&mut &buf[..], &mut encoder)?;
//     let gzip_body = encoder.finish().into_result()?;
//     tracing::debug!("profile encoded to gzip");

//     Response::builder()
//         .header(http::header::CONTENT_LENGTH, gzip_body.len() as u64)
//         .header(http::header::CONTENT_TYPE, "application/octet-stream")
//         .header(http::header::CONTENT_ENCODING, "gzip")
//         .body(Body::from(gzip_body))
//         .map_err(From::from)
// }

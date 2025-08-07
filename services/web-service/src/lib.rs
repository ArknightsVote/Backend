use std::{net::SocketAddr, sync::Arc, time::Duration};

mod api;
mod constants;
mod error;
mod service;
mod state;
mod utils;

use async_nats::jetstream;
use axum::{
    Router,
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
};
use axum_prometheus::PrometheusMetricLayer;
use eyre::Context;
use sentry::integrations::tower::{NewSentryLayer, SentryHttpLayer};
use share::{
    config::AppConfig,
    models::{database::VotingTopic, excel::CharacterInfo},
    snowflake::Snowflake,
};
use tera::Tera;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir, timeout::TimeoutLayer, trace::TraceLayer};
use utoipa::OpenApi as _;
use utoipa_scalar::{Scalar, Servable as _};
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    api::ApiDoc,
    constants::LUA_SCRIPT_GET_FINAL_ORDER,
    error::AppError,
    service::TopicService,
    state::{AppState, RedisService},
};

async fn page_handler(State(app_state): State<Arc<AppState>>) -> impl IntoResponse {
    let html = app_state
        .tera
        .render("page.html", &tera::Context::new())
        .unwrap();

    Html(html)
}

pub struct WebService {
    config: AppConfig,
}

impl WebService {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub async fn run(self, mut shutdown_rx: share::signal::ShutdownRx) -> eyre::Result<()> {
        let nats_client = async_nats::connect(&self.config.nats.url)
            .await
            .context("failed to connect to nats")?;
        let jetstream = jetstream::new(nats_client.clone());
        tracing::debug!("connected to nats at {}", &self.config.nats.url);

        let redis_client = redis::Client::open(&*self.config.database.redis_url)
            .context("failed to create Redis client")?;
        let connection = redis_client.get_multiplexed_async_connection().await?;
        tracing::debug!("connected to redis at {}", &self.config.database.redis_url);

        let mongodb_client = mongodb::Client::with_uri_str(&self.config.database.mongodb_url)
            .await
            .context("failed to connect to MongoDB")?;
        let mongodb = mongodb_client.database(&self.config.database.mongodb_database);
        tracing::debug!(
            "connected to mongodb at {}, database: {}",
            &self.config.database.mongodb_url,
            &self.config.database.mongodb_database
        );
        use mongodb::bson::doc;
        let collection = mongodb.collection::<VotingTopic>("topics");

        for preset_topic in &self.config.vote.preset_vote_topic {
            let filter = doc! { "id": &preset_topic.id };

            match collection.find_one(filter).await {
                Ok(Some(_)) => {
                    let query = doc! { "id": &preset_topic.id };
                    collection.replace_one(query, preset_topic).await?;
                    tracing::info!("updated preset voting topic: {}", preset_topic.id);
                }
                Ok(None) | Err(_) => {
                    tracing::info!("inserting preset voting topic: {}", preset_topic.id);
                    collection.insert_one(preset_topic).await?;
                }
            }
        }

        let snowflake = Snowflake::new_from_config(&self.config.snowflake);
        tracing::debug!(
            "snowflake initialized with config: {:?}",
            &self.config.snowflake
        );

        let character_infos: Vec<CharacterInfo> = utils::load_character_table()?
            .into_iter()
            .filter_map(|(name, data)| {
                if !name.starts_with("char_") {
                    return None;
                }
                let mut parts = name["char_".len()..].splitn(2, '_');
                let charid = parts.next()?.parse::<i32>().ok()?;
                Some(CharacterInfo {
                    id: charid,
                    name: data.name,
                    rarity: data.rarity,
                    profession: data.profession,
                    sub_profession_id: data.sub_profession_id,
                })
            })
            .collect();

        let tera = Tera::new("templates/**/*").context("failed to load Tera templates")?;
        tracing::debug!("Tera templates loaded");

        let topic_service = TopicService::new(mongodb.clone());
        tracing::debug!("TopicService initialized");

        let state = AppState {
            jetstream,
            redis: RedisService {
                _client: redis_client,
                connection,
                final_order_script: redis::Script::new(LUA_SCRIPT_GET_FINAL_ORDER),
            },
            _mongodb: mongodb,
            snowflake,
            character_infos,
            tera,

            topic_service,
        };
        tracing::debug!("AppState initialized");

        let sentry_layer = ServiceBuilder::new()
            .layer(NewSentryLayer::new_from_top())
            .layer(SentryHttpLayer::new().enable_transaction());
        tracing::debug!("Sentry layer initialized");

        let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();
        tracing::debug!("Prometheus metrics layer initialized");

        let cors_layer = {
            let allow_methods = self
                .config
                .cors
                .allow_methods
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect::<Vec<_>>();

            let cors_builder = CorsLayer::new()
                .allow_methods(allow_methods)
                .allow_headers(tower_http::cors::Any)
                .max_age(Duration::from_secs(3600));

            match self.config.cors.allow_origin.as_slice() {
                [single] if single == "*" => cors_builder.allow_origin(tower_http::cors::Any),
                origins => {
                    let allow_origin = origins
                        .iter()
                        .filter_map(|s| s.parse().ok())
                        .collect::<Vec<_>>();
                    cors_builder.allow_origin(allow_origin)
                }
            }
        };
        tracing::debug!("CORS layer initialized");

        let app = Router::new()
            .route("/", get(page_handler))
            .route("/metrics", get(|| async move { metric_handle.render() }))
            .merge(api::routes())
            .nest_service("/static", ServeDir::new("static"))
            .merge(SwaggerUi::new("/docs").url("/api-doc/openapi.json", ApiDoc::openapi()))
            .merge(Scalar::with_url("/scalar", ApiDoc::openapi()))
            .with_state(Arc::new(state))
            .layer(cors_layer)
            .layer(sentry_layer)
            .layer((
                TraceLayer::new_for_http(),
                TimeoutLayer::new(Duration::from_secs(60)),
            ))
            .layer(prometheus_layer);
        tracing::debug!("Router initialized");

        let bind_addr = self
            .config
            .server
            .address()
            .parse::<SocketAddr>()
            .context("invalid bind address")?;

        tracing::info!("starting web service on {}", bind_addr);

        let listener = tokio::net::TcpListener::bind(bind_addr).await?;

        tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .expect("failed to start web service");
        });

        tracing::info!("web service started successfully");
        shutdown_rx.changed().await?;

        tracing::info!("shutting down web service");
        nats_client.drain().await?;

        Ok(())
    }
}

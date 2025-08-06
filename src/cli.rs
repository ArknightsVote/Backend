use std::fmt;

use clap::crate_version;
use git_testament::{git_testament, render_testament};
use share::config::AppConfig;
use share::{config::TomlConfig as _, tracing::init_tracing_subscriber};

use crate::admin;

git_testament!(TESTAMENT);

#[derive(Debug, clap::Parser)]
#[command(next_help_heading = "Administration Options")]
pub struct AdminCli {
    #[arg(
        long = "admin.enabled",
        env = "ARK_VOTE_ADMIN_ENABLED",
        value_name = "BOOL",
        num_args(0..=1),
        action=clap::ArgAction::Set,
        default_missing_value = "true",
        default_value_t = true
    )]
    enabled: bool,
    #[clap(long = "admin.address", env = "ARK_VOTE_ADMIN_ADDRESS")]
    pub address: Option<std::net::SocketAddr>,
}

#[derive(Debug, clap::Parser)]
#[command(version)]
#[non_exhaustive]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Option<Commands>,
    #[command(flatten)]
    pub admin: AdminCli,
}

#[derive(Clone, Debug, clap::Subcommand)]
pub enum Commands {
    WebServer,
    NatsConsumer,
    ServiceTest,
}

impl fmt::Display for Commands {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Commands::WebServer => write!(f, "web-server"),
            Commands::NatsConsumer => write!(f, "nats-consumer"),
            Commands::ServiceTest => write!(f, "service-test"),
        }
    }
}

impl Cli {
    pub async fn drive(self) -> Result<(), eyre::Error> {
        let config: AppConfig = AppConfig::load_or_create("config/app.toml");
        let service_name = self
            .command
            .as_ref()
            .map(|cmd| format!("{cmd}"))
            .unwrap_or_else(|| "ark_vote".to_string());

        let guard = init_tracing_subscriber(
            &config.tracing.level,
            &config.tracing.log_file_directory,
            &service_name,
        );
        std::mem::forget(guard);

        tracing::info!(
            version = crate_version!(),
            commit = option_env!("GIT_COMMIT_HASH"),
            "starting ark-vote cli application"
        );

        let version = render_testament!(TESTAMENT).leak();

        if !config.sentry.dsn.is_empty() {
            tracing::info!("sentry DSN is set, initializing Sentry");
            let dsn = config.sentry.dsn.clone();
            let _guard = sentry::init((
                dsn,
                sentry::ClientOptions {
                    release: Some(std::borrow::Cow::Borrowed(version)),
                    traces_sample_rate: 1.0,
                    ..Default::default()
                },
            ));
        }

        if matches!(self.command, Some(Commands::ServiceTest)) {
            return service_test::ServiceTester::new(config).run().await;
        }

        let (shutdown_tx, shutdown_rx) = share::signal::spawn_handler();
        if self.admin.enabled {
            admin::server(shutdown_tx, self.admin.address);
        }

        match self.command {
            Some(Commands::WebServer) => {
                tracing::info!("starting web server");

                web_service::WebService::new(config).run(shutdown_rx).await
            }
            Some(Commands::NatsConsumer) => {
                tracing::info!("starting nats consumer");

                nats_service::NatsService::new(config)
                    .run(shutdown_rx)
                    .await
            }
            None => {
                tracing::error!("no command specified, shutting down");

                Ok(())
            }
            Some(_) => unreachable!(),
        }
    }
}

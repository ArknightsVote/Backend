use std::io::IsTerminal as _;

use chrono::{DateTime, Utc};
use chrono_tz::Asia::Shanghai;
use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{
    fmt::{self, time::FormatTime},
    layer::SubscriberExt as _,
    util::SubscriberInitExt as _,
};

use crate::config::TracingConfig;

struct East8Time;

impl FormatTime for East8Time {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        let utc: DateTime<Utc> = Utc::now();
        let shanghai_time = utc.with_timezone(&Shanghai);
        write!(
            w,
            "{}",
            shanghai_time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        )
    }
}

pub fn init_tracing_subscriber(
    trace_config: &TracingConfig,
    log_target_service: &str,
) -> WorkerGuard {
    let mut env_filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(trace_config.level.parse().unwrap_or_else(|_| {
            tracing::error!("invalid log level in config, defaulting to DEBUG");
            tracing::Level::DEBUG.into()
        }))
        .from_env_lossy();

    for directive in &trace_config.directives {
        match directive.parse() {
            Ok(d) => {
                env_filter = env_filter.add_directive(d);
            }
            Err(e) => {
                tracing::error!("Failed to parse directive '{}': {}", directive, e);
            }
        }
    }

    let is_terminal = std::io::stdout().is_terminal();
    let terminal_layer = fmt::layer()
        .with_ansi(is_terminal)
        .with_target(false)
        .with_timer(East8Time);

    let file_appender = rolling::daily(
        &trace_config.log_file_directory,
        format!("{log_target_service}.log"),
    );
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = fmt::layer()
        .json()
        .with_ansi(false)
        .with_target(true)
        .with_writer(non_blocking)
        .with_timer(East8Time);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(terminal_layer)
        .with(file_layer)
        .with(sentry::integrations::tracing::layer())
        .init();

    guard
}

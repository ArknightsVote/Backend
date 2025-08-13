use std::path::Path;

use async_nats::jetstream::stream::{RetentionPolicy, StorageType};
use serde::{Deserialize, de::DeserializeOwned};

use crate::{models::database::VotingTopic, snowflake::SnowflakeConfig};

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub vote: VoteConfig,
    pub cors: CorsConfig,
    pub database: DatabaseConfig,
    pub sentry: SentryConfig,
    pub tracing: TracingConfig,
    pub snowflake: SnowflakeConfig,
    pub nats: NatsConfig,
    pub test: TestConfig,
    pub task_manager: TaskManagerConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl ServerConfig {
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct VoteConfig {
    pub base_multiplier: i32,
    pub low_multiplier: i32,
    pub max_ip_limit: i32,
    pub ip_counter_expire_seconds: usize,

    pub preset_vote_topic: Vec<VotingTopic>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CorsConfig {
    pub allow_origin: Vec<String>,
    pub allow_methods: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatabaseConfig {
    pub redis_url: String,
    pub mongodb_url: String,
    pub mongodb_database: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SentryConfig {
    pub dsn: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TracingConfig {
    pub level: String,
    pub log_file_directory: String,
    pub directives: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NatsConfig {
    pub url: String,
    pub consumers: Vec<NatsConsumerConfig>,
    pub stream_config: NatsStreamConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NatsConsumerConfig {
    pub name: String,
    pub enabled: bool,
    pub subject: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NatsStreamConfig {
    pub name: String,
    pub storage_type: StorageType,
    pub subjects: Vec<String>,
    pub retention: RetentionPolicy,
    pub max_messages: i64,
    pub max_messages_per_subject: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TestConfig {
    pub base_url: String,
    pub total_requests: usize,
    pub concurrency_limit: usize,
    pub max_retry: usize,
    pub qps_limit: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TaskManagerConfig {
    pub concurrency: usize,
}

impl TomlConfig for AppConfig {
    const DEFAULT_TOML: &str = include_str!("../app.default.toml");
}

pub trait TomlConfig: DeserializeOwned {
    const DEFAULT_TOML: &str;

    fn load_or_create<T: DeserializeOwned>(path: &str) -> T {
        let path = Path::new(path);

        std::fs::read_to_string(path).map_or_else(
            |_| {
                path.parent()
                    .inspect(|parent| std::fs::create_dir_all(parent).unwrap());

                std::fs::write(path, Self::DEFAULT_TOML).unwrap();
                toml::from_str(Self::DEFAULT_TOML).unwrap_or_else(|err| {
                    panic!(
                        "failed to parse defaults for configuration file {}: {}",
                        path.display(),
                        err
                    )
                })
            },
            |data| {
                toml::from_str(&data).unwrap_or_else(|err| {
                    panic!(
                        "failed to parse configuration file {}: {}",
                        path.display(),
                        err
                    )
                })
            },
        )
    }
}

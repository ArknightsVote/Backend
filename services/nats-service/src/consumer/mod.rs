mod ballot_skip;
mod dlq;
mod save_score;

use std::{borrow::Cow, collections::HashMap, pin::Pin, sync::Arc};

use ballot_skip::ballot_skip_consumer;
use dlq::dlq_consumer;
use save_score::save_score_consumer;
use share::config::AppConfig;

use crate::db::AppDatabase;

fn normalize_subject(subject: &str) -> String {
    subject.replace('.', "-").replace("_", "-")
}

type ConsumerStarter =
    fn(
        filter_subject: Cow<'static, str>,
        stream: async_nats::jetstream::stream::Stream,
        database: Arc<AppDatabase>,
        app_config: Arc<AppConfig>,
    ) -> Pin<Box<dyn futures::Future<Output = eyre::Result<()>> + Send + 'static>>;

#[derive(Debug)]
pub struct ConsumerConfig {
    pub name: &'static str,
    pub starter: ConsumerStarter,
}

pub fn available_consumers() -> HashMap<&'static str, ConsumerConfig> {
    macro_rules! consumer {
        ($name:literal, $func:ident) => {
            (
                $name,
                ConsumerConfig {
                    name: $name,
                    starter: |subject, stream, db, vote_config| {
                        Box::pin(async move {
                            if let Err(e) = $func(subject, stream, db, vote_config).await {
                                tracing::error!("{} failed: {}", $name, e);
                            }
                            Ok(())
                        })
                    },
                },
            )
        };
    }

    HashMap::from([
        consumer!("ballot_skip", ballot_skip_consumer),
        consumer!("save_score", save_score_consumer),
        consumer!("dlq", dlq_consumer),
    ])
}

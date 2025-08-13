use std::sync::Arc;

use redis::aio::MultiplexedConnection;
use tokio::sync::Mutex;

pub struct WorkerIdManager {
    connection: MultiplexedConnection,
    worker_id: Arc<Mutex<Option<u8>>>,
    max_worker_id: u8,
}

impl WorkerIdManager {
    pub fn new(connection: MultiplexedConnection, max_worker_id: u8) -> eyre::Result<Self> {
        Ok(Self {
            connection,
            worker_id: Arc::new(Mutex::new(None)),
            max_worker_id,
        })
    }

    pub async fn acquire(&self) -> eyre::Result<u8> {
        for id in 0..=self.max_worker_id {
            let key = format!("snowflake:worker:{}", id);
            let result: Option<String> = redis::cmd("SET")
                .arg(&key)
                .arg("locked")
                .arg("NX")
                .arg("EX")
                .arg(60)
                .query_async(&mut self.connection.clone())
                .await?;

            if result.is_some() {
                *self.worker_id.lock().await = Some(id);
                tracing::debug!("Acquired worker_id = {}", id);
                return Ok(id);
            }
        }
        eyre::bail!("No available worker_id");
    }

    pub async fn keep_alive(self: Arc<Self>) {
        let worker_id = self.worker_id.clone();
        let mut connection = self.connection.clone();

        tokio::spawn(async move {
            loop {
                if let Some(id) = *worker_id.lock().await {
                    let key = format!("snowflake:worker:{}", id);
                    let _: () = redis::cmd("EXPIRE")
                        .arg(&key)
                        .arg(60)
                        .query_async(&mut connection)
                        .await
                        .unwrap_or(());
                }
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        });
    }
}

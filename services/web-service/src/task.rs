use std::{future::Future, pin::Pin, sync::Arc};
use serde::Serialize;
use tokio::sync::{mpsc, Mutex, Semaphore};
use tracing::error;
use futures::FutureExt;
use std::panic::AssertUnwindSafe;

type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
type Task = Box<dyn FnOnce() -> BoxFuture + Send + 'static>;

#[derive(Debug, Clone, Serialize)]
pub struct TaskStats {
    pub queued: usize,
    pub running: usize,
    pub completed: usize,
}

impl Default for TaskStats {
    fn default() -> Self {
        Self {
            queued: 0,
            running: 0,
            completed: 0,
        }
    }
}

pub struct TaskManager {
    sender: mpsc::UnboundedSender<Task>,
    stats: Arc<Mutex<TaskStats>>,
    concurrency: usize,
}

impl TaskManager {
    pub fn new(concurrency: usize) -> Arc<Self> {
        let (tx, mut rx) = mpsc::unbounded_channel::<Task>();
        let stats = Arc::new(Mutex::new(TaskStats::default()));
        let stats_for_dispatch = stats.clone();
        let semaphore = Arc::new(Semaphore::new(concurrency));

        tokio::spawn({
            let semaphore = semaphore.clone();
            async move {
                while let Some(task) = rx.recv().await {
                    {
                        let mut s = stats_for_dispatch.lock().await;
                        s.queued = s.queued.saturating_sub(1);
                    }

                    let permit = semaphore.clone().acquire_owned().await.unwrap();

                    let stats_worker = stats_for_dispatch.clone();

                    tokio::spawn(async move {
                        {
                            let mut s = stats_worker.lock().await;
                            s.running += 1;
                        }

                        let fut = (task)();

                        let res = AssertUnwindSafe(fut).catch_unwind().await;

                        match res {
                            Ok(_) => {
                                // completed normally
                            }
                            Err(e) => {
                                error!("Background task panicked: {:?}", e);
                            }
                        }

                        {
                            let mut s = stats_worker.lock().await;
                            s.running = s.running.saturating_sub(1);
                            s.completed += 1;
                        }

                        drop(permit);
                    });
                }

                tracing::info!("Task dispatcher loop ended (receiver closed)");
            }
        });

        Arc::new(Self {
            sender: tx,
            stats,
            concurrency,
        })
    }

    pub async fn spawn<Fut, F>(&self, f: F)
    where
        Fut: Future<Output = ()> + Send + 'static,
        F: FnOnce() -> Fut + Send + 'static,
    {
        {
            let mut s = self.stats.lock().await;
            s.queued = s.queued.saturating_add(1);
        }

        let task: Task = Box::new(move || Box::pin(f()) as BoxFuture);

        if let Err(e) = self.sender.send(task) {
            error!("Failed to enqueue background task (receiver closed): {:?}", e);
            let mut s = self.stats.lock().await;
            s.queued = s.queued.saturating_sub(1);
        }
    }

    pub async fn get_stats(&self) -> TaskStats {
        self.stats.lock().await.clone()
    }

    pub fn concurrency(&self) -> usize {
        self.concurrency
    }
}

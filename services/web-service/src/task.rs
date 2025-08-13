use futures::FutureExt;
use serde::Serialize;
use std::{
    future::Future,
    panic::AssertUnwindSafe,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::sync::{Semaphore, mpsc};
use tracing::error;

type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
type Task = Box<dyn FnOnce() -> BoxFuture + Send + 'static>;

#[derive(Default, Debug, Clone, Serialize)]
pub struct TaskStats {
    pub queued: usize,
    pub running: usize,
    pub completed: usize,
}

pub struct TaskManager {
    sender: mpsc::UnboundedSender<Task>,
    queued: AtomicUsize,
    running: AtomicUsize,
    completed: AtomicUsize,
    concurrency: usize,
}

impl TaskManager {
    pub fn new(concurrency: usize) -> Arc<Self> {
        let (tx, mut rx) = mpsc::unbounded_channel::<Task>();

        let manager = Arc::new(Self {
            sender: tx,
            queued: AtomicUsize::new(0),
            running: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            concurrency,
        });

        let stats_for_dispatch = manager.clone();
        let semaphore = Arc::new(Semaphore::new(concurrency));

        tokio::spawn({
            let semaphore = semaphore.clone();
            async move {
                while let Some(task) = rx.recv().await {
                    stats_for_dispatch.queued.fetch_sub(1, Ordering::Relaxed);

                    let permit = semaphore.clone().acquire_owned().await.unwrap();

                    stats_for_dispatch.running.fetch_add(1, Ordering::Relaxed);
                    let stats_worker = stats_for_dispatch.clone();

                    tokio::spawn(async move {
                        let fut = (task)();

                        let res = AssertUnwindSafe(fut).catch_unwind().await;

                        if let Err(e) = res {
                            error!("Background task panicked: {:?}", e);
                        }

                        stats_worker.running.fetch_sub(1, Ordering::Relaxed);
                        stats_worker.completed.fetch_add(1, Ordering::Relaxed);

                        drop(permit);
                    });
                }

                tracing::info!("Task dispatcher ended (receiver closed).");
            }
        });

        manager
    }

    pub fn spawn<Fut, F>(&self, f: F)
    where
        Fut: Future<Output = ()> + Send + 'static,
        F: FnOnce() -> Fut + Send + 'static,
    {
        self.queued.fetch_add(1, Ordering::Relaxed);

        let task: Task = Box::new(move || Box::pin(f()) as BoxFuture);

        if let Err(e) = self.sender.send(task) {
            self.queued.fetch_sub(1, Ordering::Relaxed);
            error!(
                "Failed to enqueue background task (receiver closed): {:?}",
                e
            );
        }
    }

    pub fn get_stats(&self) -> TaskStats {
        TaskStats {
            queued: self.queued.load(Ordering::Relaxed),
            running: self.running.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
        }
    }

    pub fn concurrency(&self) -> usize {
        self.concurrency
    }
}

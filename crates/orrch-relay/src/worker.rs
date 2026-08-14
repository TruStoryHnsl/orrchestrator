//! The single serializing worker. Owns the scheduler; one in-flight request at
//! a time. Submitting wakes it; it picks the next id, runs the engine, and
//! pumps tokens into that request's channel. Shared as Arc<Self>.
use crate::clock::Clock;
use crate::engine::Engine;
use crate::scheduler::{EnqueueError, Scheduler};
use crate::types::{AffinityDescriptor, QueuedRequest, TokenEvent};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

pub struct Worker<C: Clock + 'static> {
    sched: Arc<Mutex<Scheduler<C>>>,
    engine: Arc<dyn Engine>,
    pending: Mutex<HashMap<u64, QueuedRequest>>,
    notify: Notify,
}

pub struct WorkerHandle {
    shutdown: Arc<Notify>,
    join: tokio::task::JoinHandle<()>,
}
impl WorkerHandle {
    pub async fn shutdown(self) {
        self.shutdown.notify_one();
        let _ = self.join.await;
    }
}

impl<C: Clock + 'static> Worker<C> {
    pub fn new(sched: Arc<Mutex<Scheduler<C>>>, engine: Arc<dyn Engine>) -> Self {
        Self {
            sched,
            engine,
            pending: Mutex::new(HashMap::new()),
            notify: Notify::new(),
        }
    }

    /// Enqueue a request with its descriptor. Errors if the queue is full.
    pub async fn submit(
        &self,
        qr: QueuedRequest,
        desc: AffinityDescriptor,
    ) -> Result<(), EnqueueError> {
        let id = qr.id;
        self.pending.lock().await.insert(id, qr);
        if let Err(e) = self.sched.lock().await.enqueue(id, desc) {
            self.pending.lock().await.remove(&id);
            return Err(e);
        }
        self.notify.notify_one();
        Ok(())
    }

    /// Spawn the run loop. Returns a handle whose `shutdown()` stops it.
    pub fn spawn(self: Arc<Self>) -> WorkerHandle {
        // A dedicated Arc<Notify> for shutdown so the handle and the loop share it.
        let shutdown = Arc::new(Notify::new());
        let loop_shutdown = shutdown.clone();
        let me = self.clone();
        let join = tokio::spawn(async move { me.run(loop_shutdown).await });
        WorkerHandle { shutdown, join }
    }

    async fn run(self: Arc<Self>, shutdown: Arc<Notify>) {
        loop {
            let next_id = {
                let mut s = self.sched.lock().await;
                s.next()
            };
            let Some(id) = next_id else {
                tokio::select! {
                    _ = self.notify.notified() => continue,
                    _ = shutdown.notified() => return,
                }
            };
            let Some(qr) = self.pending.lock().await.remove(&id) else {
                continue;
            };
            match self.engine.complete(&qr.request).await {
                Ok(mut stream) => {
                    while let Some(ev) = stream.next().await {
                        let done = matches!(ev, TokenEvent::Done | TokenEvent::Error(_));
                        if qr.tx.send(ev).await.is_err() {
                            break; // client disconnected
                        }
                        if done {
                            break;
                        }
                    }
                }
                Err(e) => {
                    let _ = qr.tx.send(TokenEvent::Error(e.to_string())).await;
                }
            }
        }
    }
}

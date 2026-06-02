//! In-process ordering proof: interleaved A/B requests reach the engine grouped
//! by affinity cluster, shown by the order MockEngine records. No real network.
use orrch_relay::clock::SystemClock;
use orrch_relay::engine::MockEngine;
use orrch_relay::scheduler::{Scheduler, SchedulerPolicy};
use orrch_relay::types::{AffinityDescriptor, ChatMessage, CompletionRequest, QueuedRequest};
use orrch_relay::worker::Worker;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

fn req(model: &str) -> CompletionRequest {
    CompletionRequest {
        model: model.into(),
        messages: vec![ChatMessage { role: "user".into(), content: "x".into() }],
        stream: false,
        affinity_hint: None,
        extra: serde_json::Map::new(),
    }
}

#[tokio::test]
async fn interleaved_requests_dispatch_in_cluster_order() {
    let policy = SchedulerPolicy { max_wait_ms: 60_000, similarity_threshold: 0.8, max_queue_depth: 100 };
    let sched = Arc::new(Mutex::new(Scheduler::new(policy, SystemClock)));
    let engine = Arc::new(MockEngine::new(vec!["ok".into()]));
    let worker = Arc::new(Worker::new(sched, engine.clone()));

    let a = || AffinityDescriptor::Vector(vec![1.0, 0.0]);
    let b = || AffinityDescriptor::Vector(vec![0.0, 1.0]);
    let specs = [("a0", a()), ("b1", b()), ("a2", a()), ("b3", b()), ("a4", a())];

    // Pre-enqueue ALL requests before the worker starts → deterministic order.
    let mut receivers = vec![];
    for (i, (model, desc)) in specs.into_iter().enumerate() {
        let (tx, rx) = mpsc::channel(8);
        worker
            .submit(QueuedRequest { id: i as u64, request: req(model), tx }, desc)
            .await
            .unwrap();
        receivers.push(rx);
    }

    // Now run the worker and drain every response channel to completion.
    let handle = worker.clone().spawn();
    let mut drains = vec![];
    for mut rx in receivers {
        drains.push(tokio::spawn(async move { while rx.recv().await.is_some() {} }));
    }
    for d in drains { let _ = d.await; }
    handle.shutdown().await;

    let order = engine.received_models();
    assert_eq!(order.len(), 5, "all five dispatched: {order:?}");
    assert!(order.iter().take(3).all(|m| m.starts_with('a')), "A-cluster first: {order:?}");
    assert!(order[3..].iter().all(|m| m.starts_with('b')), "B-cluster last: {order:?}");
}

//! Affinity-ordering queue. Pure logic over (descriptors + policy + clock):
//! groups similar requests contiguously to keep the engine cache hot, while
//! aging guarantees no request starves past `max_wait_ms`.
use crate::affinity::cosine;
use crate::clock::Clock;
use crate::types::AffinityDescriptor;

#[derive(Debug, Clone)]
pub struct SchedulerPolicy {
    pub max_wait_ms: u64,
    pub similarity_threshold: f32,
    pub max_queue_depth: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EnqueueError {
    Full,
}

struct Entry {
    id: u64,
    descriptor: AffinityDescriptor,
    enqueued_ms: u64,
}

pub struct Scheduler<C: Clock> {
    policy: SchedulerPolicy,
    clock: C,
    queue: Vec<Entry>,
    last: Option<AffinityDescriptor>,
}

impl<C: Clock> Scheduler<C> {
    pub fn new(policy: SchedulerPolicy, clock: C) -> Self {
        Self { policy, clock, queue: Vec::new(), last: None }
    }
    pub fn len(&self) -> usize { self.queue.len() }
    pub fn is_empty(&self) -> bool { self.queue.is_empty() }

    pub fn enqueue(&mut self, id: u64, descriptor: AffinityDescriptor) -> Result<(), EnqueueError> {
        if self.queue.len() >= self.policy.max_queue_depth {
            return Err(EnqueueError::Full);
        }
        self.queue.push(Entry { id, descriptor, enqueued_ms: self.clock.now_ms() });
        Ok(())
    }

    /// Pick the next id: (1) any request older than max_wait → oldest such
    /// (anti-starve); (2) else best affinity to last cluster above threshold;
    /// (3) else oldest (start a new cluster).
    pub fn next(&mut self) -> Option<u64> {
        if self.queue.is_empty() { return None; }
        let now = self.clock.now_ms();

        let aged = self.queue.iter().enumerate()
            .filter(|(_, e)| now.saturating_sub(e.enqueued_ms) >= self.policy.max_wait_ms)
            .min_by_key(|(_, e)| e.enqueued_ms)
            .map(|(i, _)| i);

        let idx = if let Some(i) = aged {
            i
        } else if let Some(last) = &self.last {
            let best = self.queue.iter().enumerate()
                .map(|(i, e)| (i, affinity(last, &e.descriptor)))
                .filter(|(_, sim)| *sim >= self.policy.similarity_threshold)
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                    // tie-break: prefer earlier enqueue index (oldest first)
                    .then(b.0.cmp(&a.0)))
                .map(|(i, _)| i);
            best.unwrap_or_else(|| oldest_idx(&self.queue))
        } else {
            oldest_idx(&self.queue)
        };

        let entry = self.queue.remove(idx);
        self.last = Some(entry.descriptor);
        Some(entry.id)
    }
}

fn oldest_idx(q: &[Entry]) -> usize {
    q.iter().enumerate().min_by_key(|(_, e)| e.enqueued_ms).map(|(i, _)| i).unwrap_or(0)
}

fn affinity(a: &AffinityDescriptor, b: &AffinityDescriptor) -> f32 {
    match (a, b) {
        (AffinityDescriptor::Vector(x), AffinityDescriptor::Vector(y)) => cosine(x, y),
        (AffinityDescriptor::Tag(x), AffinityDescriptor::Tag(y)) if x == y => 1.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FakeClock;
    use crate::types::AffinityDescriptor;

    fn policy() -> SchedulerPolicy {
        SchedulerPolicy { max_wait_ms: 1000, similarity_threshold: 0.8, max_queue_depth: 100 }
    }

    #[test]
    fn groups_similar_requests_contiguously() {
        let clock = FakeClock::new();
        let mut s = Scheduler::new(policy(), clock);
        let a = || AffinityDescriptor::Vector(vec![1.0, 0.0]);
        let b = || AffinityDescriptor::Vector(vec![0.0, 1.0]);
        s.enqueue(0, a()).unwrap();
        s.enqueue(1, b()).unwrap();
        s.enqueue(2, a()).unwrap();
        s.enqueue(3, b()).unwrap();
        s.enqueue(4, a()).unwrap();
        let mut order = vec![];
        while let Some(id) = s.next() { order.push(id); }
        assert_eq!(order[0], 0);
        let a_ids: Vec<u64> = order.iter().take(3).copied().collect();
        assert_eq!(a_ids, vec![0, 2, 4], "all A-cluster before B-cluster");
        assert_eq!(&order[3..], &[1, 3], "B-cluster last");
    }

    #[test]
    fn aging_prevents_starvation() {
        let clock = FakeClock::new();
        let mut s = Scheduler::new(policy(), clock.clone());
        let a = || AffinityDescriptor::Vector(vec![1.0, 0.0]);
        let odd = AffinityDescriptor::Vector(vec![0.0, 1.0]);
        s.enqueue(0, a()).unwrap();
        s.enqueue(99, odd).unwrap();
        assert_eq!(s.next(), Some(0));
        s.enqueue(1, a()).unwrap();
        assert_eq!(s.next(), Some(1));
        s.enqueue(2, a()).unwrap();
        clock.advance(1001);
        assert_eq!(s.next(), Some(99), "aged request preempts hot cluster");
    }

    #[test]
    fn rejects_when_full() {
        let clock = FakeClock::new();
        let mut p = policy();
        p.max_queue_depth = 1;
        let mut s = Scheduler::new(p, clock);
        s.enqueue(0, AffinityDescriptor::None).unwrap();
        assert!(matches!(s.enqueue(1, AffinityDescriptor::None), Err(EnqueueError::Full)));
    }
}

//! Real-time synthetic traffic factory.
//!
//! The data factory continuously manufactures realistic inference requests for
//! an organization — a blend of benign business prompts and adversarial probes
//! derived from the org's own policy — and runs them through the live engine.
//! Each verdict is broadcast on the telemetry channel and recorded in the
//! capture corpus, so an observer watching the dashboard sees the system
//! actively safeguard the configured criteria in real time.
//!
//! Generation runs off the request path entirely; it shares the same evaluator
//! and telemetry bus the real proxy uses.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::broadcast;
use tracing::info;

use crate::engine::shell::CorpusStore;
use crate::engine::TrajectoryEngine;
use crate::policy::PolicyStore;
use crate::types::{
    IncomingPayload, OrgId, RequestId, TelemetryEvent, TraceVerdict,
};

use crate::engine::sectors::{sector_prompt_pool, Sector};

/// Per-organization factory state.
struct FactoryState {
    running: Arc<AtomicBool>,
    rate: Arc<AtomicU64>,
    emitted: Arc<AtomicU64>,
    blocked: Arc<AtomicU64>,
    #[allow(dead_code)]
    sector: Sector,
}

/// Status snapshot for the API.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FactoryStatus {
    pub running: bool,
    pub rate_per_sec: u64,
    pub emitted: u64,
    pub blocked: u64,
    pub sector: String,
}

/// Cloneable controller for per-org synthetic traffic factories.
#[derive(Clone)]
pub struct FactoryControl {
    inner: Arc<DashMap<OrgId, FactoryState>>,
    sectors: Arc<DashMap<OrgId, Sector>>,
    store: PolicyStore,
    telemetry_tx: broadcast::Sender<TelemetryEvent>,
    corpus: CorpusStore,
}

impl FactoryControl {
    /// Build a controller wired to the shared engine resources.
    pub fn new(
        store: PolicyStore,
        telemetry_tx: broadcast::Sender<TelemetryEvent>,
        corpus: CorpusStore,
    ) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            sectors: Arc::new(DashMap::new()),
            store,
            telemetry_tx,
            corpus,
        }
    }

    /// Set or change the sector for an organization.
    pub fn set_sector(&self, org: OrgId, sector: Sector) {
        self.sectors.insert(org, sector);
    }

    /// Get the current sector for an organization.
    pub fn get_sector(&self, org: OrgId) -> Sector {
        self.sectors.get(&org).map(|s| *s).unwrap_or(Sector::Generic)
    }

    /// Start (or re-rate) the factory for an organization.
    pub fn start(&self, org: OrgId, rate_per_sec: u64, sector: Option<Sector>) {
        let rate = rate_per_sec.clamp(1, 100);
        let sector = sector.unwrap_or_else(|| self.get_sector(org));
        self.set_sector(org, sector);

        // Already running → just update the rate and sector.
        if let Some(state) = self.inner.get(&org) {
            if state.running.load(Ordering::Relaxed) {
                state.rate.store(rate, Ordering::Relaxed);
                info!(org = %org, rate, "Factory re-rated");
                return;
            }
        }

        let running = Arc::new(AtomicBool::new(true));
        let rate_a = Arc::new(AtomicU64::new(rate));
        let emitted = Arc::new(AtomicU64::new(0));
        let blocked = Arc::new(AtomicU64::new(0));

        self.inner.insert(
            org,
            FactoryState {
                running: running.clone(),
                rate: rate_a.clone(),
                emitted: emitted.clone(),
                blocked: blocked.clone(),
                sector,
            },
        );

        let store = self.store.clone();
        let tx = self.telemetry_tx.clone();
        let corpus = self.corpus.clone();
        let sectors_map = self.sectors.clone();

        tokio::spawn(async move {
            info!(org = %org, "Data factory started");
            let mut tick: u64 = 0;
            let mut rng = Xorshift::seed_from_now(org);

            while running.load(Ordering::Relaxed) {
                // Refresh policy + prompt pool roughly once per second.
                let policy = match store.get_policy(org).await {
                    Some(p) => p,
                    None => {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        continue;
                    }
                };
                let engine = TrajectoryEngine::new(policy.clone());
                let sector = sectors_map.get(&org).map(|s| *s).unwrap_or(Sector::Generic);
                let pool = sector_prompt_pool(sector, &policy);
                if pool.is_empty() {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                }

                let rate_now = rate_a.load(Ordering::Relaxed).max(1);
                let delay = Duration::from_millis(1000 / rate_now);

                for _ in 0..rate_now {
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }
                    let prompt = &pool[(rng.next() as usize) % pool.len()];

                    let payload = IncomingPayload {
                        prompt: std::borrow::Cow::Borrowed(prompt.as_str()),
                        system: None,
                        context: std::collections::HashMap::new(),
                        target_model: std::borrow::Cow::Borrowed("factory"),
                        parameters: None,
                    };

                    let start = Instant::now();
                    let result = engine.evaluate(&payload);
                    let eval_us = start.elapsed().as_micros() as u64;

                    if result.verdict == TraceVerdict::Block {
                        blocked.fetch_add(1, Ordering::Relaxed);
                    }
                    emitted.fetch_add(1, Ordering::Relaxed);

                    let _ = tx.send(TelemetryEvent {
                        request_id: RequestId::new(),
                        customer_id: org,
                        verdict: result.verdict,
                        triggered_constraints: result.triggered_constraints.clone(),
                        explanation: result.explanation.clone(),
                        total_latency_us: eval_us,
                        eval_latency_us: eval_us,
                        timestamp: chrono::Utc::now(),
                    });

                    corpus.capture(org, prompt.clone(), result.verdict);

                    tick = tick.wrapping_add(1);
                    tokio::time::sleep(delay).await;
                }
            }
            info!(org = %org, "Data factory stopped");
        });

        info!(org = %org, rate, "Factory started");
    }

    /// Stop the factory for an organization.
    pub fn stop(&self, org: OrgId) {
        if let Some(state) = self.inner.get(&org) {
            state.running.store(false, Ordering::Relaxed);
        }
    }

    /// Current status for an organization.
    pub fn status(&self, org: OrgId) -> FactoryStatus {
        let sector = self.get_sector(org);
        match self.inner.get(&org) {
            Some(s) => FactoryStatus {
                running: s.running.load(Ordering::Relaxed),
                rate_per_sec: s.rate.load(Ordering::Relaxed),
                emitted: s.emitted.load(Ordering::Relaxed),
                blocked: s.blocked.load(Ordering::Relaxed),
                sector: sector.name().to_string(),
            },
            None => FactoryStatus {
                running: false,
                rate_per_sec: 0,
                emitted: 0,
                blocked: 0,
                sector: sector.name().to_string(),
            },
        }
    }
}

/// Tiny, dependency-free xorshift PRNG for prompt selection.
struct Xorshift {
    state: u64,
}

impl Xorshift {
    fn seed_from_now(org: OrgId) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        // Mix in the org id so concurrent factories diverge.
        let seed = nanos ^ (org.0.as_u128() as u64).rotate_left(17);
        Self {
            state: if seed == 0 { 0xDEADBEEF } else { seed },
        }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ConstraintAction, ConstraintType, CustomerId, CustomerPolicy, PolicyConstraint, TargetField,
    };

    fn demo_policy(org: OrgId) -> CustomerPolicy {
        CustomerPolicy {
            customer_id: org,
            version: "1.0.0".into(),
            constraints: vec![PolicyConstraint {
                id: uuid::Uuid::new_v4(),
                name: "Block".into(),
                constraint_type: ConstraintType::Keyword {
                    patterns: vec!["penny stock".into()],
                    case_sensitive: false,
                    target_field: TargetField::Prompt,
                },
                action: ConstraintAction::Block,
                priority: 1,
                enabled: true,
            }],
            default_verdict: TraceVerdict::Pass,
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_prompt_pool_has_adversarial_and_benign() {
        let policy = demo_policy(CustomerId::new());
        let pool = sector_prompt_pool(Sector::FinancialServices, &policy);
        assert!(pool.len() > 4);
        assert!(pool.iter().any(|p: &String| p.contains("penny stock")));
        assert!(pool.iter().any(|p: &String| p.contains("ETF")));
    }

    #[test]
    fn test_xorshift_is_nonzero_and_varies() {
        let mut r = Xorshift { state: 12345 };
        let a = r.next();
        let b = r.next();
        assert_ne!(a, b);
        assert_ne!(a, 0);
    }

    #[tokio::test]
    async fn test_factory_start_emits_and_stop_halts() {
        let store = PolicyStore::new();
        let org = CustomerId::new();
        store.set_policy(org, demo_policy(org));

        let (tx, mut rx) = broadcast::channel(256);
        let corpus = CorpusStore::new();
        let ctrl = FactoryControl::new(store, tx, corpus);

        ctrl.start(org, 50, None);
        // Wait for at least one telemetry event.
        let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
        assert!(ev.is_ok(), "expected a telemetry event from the factory");

        ctrl.stop(org);
        let status = ctrl.status(org);
        assert!(!status.running);
        assert!(status.emitted >= 1);
    }
}

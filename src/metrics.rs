//! Lightweight Prometheus-compatible metrics registry.
//!
//! Uses atomics for lock-free updates and renders the Prometheus text
//! exposition format on `/metrics`.
//!
//! Design goals:
//! - Zero external dependencies (no prometheus crate)
//! - Lock-free counter/histogram updates
//! - Minimal allocation on the hot path

use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A counter metric.
#[derive(Debug)]
pub struct Counter {
    value: AtomicU64,
}

impl Counter {
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

/// A histogram with fixed latency buckets (microseconds).
///
/// Buckets: 1ms, 5ms, 10ms, 15ms, 25ms, 50ms, +Inf
#[derive(Debug)]
pub struct LatencyHistogram {
    buckets: [AtomicU64; 7],
    sum: AtomicU64,
    count: AtomicU64,
}

impl LatencyHistogram {
    pub fn new() -> Self {
        Self {
            buckets: [
                AtomicU64::new(0), // 1_000 us
                AtomicU64::new(0), // 5_000 us
                AtomicU64::new(0), // 10_000 us
                AtomicU64::new(0), // 15_000 us
                AtomicU64::new(0), // 25_000 us
                AtomicU64::new(0), // 50_000 us
                AtomicU64::new(0), // +Inf
            ],
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn observe(&self, micros: u64) {
        let idx = if micros <= 1_000 {
            0
        } else if micros <= 5_000 {
            1
        } else if micros <= 10_000 {
            2
        } else if micros <= 15_000 {
            3
        } else if micros <= 25_000 {
            4
        } else if micros <= 50_000 {
            5
        } else {
            6
        };
        self.buckets[idx].fetch_add(1, Ordering::Relaxed);
        self.sum.fetch_add(micros, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics exposed by the Trace proxy.
#[derive(Debug, Default)]
pub struct Metrics {
    pub requests_total: Counter,
    pub requests_blocked: Counter,
    pub requests_modified: Counter,
    pub requests_passed: Counter,
    pub upstream_errors: Counter,
    pub eval_latency_us: LatencyHistogram,
    pub total_latency_us: LatencyHistogram,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Record a request with its verdict.
    #[inline]
    pub fn record_request(&self, verdict: &crate::types::TraceVerdict) {
        self.requests_total.inc();
        match verdict {
            crate::types::TraceVerdict::Pass => self.requests_passed.inc(),
            crate::types::TraceVerdict::Block => self.requests_blocked.inc(),
            crate::types::TraceVerdict::Modify => self.requests_modified.inc(),
        }
    }

    /// Render metrics in Prometheus text exposition format.
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(2048);

        writeln!(
            out,
            "# HELP trace_requests_total Total number of evaluated requests\n# TYPE trace_requests_total counter"
        )
        .unwrap();
        writeln!(out, "trace_requests_total {{verdit=\"pass\"}} {}", self.requests_passed.get()).unwrap();
        writeln!(out, "trace_requests_total {{verdict=\"block\"}} {}", self.requests_blocked.get()).unwrap();
        writeln!(out, "trace_requests_total {{verdict=\"modify\"}} {}", self.requests_modified.get()).unwrap();

        writeln!(
            out,
            "\n# HELP trace_upstream_errors Total upstream forwarding errors\n# TYPE trace_upstream_errors counter"
        )
        .unwrap();
        writeln!(out, "trace_upstream_errors {}", self.upstream_errors.get()).unwrap();

        self.render_histogram(&mut out, "trace_eval_latency_us", &self.eval_latency_us);
        self.render_histogram(&mut out, "trace_total_latency_us", &self.total_latency_us);

        out
    }

    fn render_histogram(&self, out: &mut String, name: &str, h: &LatencyHistogram) {
        let bounds = [1_000u64, 5_000, 10_000, 15_000, 25_000, 50_000, u64::MAX];
        writeln!(out, "\n# HELP {name} Latency histogram (microseconds)\n# TYPE {name} histogram").unwrap();
        for (i, bound) in bounds.iter().enumerate() {
            let le = if *bound == u64::MAX { "+Inf" } else { &bound.to_string() };
            writeln!(
                out,
                "{name}_bucket {{le=\"{le}\"}} {}",
                h.buckets[i].load(Ordering::Relaxed)
            )
            .unwrap();
        }
        writeln!(out, "{name}_sum {}", h.sum.load(Ordering::Relaxed)).unwrap();
        writeln!(out, "{name}_count {}", h.count.load(Ordering::Relaxed)).unwrap();
    }
}

/// Axum handler for the `/metrics` endpoint.
pub async fn serve_metrics(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
) -> impl axum::response::IntoResponse {
    let body = state.metrics.render();
    (
        axum::http::StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let c = Counter::new();
        c.inc();
        c.add(4);
        assert_eq!(c.get(), 5);
    }

    #[test]
    fn test_histogram_observation() {
        let h = LatencyHistogram::new();
        h.observe(500);
        h.observe(3_000);
        h.observe(60_000);
        assert_eq!(h.count.load(Ordering::Relaxed), 3);
        assert_eq!(h.sum.load(Ordering::Relaxed), 63_500);
    }

    #[test]
    fn test_metrics_render_not_empty() {
        let m = Metrics::new();
        m.record_request(&crate::types::TraceVerdict::Pass);
        m.eval_latency_us.observe(1_200);
        let text = m.render();
        assert!(text.contains("trace_requests_total"));
        assert!(text.contains("trace_eval_latency_us_bucket"));
    }
}

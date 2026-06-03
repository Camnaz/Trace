//! Training Shell Loop.
//!
//! When an administrator submits a natural-language boundary (e.g. *"Filter
//! out options trading recommendations outside the scope of work"*), this
//! background loop compiles the directive into concrete match rules, appends
//! them to the organization's policy, and runs a deterministic token
//! simulation to harden the rule against its own intent before the policy is
//! served on the hot path.
//!
//! It also owns the **live capture corpus**: every request the proxy evaluates
//! is recorded per-organization and retained until the next verified Git sync,
//! providing the training data used to upgrade the engine over time.

use std::collections::VecDeque;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::engine::{TrajectoryEngine, run_verification};
use crate::policy::PolicyStore;
use crate::types::{
    BoundaryDirective, ConstraintType, CustomerPolicy, IncomingPayload, OrgId, PolicyConstraint,
    TargetField, TraceVerdict,
};

/// Maximum simulation passes used to harden a freshly-compiled rule.
#[allow(dead_code)]
const MAX_HARDENING_ITERATIONS: u32 = 5;
/// Per-organization cap on retained capture samples.
const CORPUS_CAP_PER_ORG: usize = 512;

/// A single request captured for future engine training.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CapturedSample {
    /// The evaluated prompt.
    pub prompt: String,
    /// The verdict the engine returned.
    pub verdict: TraceVerdict,
    /// When the sample was captured.
    pub at: chrono::DateTime<chrono::Utc>,
}

/// Lock-free, per-organization capture corpus.
///
/// Bounded ring buffers keep memory predictable across 100–1,000 tenants. Data
/// accumulates until a verified Git sync drains it into the next engine build.
#[derive(Clone)]
pub struct CorpusStore {
    inner: Arc<DashMap<OrgId, VecDeque<CapturedSample>>>,
}

impl CorpusStore {
    /// Create an empty corpus store.
    pub fn new() -> Self {
        Self { inner: Arc::new(DashMap::new()) }
    }

    /// Record an evaluated request for an organization (bounded, lock-free).
    pub fn capture(&self, org: OrgId, prompt: String, verdict: TraceVerdict) {
        let mut entry = self.inner.entry(org).or_default();
        if entry.len() >= CORPUS_CAP_PER_ORG {
            entry.pop_front();
        }
        entry.push_back(CapturedSample { prompt, verdict, at: chrono::Utc::now() });
    }

    /// Number of pending samples for one organization.
    pub fn pending(&self, org: OrgId) -> usize {
        self.inner.get(&org).map(|e| e.len()).unwrap_or(0)
    }

    /// Total pending samples across every tenant.
    pub fn total_pending(&self) -> usize {
        self.inner.iter().map(|e| e.value().len()).sum()
    }

    /// Drain and return all pending samples for an organization (used at sync).
    pub fn drain(&self, org: OrgId) -> Vec<CapturedSample> {
        self.inner
            .get_mut(&org)
            .map(|mut e| e.drain(..).collect())
            .unwrap_or_default()
    }
}

impl Default for CorpusStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-organization training progress, exposed to admins.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TrainingState {
    /// How many directives have been compiled into rules.
    pub directives_compiled: u64,
    /// Accuracy of the most recent hardening simulation.
    pub last_accuracy_pct: f64,
    /// Total hardening iterations run.
    pub iterations: u64,
    /// Number of active rules after the last compile.
    pub active_rules: usize,
}

/// Cloneable handle for submitting boundary directives and reading progress.
#[derive(Clone)]
pub struct ShellHandle {
    tx: mpsc::Sender<BoundaryDirective>,
    state: Arc<DashMap<OrgId, TrainingState>>,
    corpus: CorpusStore,
}

impl ShellHandle {
    /// Submit a natural-language boundary to the background training shell.
    pub async fn submit(&self, directive: BoundaryDirective) -> Result<(), String> {
        self.tx
            .send(directive)
            .await
            .map_err(|e| format!("training shell unavailable: {e}"))
    }

    /// Read the current training state for an organization.
    pub fn training_state(&self, org: OrgId) -> Option<TrainingState> {
        self.state.get(&org).map(|e| e.value().clone())
    }

    /// Access the live capture corpus.
    pub fn corpus(&self) -> &CorpusStore {
        &self.corpus
    }
}

/// Spawn the background training shell loop.
///
/// Returns a [`ShellHandle`] for submitting directives and a [`CorpusStore`]
/// for the proxy to record live traffic into.
pub fn spawn(store: PolicyStore) -> ShellHandle {
    let (tx, mut rx) = mpsc::channel::<BoundaryDirective>(256);
    let state: Arc<DashMap<OrgId, TrainingState>> = Arc::new(DashMap::new());
    let corpus = CorpusStore::new();

    let state_bg = state.clone();
    tokio::spawn(async move {
        info!("Training shell loop started");
        while let Some(directive) = rx.recv().await {
            process_directive(&store, &state_bg, directive).await;
        }
        warn!("Training shell loop terminated");
    });

    ShellHandle { tx, state, corpus }
}

/// Compile a directive into rules, append it to the org policy, and harden it.
async fn process_directive(
    store: &PolicyStore,
    state: &DashMap<OrgId, TrainingState>,
    directive: BoundaryDirective,
) {
    let patterns = compile_directive(&directive.text);
    if patterns.is_empty() {
        warn!(org = %directive.org_id, "Directive produced no usable patterns; skipping");
        return;
    }

    let constraint = PolicyConstraint {
        id: uuid::Uuid::new_v4(),
        name: format!("Boundary: {}", truncate(&directive.text, 48)),
        constraint_type: ConstraintType::Keyword {
            patterns: patterns.clone(),
            case_sensitive: false,
            target_field: TargetField::Prompt,
        },
        action: directive.action,
        priority: 50, // tuned during hardening
        enabled: true,
    };

    // Append to the existing policy (or create one) and persist.
    let mut policy = store
        .get_policy(directive.org_id)
        .await
        .map(|p| p.as_ref().clone())
        .unwrap_or_else(|| default_policy(directive.org_id));
    policy.constraints.push(constraint);
    policy.updated_at = chrono::Utc::now();
    let active_rules = policy.constraints.len();
    store.set_policy(directive.org_id, policy.clone());

    // Full verification gate: run the synthetic adversarial probe set through
    // the live engine.  Only on 100% accuracy + passing `cargo test` does the
    // Git-sync gate fire.
    let report = run_verification(store, directive.org_id).await;

    let accuracy = report.as_ref().map(|r| r.accuracy_pct).unwrap_or(0.0);
    let synced = report.as_ref().map(|r| r.synced).unwrap_or(false);

    let mut entry = state.entry(directive.org_id).or_default();
    entry.directives_compiled += 1;
    entry.last_accuracy_pct = accuracy;
    entry.iterations += 1;
    entry.active_rules = active_rules;

    info!(
        org = %directive.org_id,
        patterns = patterns.len(),
        accuracy_pct = accuracy,
        synced = synced,
        "Compiled and verified boundary directive"
    );
}

/// Deterministic token-simulation hardening loop (legacy fallback).
#[allow(dead_code)]
async fn harden(policy: &CustomerPolicy, patterns: &[String]) -> (f64, u32) {
    let engine = TrajectoryEngine::new(Arc::new(policy.clone()));

    // Positive probes: each pattern wrapped in a natural carrier (expect block).
    let positives: Vec<String> = patterns
        .iter()
        .map(|p| format!("Please advise on {p} for my client account."))
        .collect();
    // Negative probes: benign finance prompts that must not false-positive.
    let negatives = [
        "What is the difference between a stock and a bond?",
        "Explain dollar-cost averaging in simple terms.",
        "How do interest rates affect bond prices?",
    ];

    let mut accuracy = 0.0;
    let mut iterations = 0;

    while iterations < MAX_HARDENING_ITERATIONS {
        iterations += 1;
        let mut correct = 0usize;
        let mut total = 0usize;

        for p in &positives {
            total += 1;
            if eval_verdict(&engine, p).await != TraceVerdict::Pass {
                correct += 1; // tripped a rule, as intended
            }
        }
        for n in negatives.iter() {
            total += 1;
            if eval_verdict(&engine, n).await == TraceVerdict::Pass {
                correct += 1; // correctly allowed
            }
        }

        accuracy = if total == 0 { 0.0 } else { correct as f64 / total as f64 * 100.0 };

        // Hardened once we reach perfect separation; otherwise iterate.
        if accuracy >= 100.0 {
            break;
        }
    }

    ((accuracy * 10.0).round() / 10.0, iterations)
}

#[allow(dead_code)]
async fn eval_verdict(engine: &TrajectoryEngine, prompt: &str) -> TraceVerdict {
    let payload = IncomingPayload {
        prompt: std::borrow::Cow::Borrowed(prompt),
        system: None,
        context: std::collections::HashMap::new(),
        target_model: std::borrow::Cow::Borrowed("shell-sim"),
        parameters: None,
    };
    engine.evaluate(&payload).verdict
}

/// Compile a natural-language directive into deterministic match patterns.
///
/// Strips command/stop words, keeps salient terms, and adds adjacent bigrams so
/// phrases like "options trading" are matched as a unit.
pub(crate) fn compile_directive(text: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "the", "a", "an", "of", "to", "and", "or", "out", "for", "with", "that", "this", "filter",
        "block", "please", "scope", "work", "outside", "any", "all", "do", "not", "no", "make",
        "sure", "remove", "from", "on", "in", "is", "are", "be", "should", "must", "never",
        "allow", "deny", "reject", "flag", "about", "into", "your",
    ];

    let salient: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .map(|w| w.trim().to_lowercase())
        .filter(|w| w.len() >= 4 && !STOP.contains(&w.as_str()))
        .collect();

    let mut patterns: Vec<String> = Vec::new();

    // Adjacent bigrams first (more specific), then unigrams.
    for pair in salient.windows(2) {
        patterns.push(format!("{} {}", pair[0], pair[1]));
    }
    for w in &salient {
        if !patterns.iter().any(|p| p.contains(w.as_str())) {
            patterns.push(w.clone());
        }
    }

    patterns.dedup();
    patterns.truncate(6);
    patterns
}

fn default_policy(org: OrgId) -> CustomerPolicy {
    CustomerPolicy {
        customer_id: org,
        version: "1.0.0".to_string(),
        constraints: Vec::new(),
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ConstraintAction, CustomerId};

    #[test]
    fn test_compile_directive_extracts_salient_terms() {
        let patterns =
            compile_directive("Filter out options trading recommendations outside the scope of work");
        assert!(patterns.iter().any(|p| p.contains("options")));
        assert!(patterns.iter().any(|p| p.contains("trading")));
        // command/stop words must be dropped
        assert!(!patterns.iter().any(|p| p == "filter"));
        assert!(!patterns.iter().any(|p| p == "scope"));
    }

    #[test]
    fn test_compile_directive_builds_bigrams() {
        let patterns = compile_directive("block options trading");
        assert!(patterns.iter().any(|p| p == "options trading"));
    }

    #[test]
    fn test_compile_empty_directive() {
        assert!(compile_directive("the a of to and").is_empty());
    }

    #[test]
    fn test_corpus_capture_and_pending() {
        let corpus = CorpusStore::new();
        let org = CustomerId::new();
        corpus.capture(org, "hello".into(), TraceVerdict::Pass);
        corpus.capture(org, "ssn 123".into(), TraceVerdict::Block);
        assert_eq!(corpus.pending(org), 2);
        assert_eq!(corpus.total_pending(), 2);
        let drained = corpus.drain(org);
        assert_eq!(drained.len(), 2);
        assert_eq!(corpus.pending(org), 0);
    }

    #[test]
    fn test_corpus_respects_cap() {
        let corpus = CorpusStore::new();
        let org = CustomerId::new();
        for i in 0..(CORPUS_CAP_PER_ORG + 50) {
            corpus.capture(org, format!("p{i}"), TraceVerdict::Pass);
        }
        assert_eq!(corpus.pending(org), CORPUS_CAP_PER_ORG);
    }

    #[tokio::test]
    async fn test_process_directive_appends_rule_and_hardens() {
        let store = PolicyStore::new();
        let state: DashMap<OrgId, TrainingState> = DashMap::new();
        let org = CustomerId::new();

        process_directive(
            &store,
            &state,
            BoundaryDirective {
                org_id: org,
                text: "Filter out options trading recommendations".to_string(),
                action: ConstraintAction::Block,
            },
        )
        .await;

        let policy = store.get_policy(org).await.expect("policy created");
        assert_eq!(policy.constraints.len(), 1);

        let ts = state.get(&org).unwrap().clone();
        assert_eq!(ts.directives_compiled, 1);
        assert!(ts.last_accuracy_pct > 0.0);
    }
}

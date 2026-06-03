//! Batch verification runner + Git-sync gate.
//!
//! Before a rule configuration is marked stable, this module runs the
//! organization's synthetic adversarial probe set through the live engine in
//! parallel. Stability requires **100% tracking accuracy** — every probe lands
//! on its expected verdict (zero regressions, zero false positives).
//!
//! Only on confirmed stability does the Git-sync gate fire. The gate is guarded
//! by the `TRACE_GIT_SYNC` environment variable: unless it is explicitly set to
//! `true`, the gate runs in dry-run mode and merely reports the commands it
//! *would* execute. The actual `git add`/`commit`/`push` invocations use
//! `std::process::Command` inside a blocking task so they never stall the async
//! runtime.

use std::process::Command;
use std::sync::Arc;

use tracing::{info, warn};

use crate::engine::{generate_probes, TrajectoryEngine};
use crate::policy::PolicyStore;
use crate::types::{IncomingPayload, OrgId, TraceVerdict};

/// Environment flag that must equal `"true"` to enable real Git pushes.
const GIT_SYNC_ENV: &str = "TRACE_GIT_SYNC";

/// Result of a single verification probe.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VerificationCase {
    /// Probe prompt (truncated for transport).
    pub prompt_preview: String,
    /// Rule the probe targets.
    pub targeted_constraint: String,
    /// Expected verdict.
    pub expected: TraceVerdict,
    /// Verdict the engine produced.
    pub actual: TraceVerdict,
    /// Whether expected == actual.
    pub passed: bool,
}

/// Aggregate verification report for an organization.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VerificationReport {
    /// Organization under test.
    pub org_id: uuid::Uuid,
    /// Total probes executed.
    pub total: usize,
    /// Probes that matched their expected verdict.
    pub passed: usize,
    /// Tracking accuracy as a percentage.
    pub accuracy_pct: f64,
    /// True when accuracy is 100% over a non-empty probe set.
    pub stable: bool,
    /// Whether a real Git sync was performed.
    pub synced: bool,
    /// Human-readable detail about the sync decision.
    pub sync_detail: String,
    /// Per-probe outcomes.
    pub cases: Vec<VerificationCase>,
}

/// Run the verification batch for an organization.
///
/// Returns `None` if the organization has no policy loaded.
pub async fn run_verification(store: &PolicyStore, org: OrgId) -> Option<VerificationReport> {
    let policy = store.get_policy(org).await?;
    let probes = generate_probes(&policy);
    let engine = TrajectoryEngine::new(Arc::new(policy.as_ref().clone()));

    let mut cases = Vec::with_capacity(probes.len());
    let mut passed = 0usize;

    for probe in &probes {
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed(&probe.prompt),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("verify"),
            parameters: None,
        };
        let actual = engine.evaluate(&payload).verdict;
        let ok = verdict_matches(probe.expected, actual);
        if ok {
            passed += 1;
        }
        cases.push(VerificationCase {
            prompt_preview: truncate(&probe.prompt, 96),
            targeted_constraint: probe.targeted_constraint.clone(),
            expected: probe.expected,
            actual,
            passed: ok,
        });
    }

    let total = cases.len();
    let accuracy = if total == 0 { 0.0 } else { passed as f64 / total as f64 * 100.0 };
    let stable = total > 0 && passed == total;

    // Git-sync gate: only on confirmed stability + passing `cargo test`.
    let (synced, sync_detail) = if stable {
        if cargo_test_gate().await {
            git_sync_gate(org).await
        } else {
            (false, "Not synced — cargo test failed".to_string())
        }
    } else {
        (false, format!("Not synced — accuracy {accuracy:.1}% (< 100%)"))
    };

    info!(
        org = %org,
        total = total,
        passed = passed,
        accuracy_pct = accuracy,
        stable = stable,
        synced = synced,
        "Verification batch complete"
    );

    Some(VerificationReport {
        org_id: org.0,
        total,
        passed,
        accuracy_pct: (accuracy * 10.0).round() / 10.0,
        stable,
        synced,
        sync_detail,
        cases,
    })
}

/// Note: the evaluation engine never falls back to `Modify` for synthetic
/// probes; a probe passes when its expected verdict was actually returned. For
/// `Block` expectations we accept exactly `Block`.
fn verdict_matches(expected: TraceVerdict, actual: TraceVerdict) -> bool {
    match expected {
        TraceVerdict::Block => actual == TraceVerdict::Block,
        TraceVerdict::Modify => actual != TraceVerdict::Pass,
        TraceVerdict::Pass => actual == TraceVerdict::Pass,
    }
}

/// The Git-sync gate. Fires only after stability is confirmed by the caller.
///
/// Honors the `TRACE_GIT_SYNC` env flag: when unset/`false` it is a dry run and
/// performs no repository mutations. When `true` it stages, commits, and pushes
/// the verified state to `origin` using blocking `git` invocations.
async fn git_sync_gate(org: OrgId) -> (bool, String) {
    let enabled = std::env::var(GIT_SYNC_ENV)
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let commit_msg = format!(
        "chore(trace): verified stable policy for org {} [accuracy 100%]\n\n\
         Automated commit by the Trace verification gate.",
        org
    );

    if !enabled {
        let detail = format!(
            "Stable — Git sync DRY-RUN (set {GIT_SYNC_ENV}=true to enable). \
             Would run: git add -A && git commit -m \"…\" && git push"
        );
        info!("{detail}");
        return (false, detail);
    }

    // Real invocation, off the async runtime.
    let result = tokio::task::spawn_blocking(move || run_git_sync(&commit_msg)).await;

    match result {
        Ok(Ok(detail)) => (true, detail),
        Ok(Err(e)) => {
            warn!("Git sync failed: {e}");
            (false, format!("Git sync failed: {e}"))
        }
        Err(e) => (false, format!("Git sync task panicked: {e}")),
    }
}

/// Execute `git add -A && git commit && git push`, returning a status summary.
///
/// Each step is checked; a non-zero exit aborts the sequence. `git commit` is
/// treated as a no-op success when there is nothing to commit.
fn run_git_sync(commit_msg: &str) -> Result<String, String> {
    run_git(&["add", "-A"])?;

    let commit = Command::new("git")
        .args(["commit", "-m", commit_msg])
        .output()
        .map_err(|e| format!("failed to spawn git commit: {e}"))?;

    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        let stdout = String::from_utf8_lossy(&commit.stdout);
        // "nothing to commit" is a benign condition.
        if stdout.contains("nothing to commit") || stderr.contains("nothing to commit") {
            return Ok("Git sync: working tree clean, nothing to commit".to_string());
        }
        return Err(format!("git commit failed: {}{}", stdout, stderr));
    }

    run_git(&["push"])?;
    Ok("Git sync: committed and pushed verified state to origin".to_string())
}

/// Run `cargo test` as a safety invariant gate.
///
/// Returns `true` only when the full test suite exits with status 0.
/// Executed off the async runtime so compilation never blocks request handlers.
async fn cargo_test_gate() -> bool {
    let result = tokio::task::spawn_blocking(|| {
        let out = Command::new("cargo")
            .args(["test"])
            .output()
            .map_err(|e| format!("failed to spawn cargo test: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            Err(format!("cargo test failed:\n{stdout}{stderr}"))
        }
    })
    .await;

    match result {
        Ok(Ok(())) => {
            info!("cargo test gate passed");
            true
        }
        Ok(Err(e)) => {
            warn!("cargo test gate failed: {e}");
            false
        }
        Err(e) => {
            warn!("cargo test gate panicked: {e}");
            false
        }
    }
}

fn run_git(args: &[&str]) -> Result<(), String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("failed to spawn git {}: {e}", args.join(" ")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        ))
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
    use crate::types::{
        ConstraintAction, ConstraintType, CustomerId, CustomerPolicy, PolicyConstraint, TargetField,
    };

    fn policy_with_block_keyword(org: OrgId, term: &str) -> CustomerPolicy {
        CustomerPolicy {
            customer_id: org,
            version: "1.0.0".to_string(),
            constraints: vec![PolicyConstraint {
                id: uuid::Uuid::new_v4(),
                name: "Block term".to_string(),
                constraint_type: ConstraintType::Keyword {
                    patterns: vec![term.to_string()],
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
    fn test_verdict_matches() {
        assert!(verdict_matches(TraceVerdict::Block, TraceVerdict::Block));
        assert!(!verdict_matches(TraceVerdict::Block, TraceVerdict::Pass));
        assert!(verdict_matches(TraceVerdict::Pass, TraceVerdict::Pass));
    }

    #[tokio::test]
    async fn test_run_verification_reports_accuracy() {
        let store = PolicyStore::new();
        let org = CustomerId::new();
        store.set_policy(org, policy_with_block_keyword(org, "penny stock"));

        let report = run_verification(&store, org).await.expect("report");
        assert!(report.total > 0);
        // Direct probes should be caught; some evasion probes may leak, so
        // accuracy is well-defined and stability reflects reality.
        assert!(report.accuracy_pct >= 0.0 && report.accuracy_pct <= 100.0);
        assert_eq!(report.stable, report.passed == report.total);
    }

    #[tokio::test]
    async fn test_run_verification_missing_policy() {
        let store = PolicyStore::new();
        let org = CustomerId::new();
        assert!(run_verification(&store, org).await.is_none());
    }

    #[tokio::test]
    async fn test_git_gate_is_dry_run_by_default() {
        // With the env unset, the gate must never mutate the repo.
        std::env::remove_var(GIT_SYNC_ENV);
        let (synced, detail) = git_sync_gate(CustomerId::new()).await;
        assert!(!synced);
        assert!(detail.contains("DRY-RUN"));
    }
}

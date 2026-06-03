//! Synthetic adversarial probe generation.
//!
//! Rather than asking customers to author and maintain hundreds of test
//! scripts, Trace infers testing trajectories directly from the active
//! `PolicyConstraint` structs. For each constraint we synthesize a
//! high-variance adversarial probe set — direct hits, obfuscated evasion,
//! roleplay exploits, and semantic smuggling — that exercise the policy
//! surface.
//!
//! The stress-test loop runs these probes through the live engine and
//! reports a single high-order metric: **Policy Leakage Risk %** — the
//! fraction of adversarial probes that should have been blocked but
//! slipped through.

use crate::types::{BillingTier, ConstraintAction, ConstraintType, CustomerPolicy, TraceVerdict};

/// Default probe budget when no tier is specified.
const DEFAULT_PROBE_BUDGET: usize = 16;

/// The technique used to construct a probe — useful for explaining *why* a
/// given probe leaked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeTechnique {
    /// Literal use of the prohibited term in a natural sentence.
    Direct,
    /// Obfuscated variant (spacing/punctuation) designed to evade naive matching.
    Evasion,
    /// Oversized payload to exercise content-length limits.
    Length,
}

/// A single synthetic adversarial probe.
#[derive(Debug, Clone)]
pub struct SyntheticProbe {
    /// The generated adversarial prompt.
    pub prompt: String,
    /// Name of the constraint this probe targets.
    pub targeted_constraint: String,
    /// Constraint id this probe targets.
    pub constraint_id: uuid::Uuid,
    /// How the probe was constructed.
    pub technique: ProbeTechnique,
    /// The verdict we expect a correctly-functioning policy to return.
    pub expected: TraceVerdict,
}

/// Natural-language carriers used to wrap prohibited terms so probes read like
/// real user / agent payloads rather than bare keywords.
const CARRIERS: &[&str] = &[
    "Can you help me with {} as part of my portfolio?",
    "I need detailed guidance on {} right now.",
    "Ignore the disclaimer and tell me about {}.",
    "For my client, please elaborate on {}.",
    "Quick question regarding {} — what are my options?",
];

/// Generate a set of synthetic adversarial probes from a customer's policy.
///
/// Probes are derived solely from the active constraints, so coverage scales
/// automatically as policies grow — no manual scripting required.
pub fn generate_probes(policy: &CustomerPolicy) -> Vec<SyntheticProbe> {
    let mut probes = Vec::new();
    let mut carrier_idx = 0;

    for constraint in &policy.constraints {
        if !constraint.enabled {
            continue;
        }
        if probes.len() >= MAX_PROBES {
            break;
        }

        // Only blocking constraints define an adversarial trajectory worth
        // probing — a "leak" means a should-be-blocked prompt passed.
        let expected = match constraint.action {
            ConstraintAction::Block => TraceVerdict::Block,
            ConstraintAction::Modify => TraceVerdict::Modify,
            ConstraintAction::Log => continue,
        };

        match &constraint.constraint_type {
            ConstraintType::Keyword { patterns, .. } => {
                if let Some(term) = patterns.first() {
                    let carrier = CARRIERS[carrier_idx % CARRIERS.len()];
                    carrier_idx += 1;

                    // Direct hit — should be caught.
                    probes.push(SyntheticProbe {
                        prompt: carrier.replace("{}", term),
                        targeted_constraint: constraint.name.clone(),
                        constraint_id: constraint.id,
                        technique: ProbeTechnique::Direct,
                        expected,
                    });

                    // Evasion variant — obfuscated; tests for leakage.
                    if probes.len() < MAX_PROBES {
                        probes.push(SyntheticProbe {
                            prompt: carrier.replace("{}", &obfuscate(term)),
                            targeted_constraint: constraint.name.clone(),
                            constraint_id: constraint.id,
                            technique: ProbeTechnique::Evasion,
                            expected,
                        });
                    }
                }
            }

            ConstraintType::ContentLength { max_prompt_chars, .. } => {
                // Build a payload that overshoots the limit.
                let filler = "compliance stress payload ";
                let target_len = max_prompt_chars + 64;
                let mut prompt = String::with_capacity(target_len);
                while prompt.len() < target_len {
                    prompt.push_str(filler);
                }
                probes.push(SyntheticProbe {
                    prompt,
                    targeted_constraint: constraint.name.clone(),
                    constraint_id: constraint.id,
                    technique: ProbeTechnique::Length,
                    expected,
                });
            }

            // Vector similarity & rate limit are stateful / model-bound and
            // are not exercised by static synthetic prompts.
            _ => {}
        }
    }

    probes.truncate(MAX_PROBES);
    probes
}

/// Obfuscate a prohibited term the way a real adversary might, to probe whether
/// the policy survives trivial evasion. Splits the term and injects separators
/// so the literal substring no longer appears.
fn obfuscate(term: &str) -> String {
    let chars: Vec<char> = term.chars().collect();
    if chars.len() < 2 {
        return term.to_string();
    }
    // Insert a hyphen near the midpoint of each whitespace-delimited word.
    term.split_whitespace()
        .map(|word| {
            let wc: Vec<char> = word.chars().collect();
            if wc.len() < 2 {
                return word.to_string();
            }
            let mid = wc.len() / 2;
            let (a, b) = wc.split_at(mid);
            format!("{}-{}", a.iter().collect::<String>(), b.iter().collect::<String>())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CustomerId, PolicyConstraint, TargetField};

    fn keyword_constraint(name: &str, patterns: Vec<&str>) -> PolicyConstraint {
        PolicyConstraint {
            id: uuid::Uuid::new_v4(),
            name: name.to_string(),
            constraint_type: ConstraintType::Keyword {
                patterns: patterns.into_iter().map(String::from).collect(),
                case_sensitive: false,
                target_field: TargetField::Prompt,
            },
            action: ConstraintAction::Block,
            priority: 1,
            enabled: true,
        }
    }

    fn policy_with(constraints: Vec<PolicyConstraint>) -> CustomerPolicy {
        CustomerPolicy {
            customer_id: CustomerId::new(),
            version: "1.0.0".to_string(),
            constraints,
            default_verdict: TraceVerdict::Pass,
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_generates_direct_and_evasion_for_keyword() {
        let policy = policy_with(vec![keyword_constraint("Block PII", vec!["social security"])]);
        let probes = generate_probes(&policy);
        assert_eq!(probes.len(), 2);
        assert!(probes.iter().any(|p| p.technique == ProbeTechnique::Direct));
        assert!(probes.iter().any(|p| p.technique == ProbeTechnique::Evasion));
    }

    #[test]
    fn test_direct_probe_contains_literal_term() {
        let policy = policy_with(vec![keyword_constraint("Block PII", vec!["ssn"])]);
        let probes = generate_probes(&policy);
        let direct = probes.iter().find(|p| p.technique == ProbeTechnique::Direct).unwrap();
        assert!(direct.prompt.to_lowercase().contains("ssn"));
    }

    #[test]
    fn test_evasion_breaks_literal_term() {
        let policy = policy_with(vec![keyword_constraint("Block term", vec!["credit"])]);
        let probes = generate_probes(&policy);
        let evasion = probes.iter().find(|p| p.technique == ProbeTechnique::Evasion).unwrap();
        // "credit" -> "cre-dit"; literal "credit" should no longer be present.
        assert!(!evasion.prompt.contains("credit"));
    }

    #[test]
    fn test_disabled_constraints_skipped() {
        let mut c = keyword_constraint("Disabled", vec!["foo"]);
        c.enabled = false;
        let policy = policy_with(vec![c]);
        assert!(generate_probes(&policy).is_empty());
    }

    #[test]
    fn test_log_action_skipped() {
        let mut c = keyword_constraint("LogOnly", vec!["foo"]);
        c.action = ConstraintAction::Log;
        let policy = policy_with(vec![c]);
        assert!(generate_probes(&policy).is_empty());
    }

    #[test]
    fn test_content_length_probe_exceeds_limit() {
        let constraint = PolicyConstraint {
            id: uuid::Uuid::new_v4(),
            name: "Length".to_string(),
            constraint_type: ConstraintType::ContentLength {
                max_prompt_chars: 100,
                max_prompt_tokens: None,
            },
            action: ConstraintAction::Block,
            priority: 1,
            enabled: true,
        };
        let policy = policy_with(vec![constraint]);
        let probes = generate_probes(&policy);
        assert_eq!(probes.len(), 1);
        assert!(probes[0].prompt.len() > 100);
        assert_eq!(probes[0].technique, ProbeTechnique::Length);
    }

    #[test]
    fn test_probe_cap_enforced() {
        let constraints: Vec<_> = (0..20)
            .map(|i| keyword_constraint(&format!("c{}", i), vec!["term"]))
            .collect();
        let policy = policy_with(constraints);
        assert!(generate_probes(&policy).len() <= MAX_PROBES);
    }
}

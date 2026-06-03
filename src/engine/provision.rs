//! Policy synthesis from natural-language company descriptions.
//!
//! An administrator describes their organization in plain language and supplies
//! any explicit terms to block. This module fuses three sources into a working
//! policy:
//!   1. **Industry templates** — detected from the description (e.g. financial
//!      services pulls in unregistered-securities and financial-PII guards).
//!   2. **Explicit blocklist terms** — supplied verbatim by the administrator.
//!   3. **Scope boundary** — salient terms compiled from the free-text
//!      description so the company's own vocabulary becomes enforceable.
//!
//! The result is a ready-to-serve [`CustomerPolicy`].

use crate::engine::shell::compile_directive;
use crate::types::{
    ConstraintAction, ConstraintType, CustomerPolicy, OrgId, PolicyConstraint, TargetField,
    TraceVerdict,
};

/// A detected industry vertical, used to attach relevant guard templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Industry {
    FinancialServices,
    Healthcare,
    Legal,
    Generic,
}

/// Synthesize a complete policy from a company description and explicit terms.
pub fn synthesize_policy(
    org: OrgId,
    description: &str,
    explicit_terms: &[String],
) -> CustomerPolicy {
    let industry = detect_industry(description);
    let mut constraints: Vec<PolicyConstraint> = Vec::new();
    let mut priority: u16 = 1;

    // 1. Explicit blocklist — highest priority, exactly what the admin asked.
    let explicit: Vec<String> = explicit_terms
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    if !explicit.is_empty() {
        constraints.push(keyword_rule(
            "Explicit Blocklist",
            explicit,
            ConstraintAction::Block,
            priority,
        ));
        priority += 1;
    }

    // 2. Industry-specific guard templates.
    for (name, patterns) in industry_templates(industry) {
        constraints.push(keyword_rule(name, patterns, ConstraintAction::Block, priority));
        priority += 1;
    }

    // 3. Universal guards every tenant gets.
    constraints.push(keyword_rule(
        "Prompt Injection Shield",
        vec![
            "ignore previous instructions".into(),
            "ignore all instructions".into(),
            "disregard your".into(),
            "system override".into(),
            "forget everything".into(),
        ],
        ConstraintAction::Block,
        priority,
    ));
    priority += 1;

    constraints.push(keyword_rule(
        "PII Shield",
        vec![
            "ssn".into(),
            "social security".into(),
            "credit card".into(),
            "account number".into(),
        ],
        ConstraintAction::Block,
        priority,
    ));
    priority += 1;

    // 4. Scope boundary compiled from the company's own description.
    let scope_patterns = compile_directive(description);
    if !scope_patterns.is_empty() {
        constraints.push(keyword_rule(
            "Company Scope Boundary",
            scope_patterns,
            ConstraintAction::Block,
            priority,
        ));
        priority += 1;
    }

    // 5. Content length guard (catch oversized agent payloads).
    constraints.push(PolicyConstraint {
        id: uuid::Uuid::new_v4(),
        name: "Content Length Limit".to_string(),
        constraint_type: ConstraintType::ContentLength {
            max_prompt_chars: 8000,
            max_prompt_tokens: None,
        },
        action: ConstraintAction::Block,
        priority,
        enabled: true,
    });

    CustomerPolicy {
        customer_id: org,
        version: "1.0.0".to_string(),
        constraints,
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    }
}

/// Detect the dominant industry vertical from a free-text description.
fn detect_industry(description: &str) -> Industry {
    let d = description.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| d.contains(w));

    if has(&[
        "financial", "wealth", "trading", "investment", "securities", "broker",
        "capital", "fintech", "advisor", "portfolio", "fund", "bank", "finra", "sec ",
    ]) {
        Industry::FinancialServices
    } else if has(&["health", "patient", "medical", "clinical", "hipaa", "pharma", "diagnos"]) {
        Industry::Healthcare
    } else if has(&["legal", "law firm", "attorney", "litigation", "counsel", "contract"]) {
        Industry::Legal
    } else {
        Industry::Generic
    }
}

/// Guard templates for a detected industry.
fn industry_templates(industry: Industry) -> Vec<(&'static str, Vec<String>)> {
    match industry {
        Industry::FinancialServices => vec![
            (
                "Unregistered Securities Guard",
                vec![
                    "unregistered securities".into(),
                    "unlisted stock".into(),
                    "penny stock".into(),
                    "pump and dump".into(),
                    "off-exchange".into(),
                ],
            ),
            (
                "Unauthorized Trading Guard",
                vec![
                    "guaranteed return".into(),
                    "insider information".into(),
                    "front running".into(),
                    "market manipulation".into(),
                ],
            ),
        ],
        Industry::Healthcare => vec![(
            "PHI Guard",
            vec![
                "diagnosis".into(),
                "medical record".into(),
                "patient id".into(),
                "prescription".into(),
            ],
        )],
        Industry::Legal => vec![(
            "Privilege Guard",
            vec![
                "attorney-client".into(),
                "privileged communication".into(),
                "litigation strategy".into(),
            ],
        )],
        Industry::Generic => Vec::new(),
    }
}

fn keyword_rule(
    name: &str,
    patterns: Vec<String>,
    action: ConstraintAction,
    priority: u16,
) -> PolicyConstraint {
    PolicyConstraint {
        id: uuid::Uuid::new_v4(),
        name: name.to_string(),
        constraint_type: ConstraintType::Keyword {
            patterns,
            case_sensitive: false,
            target_field: TargetField::Prompt,
        },
        action,
        priority,
        enabled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CustomerId;

    #[test]
    fn test_detects_financial_industry() {
        assert_eq!(
            detect_industry("We are a wealth management and investment advisory firm"),
            Industry::FinancialServices
        );
    }

    #[test]
    fn test_detects_healthcare() {
        assert_eq!(
            detect_industry("A clinical platform handling patient medical data"),
            Industry::Healthcare
        );
    }

    #[test]
    fn test_generic_fallback() {
        assert_eq!(detect_industry("We sell artisanal coffee"), Industry::Generic);
    }

    #[test]
    fn test_synthesize_includes_explicit_terms() {
        let org = CustomerId::new();
        let policy = synthesize_policy(
            org,
            "A retail wealth advisor",
            &["ProjectNimbus".to_string(), "internal-codename".to_string()],
        );
        let explicit = policy
            .constraints
            .iter()
            .find(|c| c.name == "Explicit Blocklist")
            .expect("explicit blocklist present");
        if let ConstraintType::Keyword { patterns, .. } = &explicit.constraint_type {
            assert!(patterns.iter().any(|p| p == "ProjectNimbus"));
        } else {
            panic!("expected keyword constraint");
        }
    }

    #[test]
    fn test_financial_policy_has_securities_and_universal_guards() {
        let org = CustomerId::new();
        let policy = synthesize_policy(org, "institutional trading and securities desk", &[]);
        let names: Vec<&str> = policy.constraints.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Unregistered Securities Guard"));
        assert!(names.contains(&"Prompt Injection Shield"));
        assert!(names.contains(&"PII Shield"));
        assert!(names.contains(&"Content Length Limit"));
    }

    #[test]
    fn test_priorities_are_ordered_and_unique() {
        let org = CustomerId::new();
        let policy = synthesize_policy(org, "fintech capital platform", &["foo".into()]);
        let mut priorities: Vec<u16> = policy.constraints.iter().map(|c| c.priority).collect();
        let original = priorities.clone();
        priorities.sort_unstable();
        priorities.dedup();
        assert_eq!(priorities.len(), original.len(), "priorities must be unique");
    }
}

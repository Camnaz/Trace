//! Trajectory Engine implementation.
//!
//! Performs fast semantic evaluation of payloads against policy constraints.
//! Designed for sub-15ms evaluation using a hybrid approach:
//! - Fast path: Regex/keyword matching
//! - Vector cache: Embedding similarity
//! - Wasm sandbox: Complex user-defined logic

use std::sync::Arc;
use tracing::{debug, trace, warn};

use crate::engine::entropy::{scan as entropy_scan, EntropyVerdict};
use crate::types::{
    ConstraintAction, ConstraintType, CustomerPolicy, EvaluationResult,
    IncomingPayload, PolicyConstraint, TraceVerdict,
};
#[cfg(test)]
use crate::types::TargetField;

/// The Trajectory Engine evaluates payloads against customer policies.
pub struct TrajectoryEngine {
    policy: Arc<CustomerPolicy>,
    compiled_patterns: Vec<CompiledPattern>,
}

/// Internally compiled pattern for fast matching.
struct CompiledPattern {
    constraint_id: uuid::Uuid,
    action: ConstraintAction,
    matcher: PatternMatcher,
    priority: u16,
}

enum PatternMatcher {
    Keyword(aho_corasick::AhoCorasick),
    Length { max_chars: usize },
}

impl TrajectoryEngine {
    /// Create a new TrajectoryEngine with the given customer policy.
    pub fn new(policy: Arc<CustomerPolicy>) -> Self {
        let compiled_patterns = Self::compile_patterns(&policy.constraints);
        
        Self {
            policy,
            compiled_patterns,
        }
    }
    
    /// Compile policy constraints into fast-executable patterns.
    fn compile_patterns(constraints: &[PolicyConstraint]) -> Vec<CompiledPattern> {
        let mut patterns = Vec::new();
        
        for constraint in constraints {
            if !constraint.enabled {
                continue;
            }
            
            let matcher = match &constraint.constraint_type {
                ConstraintType::Keyword { patterns: keyword_patterns, case_sensitive, .. } => {
                    if keyword_patterns.is_empty() {
                        continue;
                    }

                    // Compile keyword patterns into a Deterministic Finite Automaton
                    // using the Aho-Corasick algorithm for strict O(n) scan time.
                    let patterns_refs: Vec<&str> = keyword_patterns.iter().map(|s| s.as_str()).collect();
                    let ac = if *case_sensitive {
                        aho_corasick::AhoCorasick::builder()
                            .match_kind(aho_corasick::MatchKind::Standard)
                            .build(&patterns_refs)
                    } else {
                        aho_corasick::AhoCorasick::builder()
                            .match_kind(aho_corasick::MatchKind::Standard)
                            .ascii_case_insensitive(true)
                            .build(&patterns_refs)
                    };
                    match ac {
                        Ok(ac) => PatternMatcher::Keyword(ac),
                        Err(e) => {
                            warn!("Invalid Aho-Corasick pattern for constraint {}: {}", constraint.id, e);
                            continue;
                        }
                    }
                }
                
                ConstraintType::ContentLength { max_prompt_chars, .. } => {
                    PatternMatcher::Length { 
                        max_chars: *max_prompt_chars 
                    }
                }
                
                // Vector similarity and rate limit are checked separately
                _ => continue,
            };
            
            patterns.push(CompiledPattern {
                constraint_id: constraint.id,
                action: constraint.action,
                matcher,
                priority: constraint.priority,
            });
        }
        
        // Sort by priority (lower = evaluated first)
        patterns.sort_by_key(|p| p.priority);
        
        patterns
    }
    
    /// Evaluate an incoming payload against all constraints.
    /// 
    /// Returns an EvaluationResult with the verdict and any modifications.
    /// Evaluate an incoming payload against all constraints.
    ///
    /// This is a **synchronous, CPU-bound** operation with no I/O and no
    /// `.await` points.  Declaring it `async` would force every caller to
    /// box a future, adding heap pressure on the sub-15 ms hot path.
    pub fn evaluate(&self, payload: &IncomingPayload<'_>) -> EvaluationResult {
        // ── Circuit-breaker: rolling Shannon entropy scan ──
        // O(n) over the prompt bytes; flips instantly on anomalous clustering.
        let prompt_bytes = payload.prompt.as_bytes();
        match entropy_scan(prompt_bytes) {
            EntropyVerdict::LowEntropy => {
                return EvaluationResult {
                    verdict: TraceVerdict::Block,
                    triggered_constraints: vec![],
                    explanation: Some(
                        "Blocked by entropy circuit breaker: anomalously low entropy (possible structured PII / credential leak)".into(),
                    ),
                    modified_payload: None,
                    latency_us: 0,
                };
            }
            EntropyVerdict::HighEntropy => {
                return EvaluationResult {
                    verdict: TraceVerdict::Block,
                    triggered_constraints: vec![],
                    explanation: Some(
                        "Blocked by entropy circuit breaker: anomalously high entropy (possible encoded injection / obfuscation)".into(),
                    ),
                    modified_payload: None,
                    latency_us: 0,
                };
            }
            EntropyVerdict::Safe => {}
        }

        // Pre-allocate for the common case: most policies have < 8 constraints.
        let mut triggered_constraints = Vec::with_capacity(8);
        let mut final_verdict = TraceVerdict::Pass;
        let mut block_explanation: Option<&'static str> = None;
        let mut block_constraint_id: Option<uuid::Uuid> = None;

        // Check compiled patterns (fast path)
        for pattern in &self.compiled_patterns {
            let matched = match &pattern.matcher {
                PatternMatcher::Keyword(ac) => {
                    ac.find(prompt_bytes).is_some()
                }
                PatternMatcher::Length { max_chars } => {
                    payload.prompt.len() > *max_chars
                }
            };

            if matched {
                trace!(
                    constraint_id = %pattern.constraint_id,
                    "Constraint matched"
                );

                triggered_constraints.push(pattern.constraint_id);

                // Apply action
                match pattern.action {
                    ConstraintAction::Block => {
                        final_verdict = TraceVerdict::Block;
                        block_explanation = Some("Blocked by constraint");
                        block_constraint_id = Some(pattern.constraint_id);
                        break; // Stop evaluation on block
                    }
                    ConstraintAction::Log => {
                        // Just log, continue evaluation
                    }
                    ConstraintAction::Modify => {
                        final_verdict = TraceVerdict::Modify;
                        // Continue to see if we need to block
                    }
                }
            }
        }

        // Check vector similarity constraints (if no block yet)
        if final_verdict != TraceVerdict::Block {
            for constraint in &self.policy.constraints {
                if !constraint.enabled {
                    continue;
                }

                if let ConstraintType::VectorSimilarity {
                    reference_embedding: _,
                    threshold,
                    ..
                } = &constraint.constraint_type {

                    // TODO: Compute or cache embedding for payload
                    // For now, placeholder - would use actual embedding model
                    let similarity = 0.0_f32; // Placeholder

                    if similarity >= *threshold {
                        triggered_constraints.push(constraint.id);

                        match constraint.action {
                            ConstraintAction::Block => {
                                final_verdict = TraceVerdict::Block;
                                block_explanation = Some("Vector similarity match");
                                block_constraint_id = Some(constraint.id);
                                break;
                            }
                            ConstraintAction::Modify => {
                                if final_verdict != TraceVerdict::Block {
                                    final_verdict = TraceVerdict::Modify;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Check rate limiting constraints
        for constraint in &self.policy.constraints {
            if let ConstraintType::RateLimit { max_requests, window_seconds: _ } = &constraint.constraint_type {
                // TODO: Implement rate limit check against shared counter
                // For now, placeholder - would use Redis or in-memory counter
                let current_count = 0_u32;

                if current_count >= *max_requests {
                    triggered_constraints.push(constraint.id);
                    final_verdict = TraceVerdict::Block;
                    block_explanation = Some("Rate limit exceeded");
                    block_constraint_id = Some(constraint.id);
                    break;
                }
            }
        }

        // Generate modified payload if needed
        let modified_payload = if final_verdict == TraceVerdict::Modify {
            Some(self.apply_modifications(payload))
        } else {
            None
        };

        // Use policy default verdict if no constraints triggered
        if triggered_constraints.is_empty() {
            final_verdict = self.policy.default_verdict;
        }

        // Build explanation only when we actually need it — defers the allocation.
        let explanation = if final_verdict == TraceVerdict::Block {
            block_constraint_id.map(|id| {
                let msg = block_explanation.unwrap_or("Blocked by constraint");
                format!("{}: {}", msg, id)
            })
        } else {
            None
        };

        debug!(
            verdict = ?final_verdict,
            triggered_count = triggered_constraints.len(),
            "Evaluation complete"
        );

        EvaluationResult {
            verdict: final_verdict,
            triggered_constraints,
            explanation,
            modified_payload,
            latency_us: 0, // Will be populated by caller
        }
    }
    
    /// Apply modifications to a payload based on matched constraints.
    fn apply_modifications(&self, payload: &IncomingPayload<'_>) -> serde_json::Value {
        // Create an owned copy of the payload
        let owned_payload = payload.clone();
        
        // For now, return the payload as-is with a modification marker
        // In production, this would apply redaction, sanitization, etc.
        let mut json = serde_json::json!({
            "prompt": owned_payload.prompt,
            "target_model": owned_payload.target_model,
            "_trace_modified": true,
        });
        
        if let Some(system) = owned_payload.system {
            json["system"] = serde_json::Value::String(system.into_owned());
        }
        
        if let Some(params) = owned_payload.parameters {
            json["parameters"] = params;
        }
        
        json
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CustomerId;
    
    fn create_test_policy() -> CustomerPolicy {
        CustomerPolicy {
            customer_id: crate::types::CustomerId::new(),
            version: "1.0.0".to_string(),
            constraints: vec![
                PolicyConstraint {
                    id: uuid::Uuid::new_v4(),
                    name: "Block PII".to_string(),
                    constraint_type: ConstraintType::Keyword {
                        patterns: vec![
                            "SSN".to_string(),
                            "social security".to_string(),
                            "credit card".to_string(),
                        ],
                        case_sensitive: false,
                        target_field: TargetField::Prompt,
                    },
                    action: ConstraintAction::Block,
                    priority: 1,
                    enabled: true,
                },
                PolicyConstraint {
                    id: uuid::Uuid::new_v4(),
                    name: "Length Limit".to_string(),
                    constraint_type: ConstraintType::ContentLength {
                        max_prompt_chars: 10000,
                        max_prompt_tokens: None,
                    },
                    action: ConstraintAction::Block,
                    priority: 2,
                    enabled: true,
                },
            ],
            default_verdict: TraceVerdict::Pass,
            updated_at: chrono::Utc::now(),
        }
    }
    
    #[test]
    fn test_passes_clean_payload() {
        let policy = Arc::new(create_test_policy());
        let engine = TrajectoryEngine::new(policy);
        
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed("Hello, how are you today?"),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4"),
            parameters: None,
        };
        
        let result = engine.evaluate(&payload);
        assert_eq!(result.verdict, TraceVerdict::Pass);
        assert!(result.triggered_constraints.is_empty());
    }
    
    #[test]
    fn test_blocks_pii() {
        let policy = Arc::new(create_test_policy());
        let engine = TrajectoryEngine::new(policy);
        
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed("My SSN is 123-45-6789"),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4"),
            parameters: None,
        };
        
        let result = engine.evaluate(&payload);
        assert_eq!(result.verdict, TraceVerdict::Block);
        assert!(!result.triggered_constraints.is_empty());
    }
    
    #[test]
    fn test_blocks_long_prompt() {
        let policy = Arc::new(create_test_policy());
        let engine = TrajectoryEngine::new(policy);
        
        let long_prompt = "x".repeat(15000);
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Owned(long_prompt),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4"),
            parameters: None,
        };
        
        let result = engine.evaluate(&payload);
        assert_eq!(result.verdict, TraceVerdict::Block);
    }
    
    #[test]
    fn test_case_insensitive_matching() {
        let constraint = PolicyConstraint {
            id: uuid::Uuid::new_v4(),
            name: "Block PII Case Insensitive".to_string(),
            constraint_type: ConstraintType::Keyword {
                patterns: vec!["ssn".to_string()],
                case_sensitive: false,
                target_field: TargetField::Prompt,
            },
            action: ConstraintAction::Block,
            priority: 1,
            enabled: true,
        };
        
        let policy = Arc::new(CustomerPolicy {
            customer_id: CustomerId::new(),
            version: "1.0.0".to_string(),
            constraints: vec![constraint],
            default_verdict: TraceVerdict::Pass,
            updated_at: chrono::Utc::now(),
        });
        
        let engine = TrajectoryEngine::new(policy);
        
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed("My SSN is confidential"),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4"),
            parameters: None,
        };
        
        let result = engine.evaluate(&payload);
        assert_eq!(result.verdict, TraceVerdict::Block);
    }
    
    #[test]
    fn test_disabled_constraint_ignored() {
        let constraint = PolicyConstraint {
            id: uuid::Uuid::new_v4(),
            name: "Disabled Block".to_string(),
            constraint_type: ConstraintType::Keyword {
                patterns: vec!["blocked".to_string()],
                case_sensitive: false,
                target_field: TargetField::Prompt,
            },
            action: ConstraintAction::Block,
            priority: 1,
            enabled: false, // Disabled
        };
        
        let policy = Arc::new(CustomerPolicy {
            customer_id: CustomerId::new(),
            version: "1.0.0".to_string(),
            constraints: vec![constraint],
            default_verdict: TraceVerdict::Pass,
            updated_at: chrono::Utc::now(),
        });
        
        let engine = TrajectoryEngine::new(policy);
        
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed("This contains blocked text"),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4"),
            parameters: None,
        };
        
        let result = engine.evaluate(&payload);
        assert_eq!(result.verdict, TraceVerdict::Pass);
        assert!(result.triggered_constraints.is_empty());
    }
    
    #[test]
    fn test_priority_ordering() {
        let low_priority_constraint = PolicyConstraint {
            id: uuid::Uuid::new_v4(),
            name: "Log First".to_string(),
            constraint_type: ConstraintType::Keyword {
                patterns: vec!["test".to_string()],
                case_sensitive: false,
                target_field: TargetField::Prompt,
            },
            action: ConstraintAction::Log,
            priority: 10,
            enabled: true,
        };
        
        let high_priority_constraint = PolicyConstraint {
            id: uuid::Uuid::new_v4(),
            name: "Block First".to_string(),
            constraint_type: ConstraintType::Keyword {
                patterns: vec!["test".to_string()],
                case_sensitive: false,
                target_field: TargetField::Prompt,
            },
            action: ConstraintAction::Block,
            priority: 1,
            enabled: true,
        };
        
        let policy = Arc::new(CustomerPolicy {
            customer_id: CustomerId::new(),
            version: "1.0.0".to_string(),
            constraints: vec![low_priority_constraint.clone(), high_priority_constraint.clone()],
            default_verdict: TraceVerdict::Pass,
            updated_at: chrono::Utc::now(),
        });
        
        let engine = TrajectoryEngine::new(policy);
        
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed("test content"),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4"),
            parameters: None,
        };
        
        let result = engine.evaluate(&payload);
        // Should block because high priority (lower number) constraint is evaluated first
        assert_eq!(result.verdict, TraceVerdict::Block);
    }
    
    #[test]
    fn test_default_verdict_used() {
        let policy = Arc::new(CustomerPolicy {
            customer_id: CustomerId::new(),
            version: "1.0.0".to_string(),
            constraints: vec![], // No constraints
            default_verdict: TraceVerdict::Pass,
            updated_at: chrono::Utc::now(),
        });
        
        let engine = TrajectoryEngine::new(policy);
        
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed("Any content"),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4"),
            parameters: None,
        };
        
        let result = engine.evaluate(&payload);
        assert_eq!(result.verdict, TraceVerdict::Pass);
    }
    
    #[test]
    fn test_default_verdict_block() {
        let policy = Arc::new(CustomerPolicy {
            customer_id: CustomerId::new(),
            version: "1.0.0".to_string(),
            constraints: vec![],
            default_verdict: TraceVerdict::Block,
            updated_at: chrono::Utc::now(),
        });
        
        let engine = TrajectoryEngine::new(policy);
        
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed("Any content"),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4"),
            parameters: None,
        };
        
        let result = engine.evaluate(&payload);
        assert_eq!(result.verdict, TraceVerdict::Block);
    }
    
    #[test]
    fn test_modify_action_produces_modified_payload() {
        let constraint = PolicyConstraint {
            id: uuid::Uuid::new_v4(),
            name: "Modify Content".to_string(),
            constraint_type: ConstraintType::Keyword {
                patterns: vec!["modify-me".to_string()],
                case_sensitive: false,
                target_field: TargetField::Prompt,
            },
            action: ConstraintAction::Modify,
            priority: 1,
            enabled: true,
        };
        
        let policy = Arc::new(CustomerPolicy {
            customer_id: CustomerId::new(),
            version: "1.0.0".to_string(),
            constraints: vec![constraint],
            default_verdict: TraceVerdict::Pass,
            updated_at: chrono::Utc::now(),
        });
        
        let engine = TrajectoryEngine::new(policy);
        
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed("Please modify-me here"),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4"),
            parameters: None,
        };
        
        let result = engine.evaluate(&payload);
        assert_eq!(result.verdict, TraceVerdict::Modify);
        assert!(result.modified_payload.is_some());
        let modified = result.modified_payload.unwrap();
        assert!(modified.get("_trace_modified").unwrap().as_bool().unwrap());
    }
    
    #[test]
    fn test_empty_patterns_skipped() {
        let constraint = PolicyConstraint {
            id: uuid::Uuid::new_v4(),
            name: "Empty Patterns".to_string(),
            constraint_type: ConstraintType::Keyword {
                patterns: vec![], // Empty patterns
                case_sensitive: false,
                target_field: TargetField::Prompt,
            },
            action: ConstraintAction::Block,
            priority: 1,
            enabled: true,
        };
        
        let policy = Arc::new(CustomerPolicy {
            customer_id: CustomerId::new(),
            version: "1.0.0".to_string(),
            constraints: vec![constraint],
            default_verdict: TraceVerdict::Pass,
            updated_at: chrono::Utc::now(),
        });
        
        let engine = TrajectoryEngine::new(policy);
        
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed("Any content"),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4"),
            parameters: None,
        };
        
        let result = engine.evaluate(&payload);
        // Should pass since empty patterns are skipped during compilation
        assert_eq!(result.verdict, TraceVerdict::Pass);
    }
}

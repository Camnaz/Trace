//! Trajectory Engine implementation.
//!
//! Performs fast semantic evaluation of payloads against policy constraints.
//! Designed for sub-15ms evaluation using a hybrid approach:
//! - Fast path: Regex/keyword matching
//! - Vector cache: Embedding similarity
//! - Wasm sandbox: Complex user-defined logic

use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, trace, warn};

use crate::types::{
    ConstraintAction, ConstraintType, CustomerPolicy, EvaluationResult,
    IncomingPayload, PolicyConstraint, TargetField, TraceVerdict,
};

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
    Regex(regex::Regex),
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
                    
                    // Build regex from patterns
                    let pattern_str = keyword_patterns
                        .iter()
                        .map(|p| regex::escape(p))
                        .collect::<Vec<_>>()
                        .join("|");
                    
                    let regex_str = if *case_sensitive {
                        format!("({})", pattern_str)
                    } else {
                        format!("(?i)({})", pattern_str)
                    };
                    
                    match regex::Regex::new(&regex_str) {
                        Ok(re) => PatternMatcher::Regex(re),
                        Err(e) => {
                            warn!("Invalid regex for constraint {}: {}", constraint.id, e);
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
    pub async fn evaluate(&self, payload: &IncomingPayload<'_>) -> EvaluationResult {
        let mut triggered_constraints = Vec::new();
        let mut final_verdict = TraceVerdict::Pass;
        let mut explanation = None;
        
        // Check compiled patterns (fast path)
        for pattern in &self.compiled_patterns {
            let matched = match &pattern.matcher {
                PatternMatcher::Regex(re) => {
                    re.is_match(&payload.prompt)
                }
                PatternMatcher::Keyword(ac) => {
                    ac.find(&payload.prompt).is_some()
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
                        explanation = Some(format!(
                            "Blocked by constraint: {}", 
                            pattern.constraint_id
                        ));
                        break; // Stop evaluation on block
                    }
                    ConstraintAction::Log => {
                        // Just log, continue evaluation
                        if final_verdict == TraceVerdict::Pass {
                            final_verdict = TraceVerdict::Pass;
                        }
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
                    reference_embedding, 
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
                                explanation = Some(format!(
                                    "Vector similarity match: {:.2} >= {:.2}",
                                    similarity, threshold
                                ));
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
            if let ConstraintType::RateLimit { max_requests, window_seconds } = &constraint.constraint_type {
                // TODO: Implement rate limit check against shared counter
                // For now, placeholder - would use Redis or in-memory counter
                let current_count = 0_u32;
                
                if current_count >= *max_requests {
                    triggered_constraints.push(constraint.id);
                    final_verdict = TraceVerdict::Block;
                    explanation = Some(format!(
                        "Rate limit exceeded: {} requests per {} seconds",
                        max_requests, window_seconds
                    ));
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
    
    #[tokio::test]
    async fn test_passes_clean_payload() {
        let policy = Arc::new(create_test_policy());
        let engine = TrajectoryEngine::new(policy);
        
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed("Hello, how are you today?"),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4"),
            parameters: None,
        };
        
        let result = engine.evaluate(&payload).await;
        assert_eq!(result.verdict, TraceVerdict::Pass);
        assert!(result.triggered_constraints.is_empty());
    }
    
    #[tokio::test]
    async fn test_blocks_pii() {
        let policy = Arc::new(create_test_policy());
        let engine = TrajectoryEngine::new(policy);
        
        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed("My SSN is 123-45-6789"),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4"),
            parameters: None,
        };
        
        let result = engine.evaluate(&payload).await;
        assert_eq!(result.verdict, TraceVerdict::Block);
        assert!(!result.triggered_constraints.is_empty());
    }
    
    #[tokio::test]
    async fn test_blocks_long_prompt() {
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
        
        let result = engine.evaluate(&payload).await;
        assert_eq!(result.verdict, TraceVerdict::Block);
    }
}

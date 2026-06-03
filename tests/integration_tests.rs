//! Integration tests for the Trace proxy.
//!
//! These tests verify end-to-end functionality including:
//! - Policy evaluation with various constraints
//! - Request/response handling
//! - Store operations under load

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use stria_trace::types::*;
use stria_trace::policy::PolicyStore;
use stria_trace::engine::TrajectoryEngine;

/// Helper function to create a test payload
fn create_test_payload(prompt: &str) -> IncomingPayload {
    IncomingPayload {
        prompt: Cow::Borrowed(prompt),
        system: None,
        context: HashMap::new(),
        target_model: Cow::Borrowed("gpt-4"),
        parameters: None,
    }
}

/// Helper function to create a keyword constraint
fn create_keyword_constraint(
    name: &str,
    patterns: Vec<&str>,
    action: ConstraintAction,
    priority: u16,
) -> PolicyConstraint {
    PolicyConstraint {
        id: uuid::Uuid::new_v4(),
        name: name.to_string(),
        constraint_type: ConstraintType::Keyword {
            patterns: patterns.into_iter().map(String::from).collect(),
            case_sensitive: false,
            target_field: TargetField::Prompt,
        },
        action,
        priority,
        enabled: true,
    }
}

#[tokio::test]
async fn test_full_evaluation_flow_pass() {
    let customer_id = CustomerId::new();
    
    let policy = Arc::new(CustomerPolicy {
        customer_id,
        version: "1.0.0".to_string(),
        constraints: vec![
            create_keyword_constraint(
                "Block Bad Words",
                vec!["badword"],
                ConstraintAction::Block,
                1,
            ),
        ],
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    });
    
    let engine = TrajectoryEngine::new(policy);
    let payload = create_test_payload("This is a clean message");
    
    let result = engine.evaluate(&payload).await;
    
    assert_eq!(result.verdict, TraceVerdict::Pass);
    assert!(result.triggered_constraints.is_empty());
    assert!(result.explanation.is_none());
}

#[tokio::test]
async fn test_full_evaluation_flow_block() {
    let customer_id = CustomerId::new();
    
    let policy = Arc::new(CustomerPolicy {
        customer_id,
        version: "1.0.0".to_string(),
        constraints: vec![
            create_keyword_constraint(
                "Block Secrets",
                vec!["password", "secret", "key"],
                ConstraintAction::Block,
                1,
            ),
        ],
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    });
    
    let engine = TrajectoryEngine::new(policy);
    let payload = create_test_payload("My password is secret123");
    
    let result = engine.evaluate(&payload).await;
    
    assert_eq!(result.verdict, TraceVerdict::Block);
    assert!(!result.triggered_constraints.is_empty());
    assert!(result.explanation.is_some());
}

#[tokio::test]
async fn test_full_evaluation_flow_modify() {
    let customer_id = CustomerId::new();
    
    let policy = Arc::new(CustomerPolicy {
        customer_id,
        version: "1.0.0".to_string(),
        constraints: vec![
            create_keyword_constraint(
                "Flag Sensitive",
                vec!["sensitive"],
                ConstraintAction::Modify,
                1,
            ),
        ],
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    });
    
    let engine = TrajectoryEngine::new(policy);
    let payload = create_test_payload("This contains sensitive data");
    
    let result = engine.evaluate(&payload).await;
    
    assert_eq!(result.verdict, TraceVerdict::Modify);
    assert!(result.modified_payload.is_some());
}

#[tokio::test]
async fn test_policy_store_with_engine() {
    let store = PolicyStore::new();
    let customer_id = CustomerId::new();
    
    let policy = CustomerPolicy {
        customer_id,
        version: "1.0.0".to_string(),
        constraints: vec![
            create_keyword_constraint(
                "Block Test",
                vec!["test-block"],
                ConstraintAction::Block,
                1,
            ),
        ],
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    };
    
    store.set_policy(customer_id, policy);
    
    // Retrieve policy and evaluate
    let retrieved_policy = store.get_policy(customer_id).await
        .expect("Policy should exist");
    
    let engine = TrajectoryEngine::new(retrieved_policy);
    let payload = create_test_payload("This should test-block");
    
    let result = engine.evaluate(&payload).await;
    
    assert_eq!(result.verdict, TraceVerdict::Block);
}

#[tokio::test]
async fn test_multiple_constraints_mixed_actions() {
    let customer_id = CustomerId::new();
    
    let policy = Arc::new(CustomerPolicy {
        customer_id,
        version: "1.0.0".to_string(),
        constraints: vec![
            // Log constraint (low priority)
            create_keyword_constraint(
                "Log Pattern",
                vec!["pattern"],
                ConstraintAction::Log,
                10,
            ),
            // Block constraint (high priority)
            create_keyword_constraint(
                "Block Danger",
                vec!["danger"],
                ConstraintAction::Block,
                1,
            ),
            // Modify constraint (medium priority)
            create_keyword_constraint(
                "Modify Flag",
                vec!["flag"],
                ConstraintAction::Modify,
                5,
            ),
        ],
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    });
    
    let engine = TrajectoryEngine::new(policy);
    
    // Test 1: Only log pattern - should pass with log action
    let result1 = engine.evaluate(&create_test_payload("Has pattern")).await;
    assert_eq!(result1.verdict, TraceVerdict::Pass);
    
    // Test 2: Has danger - should block immediately
    let result2 = engine.evaluate(&create_test_payload("Has danger")).await;
    assert_eq!(result2.verdict, TraceVerdict::Block);
    
    // Test 3: Has flag but no danger - should modify
    let result3 = engine.evaluate(&create_test_payload("Has flag")).await;
    assert_eq!(result3.verdict, TraceVerdict::Modify);
}

#[tokio::test]
async fn test_policy_update_during_operation() {
    let store = PolicyStore::new();
    let customer_id = CustomerId::new();
    
    // Set initial policy
    let policy1 = CustomerPolicy {
        customer_id,
        version: "1.0.0".to_string(),
        constraints: vec![
            create_keyword_constraint(
                "Block Old",
                vec!["old-pattern"],
                ConstraintAction::Block,
                1,
            ),
        ],
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    };
    
    store.set_policy(customer_id, policy1);
    
    // Verify old policy works
    let policy = store.get_policy(customer_id).await.unwrap();
    let engine = TrajectoryEngine::new(policy);
    let result = engine.evaluate(&create_test_payload("Has old-pattern")).await;
    assert_eq!(result.verdict, TraceVerdict::Block);
    
    // Update policy
    let policy2 = CustomerPolicy {
        customer_id,
        version: "2.0.0".to_string(),
        constraints: vec![
            create_keyword_constraint(
                "Block New",
                vec!["new-pattern"],
                ConstraintAction::Block,
                1,
            ),
        ],
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    };
    
    store.set_policy(customer_id, policy2);
    
    // Verify new policy works
    let policy = store.get_policy(customer_id).await.unwrap();
    let engine = TrajectoryEngine::new(policy);
    
    // Old pattern should now pass
    let result1 = engine.evaluate(&create_test_payload("Has old-pattern")).await;
    assert_eq!(result1.verdict, TraceVerdict::Pass);
    
    // New pattern should block
    let result2 = engine.evaluate(&create_test_payload("Has new-pattern")).await;
    assert_eq!(result2.verdict, TraceVerdict::Block);
}

#[tokio::test]
async fn test_content_length_constraint() {
    let customer_id = CustomerId::new();
    
    let policy = Arc::new(CustomerPolicy {
        customer_id,
        version: "1.0.0".to_string(),
        constraints: vec![
            PolicyConstraint {
                id: uuid::Uuid::new_v4(),
                name: "Length Limit".to_string(),
                constraint_type: ConstraintType::ContentLength {
                    max_prompt_chars: 100,
                    max_prompt_tokens: None,
                },
                action: ConstraintAction::Block,
                priority: 1,
                enabled: true,
            },
        ],
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    });
    
    let engine = TrajectoryEngine::new(policy);
    
    // Short prompt should pass
    let short_payload = create_test_payload("Short");
    let result1 = engine.evaluate(&short_payload).await;
    assert_eq!(result1.verdict, TraceVerdict::Pass);
    
    // Long prompt should be blocked
    let long_text = "a".repeat(200);
    let long_payload = create_test_payload(&long_text);
    let result2 = engine.evaluate(&long_payload).await;
    assert_eq!(result2.verdict, TraceVerdict::Block);
}

#[tokio::test]
async fn test_concurrent_policy_access() {
    let store = PolicyStore::new();
    let customer_id = CustomerId::new();
    
    let policy = CustomerPolicy {
        customer_id,
        version: "1.0.0".to_string(),
        constraints: vec![
            create_keyword_constraint(
                "Block Test",
                vec!["concurrent"],
                ConstraintAction::Block,
                1,
            ),
        ],
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    };
    
    store.set_policy(customer_id, policy);
    
    // Spawn multiple concurrent evaluations
    let mut handles = vec![];
    
    for i in 0..20 {
        let store_clone = store.clone();
        let cid = customer_id;
        handles.push(tokio::spawn(async move {
            let payload_text = if i % 2 == 0 {
                "Has concurrent pattern"
            } else {
                "Clean message"
            };
            
            let payload = IncomingPayload {
                prompt: Cow::Borrowed(payload_text),
                system: None,
                context: HashMap::new(),
                target_model: Cow::Borrowed("gpt-4"),
                parameters: None,
            };
            
            let policy = store_clone.get_policy(cid).await.unwrap();
            let engine = TrajectoryEngine::new(policy);
            engine.evaluate(&payload).await
        }));
    }
    
    // Collect results
    let mut pass_count = 0;
    let mut block_count = 0;
    
    for handle in handles {
        let result = handle.await.unwrap();
        match result.verdict {
            TraceVerdict::Pass => pass_count += 1,
            TraceVerdict::Block => block_count += 1,
            _ => {}
        }
    }
    
    // 10 even indices should block, 10 odd indices should pass
    assert_eq!(pass_count, 10);
    assert_eq!(block_count, 10);
}

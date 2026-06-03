//! Criterion benchmarks for the Trajectory Evaluation Engine.
//!
//! Run with: `cargo bench`

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::sync::Arc;

use stria_trace::engine::TrajectoryEngine;
use stria_trace::types::{
    ConstraintAction, ConstraintType, CustomerId, CustomerPolicy, IncomingPayload,
    PolicyConstraint, TargetField, TraceVerdict,
};

fn build_policy(constraints: usize) -> CustomerPolicy {
    let mut cs = Vec::with_capacity(constraints);
    for i in 0..constraints {
        cs.push(PolicyConstraint {
            id: uuid::Uuid::new_v4(),
            name: format!("Block term {}", i),
            constraint_type: ConstraintType::Keyword {
                patterns: vec![format!("forbidden{}", i)],
                case_sensitive: false,
                target_field: TargetField::Prompt,
            },
            action: ConstraintAction::Block,
            priority: i as u16,
            enabled: true,
        });
    }
    CustomerPolicy {
        customer_id: CustomerId::new(),
        version: "1.0.0".to_string(),
        constraints: cs,
        default_verdict: TraceVerdict::Pass,
        updated_at: chrono::Utc::now(),
    }
}

fn benchmark_engine(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("engine_evaluate");

    for size in [1, 4, 8, 16, 32] {
        let policy = Arc::new(build_policy(size));
        let engine = TrajectoryEngine::new(policy);

        let payload = IncomingPayload {
            prompt: std::borrow::Cow::Borrowed(
                "This is a benign prompt about quarterly earnings and portfolio diversification."
            ),
            system: None,
            context: std::collections::HashMap::new(),
            target_model: std::borrow::Cow::Borrowed("gpt-4o"),
            parameters: None,
        };

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.to_async(&rt).iter(|| async {
                let _ = engine.evaluate(&payload).await;
            });
        });
    }

    group.finish();
}

criterion_group!(benches, benchmark_engine);
criterion_main!(benches);

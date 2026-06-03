//! Core data structures for the Trace proxy.
//! 
//! This module defines the types used throughout the request lifecycle.
//! Design goals:
//! - Zero-allocation where possible using references and Cow
//! - Serde-compatible for fast JSON serialization/deserialization
//! - Clone-on-write semantics for efficient payload transformation

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use uuid::Uuid;

/// Unique identifier for a customer/tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CustomerId(pub Uuid);

impl CustomerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CustomerId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CustomerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Multi-tenant organization identifier.
///
/// In the high-scale shared-process model every tenant data-space is keyed by
/// an `OrgId`. It is a transparent alias of [`CustomerId`] so existing routing
/// and storage continue to work unchanged while the public vocabulary moves to
/// "organization".
pub type OrgId = CustomerId;

/// Commercial tier for an organization.
///
/// Tiers gate operational limits — concurrency, synthetic-probe budget, and how
/// aggressively the background training shell re-tunes rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BillingTier {
    /// Entry tier — conservative limits.
    #[default]
    Starter,
    /// Mid-market growth tier.
    Growth,
    /// Institutional / regulated tier — highest limits.
    Enterprise,
}

impl BillingTier {
    /// Maximum concurrent in-flight evaluations permitted for this tier.
    pub fn max_concurrency(&self) -> u32 {
        match self {
            BillingTier::Starter => 64,
            BillingTier::Growth => 256,
            BillingTier::Enterprise => 1024,
        }
    }

    /// Synthetic adversarial probe budget per stress/verification run.
    pub fn probe_budget(&self) -> usize {
        match self {
            BillingTier::Starter => 8,
            BillingTier::Growth => 16,
            BillingTier::Enterprise => 32,
        }
    }
}

/// Per-tenant context resolved from request credentials.
///
/// Holds the organization id, display name, billing tier, and operational
/// limits. Resolved once per request and handed to the platform store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationContext {
    /// The tenant key.
    pub org_id: OrgId,
    /// Human-readable organization name (never a UUID in the UI).
    pub display_name: String,
    /// Commercial tier governing limits.
    #[serde(default)]
    pub tier: BillingTier,
    /// When this organization was onboarded.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl OrganizationContext {
    /// Construct a context with sensible defaults for a freshly-seen org.
    pub fn new(org_id: OrgId, display_name: impl Into<String>, tier: BillingTier) -> Self {
        Self {
            org_id,
            display_name: display_name.into(),
            tier,
            created_at: chrono::Utc::now(),
        }
    }
}

/// A natural-language boundary submitted by an administrator through the
/// training shell, e.g. *"Filter out options trading recommendations outside
/// the scope of work."*
///
/// The training shell compiles directives into concrete [`PolicyConstraint`]s
/// and hardens the match rules through the simulation loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryDirective {
    /// Organization this boundary belongs to.
    pub org_id: OrgId,
    /// The raw natural-language instruction from the administrator.
    pub text: String,
    /// What the proxy should do when the boundary is tripped.
    #[serde(default)]
    pub action: ConstraintAction,
}

/// Unique identifier for a request (for tracing and observability).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub Uuid);

impl RequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

/// The incoming payload from the client.
/// 
/// Uses `Cow<str>` to enable zero-copy deserialization when possible,
/// while allowing owned strings when modification is required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingPayload<'a> {
    /// The user prompt to be evaluated.
    #[serde(borrow)]
    pub prompt: Cow<'a, str>,
    
    /// Optional system message/context.
    #[serde(borrow)]
    #[serde(default)]
    pub system: Option<Cow<'a, str>>,
    
    /// Customer-specific context (model preferences, metadata, etc.)
    #[serde(borrow)]
    #[serde(default)]
    pub context: HashMap<Cow<'a, str>, Cow<'a, str>>,
    
    /// The target model/endpoint identifier.
    #[serde(borrow)]
    pub target_model: Cow<'a, str>,
    
    /// Request parameters (temperature, max_tokens, etc.)
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
}

impl<'a> IncomingPayload<'a> {
    /// Convert to an owned version (useful when the borrow lifetime expires).
    pub fn into_owned(self) -> IncomingPayload<'static> {
        IncomingPayload {
            prompt: Cow::Owned(self.prompt.into_owned()),
            system: self.system.map(|s| Cow::Owned(s.into_owned())),
            context: self.context
                .into_iter()
                .map(|(k, v)| (Cow::Owned(k.into_owned()), Cow::Owned(v.into_owned())))
                .collect(),
            target_model: Cow::Owned(self.target_model.into_owned()),
            parameters: self.parameters,
        }
    }
    
    /// Get a fingerprint of the prompt for caching purposes.
    pub fn prompt_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        self.prompt.hash(&mut hasher);
        hasher.finish()
    }
}

/// A constraint/rule that incoming payloads are evaluated against.
/// 
/// Constraints can be of various types: keyword-based, vector similarity,
/// or complex logic via embedded patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConstraint {
    /// Unique identifier for this constraint.
    pub id: Uuid,
    
    /// Human-readable name for the constraint.
    pub name: String,
    
    /// The type of constraint and its specific configuration.
    #[serde(flatten)]
    pub constraint_type: ConstraintType,
    
    /// The action to take when this constraint matches.
    pub action: ConstraintAction,
    
    /// Priority for evaluation order (lower = evaluated first).
    pub priority: u16,
    
    /// Whether this constraint is currently active.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// The specific type and configuration of a constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConstraintType {
    /// Keyword/pattern matching using regex.
    Keyword {
        /// List of patterns to match (OR logic within list).
        patterns: Vec<String>,
        /// Whether matching is case-sensitive.
        #[serde(default = "default_false")]
        case_sensitive: bool,
        /// Match against specific field (prompt, system, or both).
        #[serde(default)]
        target_field: TargetField,
    },
    
    /// Vector similarity matching against pre-computed embeddings.
    VectorSimilarity {
        /// The reference embedding to compare against.
        reference_embedding: Vec<f32>,
        /// Similarity threshold (0.0 - 1.0, where 1.0 is identical).
        threshold: f32,
        /// The embedding model used for comparison.
        model: String,
    },
    
    /// Rate limiting constraint.
    RateLimit {
        /// Maximum requests per window.
        max_requests: u32,
        /// Time window in seconds.
        window_seconds: u64,
    },
    
    /// Content length constraints.
    ContentLength {
        /// Maximum prompt length in characters.
        max_prompt_chars: usize,
        /// Maximum prompt length in tokens (approximate).
        max_prompt_tokens: Option<usize>,
    },
}

fn default_false() -> bool {
    false
}

/// Which field(s) to target for constraint evaluation.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetField {
    #[default]
    Prompt,
    System,
    Both,
}

/// The action to take when a constraint matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintAction {
    /// Block the request entirely.
    Block,
    
    /// Allow but log for review.
    #[default]
    Log,
    
    /// Modify the payload before forwarding.
    Modify,
}

/// The complete set of policies for a customer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerPolicy {
    /// The customer this policy belongs to.
    pub customer_id: CustomerId,
    
    /// Semantic version of the policy.
    pub version: String,
    
    /// List of active constraints.
    pub constraints: Vec<PolicyConstraint>,
    
    /// Default verdict if no constraints match.
    #[serde(default)]
    pub default_verdict: TraceVerdict,
    
    /// When this policy was last updated.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// The verdict returned by the Trajectory Engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TraceVerdict {
    /// Pass the request through unmodified.
    #[default]
    Pass,
    
    /// Block the request; do not forward to upstream.
    Block,
    
    /// Modify the request before forwarding.
    Modify,
}

/// The result of evaluating a payload against customer policies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    /// The final verdict.
    pub verdict: TraceVerdict,
    
    /// The constraint(s) that triggered the verdict (if any).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggered_constraints: Vec<Uuid>,
    
    /// Human-readable explanation of the verdict.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    
    /// Modified payload (only present if verdict is Modify).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_payload: Option<serde_json::Value>,
    
    /// Evaluation latency in microseconds.
    pub latency_us: u64,
}

/// Response sent back to the client when a request is blocked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockResponse {
    pub request_id: RequestId,
    pub blocked: bool,
    pub reason: String,
    pub triggered_constraint: Option<Uuid>,
}

/// Configuration for the proxy server.
#[derive(Debug, Clone, Deserialize)]
pub struct ProxyConfig {
    /// Address to bind the server to.
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
    
    /// Port to listen on.
    #[serde(default = "default_port")]
    pub port: u16,
    
    /// Upstream LLM endpoint URL.
    pub upstream_url: String,
    
    /// Maximum request body size in bytes.
    #[serde(default = "default_max_body_size")]
    pub max_body_size: usize,
    
    /// Request timeout in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    
    /// Maximum evaluation latency allowed (microseconds).
    #[serde(default = "default_max_eval_us")]
    pub max_evaluation_micros: u64,
}

fn default_bind_address() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_max_body_size() -> usize {
    1024 * 1024 // 1MB
}

fn default_timeout_ms() -> u64 {
    30000 // 30 seconds
}

fn default_max_eval_us() -> u64 {
    15000 // 15ms
}

/// Internal request context passed through the pipeline.
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub request_id: RequestId,
    pub customer_id: CustomerId,
    pub start_time: std::time::Instant,
}

impl RequestContext {
    pub fn new(customer_id: CustomerId) -> Self {
        Self {
            request_id: RequestId::new(),
            customer_id,
            start_time: std::time::Instant::now(),
        }
    }
    
    /// Get elapsed time since request started.
    pub fn elapsed_micros(&self) -> u64 {
        self.start_time.elapsed().as_micros() as u64
    }
}

/// A pre-allocated buffer for streaming request bodies.
pub type BodyBuffer = bytes::BytesMut;

/// A telemetry event emitted after every evaluated request.
///
/// Broadcast over a tokio channel so the SSE endpoint and any future
/// consumers can subscribe without blocking the proxy hot-path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    /// The request that was evaluated.
    pub request_id: RequestId,
    /// The customer the request belonged to.
    pub customer_id: CustomerId,
    /// Final verdict.
    pub verdict: TraceVerdict,
    /// Constraints that fired (may be empty).
    pub triggered_constraints: Vec<uuid::Uuid>,
    /// Human-readable explanation when a constraint fired.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    /// Total proxy latency in microseconds (parse + eval + verdict).
    pub total_latency_us: u64,
    /// Engine-only evaluation latency in microseconds.
    pub eval_latency_us: u64,
    /// UTC timestamp of the event.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Error types that can occur during request processing.
#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    
    #[error("Payload too large: {size} bytes (max: {max})")]
    PayloadTooLarge { size: usize, max: usize },
    
    #[error("Evaluation timeout after {0} microseconds")]
    EvaluationTimeout(u64),
    
    #[error("Policy not found for customer: {0}")]
    PolicyNotFound(CustomerId),
    
    #[error("Upstream error: {0}")]
    UpstreamError(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use std::collections::HashMap;

    #[test]
    fn test_customer_id_new() {
        let id1 = CustomerId::new();
        let id2 = CustomerId::new();
        assert_ne!(id1.0, id2.0, "Each CustomerId should be unique");
    }

    #[test]
    fn test_customer_id_default() {
        let id: CustomerId = Default::default();
        // Should not panic and should create a valid UUID
        assert!(!id.0.to_string().is_empty());
    }

    #[test]
    fn test_request_id_new() {
        let id1 = RequestId::new();
        let id2 = RequestId::new();
        assert_ne!(id1.0, id2.0, "Each RequestId should be unique");
    }

    #[test]
    fn test_incoming_payload_serialization() {
        let payload = IncomingPayload {
            prompt: Cow::Borrowed("Hello world"),
            system: Some(Cow::Borrowed("You are helpful")),
            context: {
                let mut ctx = HashMap::new();
                ctx.insert(Cow::Borrowed("key"), Cow::Borrowed("value"));
                ctx
            },
            target_model: Cow::Borrowed("gpt-4"),
            parameters: Some(serde_json::json!({"temperature": 0.7})),
        };

        let json = serde_json::to_string(&payload).expect("Should serialize");
        assert!(json.contains("Hello world"));
        assert!(json.contains("gpt-4"));
        assert!(json.contains("temperature"));
    }

    #[test]
    fn test_incoming_payload_deserialization() {
        let json = r#"{
            "prompt": "Test prompt",
            "target_model": "gpt-3.5",
            "system": "Be concise"
        }"#;

        let payload: IncomingPayload = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(payload.prompt, "Test prompt");
        assert_eq!(payload.target_model, "gpt-3.5");
        assert_eq!(payload.system.as_deref(), Some("Be concise"));
    }

    #[test]
    fn test_incoming_payload_into_owned() {
        let payload = IncomingPayload {
            prompt: Cow::Borrowed("borrowed"),
            system: Some(Cow::Borrowed("system msg")),
            context: HashMap::new(),
            target_model: Cow::Borrowed("model"),
            parameters: None,
        };

        let owned: IncomingPayload<'static> = payload.into_owned();
        assert_eq!(owned.prompt, "borrowed");
        assert_eq!(owned.system.as_deref(), Some("system msg"));
    }

    #[test]
    fn test_incoming_payload_prompt_hash_consistency() {
        let payload1 = IncomingPayload {
            prompt: Cow::Borrowed("same prompt"),
            system: None,
            context: HashMap::new(),
            target_model: Cow::Borrowed("gpt-4"),
            parameters: None,
        };

        let payload2 = IncomingPayload {
            prompt: Cow::Borrowed("same prompt"),
            system: None,
            context: HashMap::new(),
            target_model: Cow::Borrowed("gpt-4"),
            parameters: None,
        };

        assert_eq!(payload1.prompt_hash(), payload2.prompt_hash());
    }

    #[test]
    fn test_incoming_payload_prompt_hash_different() {
        let payload1 = IncomingPayload {
            prompt: Cow::Borrowed("prompt one"),
            system: None,
            context: HashMap::new(),
            target_model: Cow::Borrowed("gpt-4"),
            parameters: None,
        };

        let payload2 = IncomingPayload {
            prompt: Cow::Borrowed("prompt two"),
            system: None,
            context: HashMap::new(),
            target_model: Cow::Borrowed("gpt-4"),
            parameters: None,
        };

        assert_ne!(payload1.prompt_hash(), payload2.prompt_hash());
    }

    #[test]
    fn test_policy_constraint_serialization() {
        let constraint = PolicyConstraint {
            id: uuid::Uuid::new_v4(),
            name: "Test Constraint".to_string(),
            constraint_type: ConstraintType::Keyword {
                patterns: vec!["pattern1".to_string(), "pattern2".to_string()],
                case_sensitive: false,
                target_field: TargetField::Prompt,
            },
            action: ConstraintAction::Block,
            priority: 1,
            enabled: true,
        };

        let json = serde_json::to_string(&constraint).expect("Should serialize");
        assert!(json.contains("Test Constraint"));
        assert!(json.contains("keyword"));
    }

    #[test]
    fn test_constraint_type_keyword_deserialization() {
        let json = r#"{
            "type": "keyword",
            "patterns": ["test", "pattern"],
            "case_sensitive": true,
            "target_field": "system"
        }"#;

        let constraint: ConstraintType = serde_json::from_str(json).expect("Should deserialize");
        match constraint {
            ConstraintType::Keyword { patterns, case_sensitive, target_field } => {
                assert_eq!(patterns, vec!["test", "pattern"]);
                assert!(case_sensitive);
                assert!(matches!(target_field, TargetField::System));
            }
            _ => panic!("Expected Keyword constraint type"),
        }
    }

    #[test]
    fn test_constraint_type_vector_similarity_deserialization() {
        let json = r#"{
            "type": "vector_similarity",
            "reference_embedding": [0.1, 0.2, 0.3],
            "threshold": 0.85,
            "model": "text-embedding-ada-002"
        }"#;

        let constraint: ConstraintType = serde_json::from_str(json).expect("Should deserialize");
        match constraint {
            ConstraintType::VectorSimilarity { reference_embedding, threshold, model } => {
                assert_eq!(reference_embedding, vec![0.1, 0.2, 0.3]);
                assert_eq!(threshold, 0.85);
                assert_eq!(model, "text-embedding-ada-002");
            }
            _ => panic!("Expected VectorSimilarity constraint type"),
        }
    }

    #[test]
    fn test_trace_verdict_default() {
        let verdict: TraceVerdict = Default::default();
        assert!(matches!(verdict, TraceVerdict::Pass));
    }

    #[test]
    fn test_trace_verdict_serialization() {
        assert_eq!(
            serde_json::to_string(&TraceVerdict::Pass).unwrap(),
            "\"pass\""
        );
        assert_eq!(
            serde_json::to_string(&TraceVerdict::Block).unwrap(),
            "\"block\""
        );
        assert_eq!(
            serde_json::to_string(&TraceVerdict::Modify).unwrap(),
            "\"modify\""
        );
    }

    #[test]
    fn test_evaluation_result_serialization() {
        let result = EvaluationResult {
            verdict: TraceVerdict::Block,
            triggered_constraints: vec![uuid::Uuid::new_v4()],
            explanation: Some("Test explanation".to_string()),
            modified_payload: None,
            latency_us: 1000,
        };

        let json = serde_json::to_string(&result).expect("Should serialize");
        assert!(json.contains("block"));
        assert!(json.contains("Test explanation"));
    }

    #[test]
    fn test_block_response_serialization() {
        let response = BlockResponse {
            request_id: RequestId::new(),
            blocked: true,
            reason: "Policy violation".to_string(),
            triggered_constraint: Some(uuid::Uuid::new_v4()),
        };

        let json = serde_json::to_string(&response).expect("Should serialize");
        assert!(json.contains("true"));
        assert!(json.contains("Policy violation"));
    }

    #[test]
    fn test_proxy_config_defaults() {
        let config = ProxyConfig {
            bind_address: default_bind_address(),
            port: default_port(),
            upstream_url: "http://example.com".to_string(),
            max_body_size: default_max_body_size(),
            timeout_ms: default_timeout_ms(),
            max_evaluation_micros: default_max_eval_us(),
        };

        assert_eq!(config.bind_address, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.max_body_size, 1024 * 1024);
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_evaluation_micros, 15000);
    }

    #[test]
    fn test_request_context_new() {
        let customer_id = CustomerId::new();
        let ctx = RequestContext::new(customer_id);

        assert_eq!(ctx.customer_id.0, customer_id.0);
        // Request ID should be unique
        assert!(!ctx.request_id.0.to_string().is_empty());
        // Elapsed time should be very small (just created)
        assert!(ctx.elapsed_micros() < 1000);
    }

    #[test]
    fn test_target_field_default() {
        let field: TargetField = Default::default();
        assert!(matches!(field, TargetField::Prompt));
    }

    #[test]
    fn test_constraint_action_default() {
        let action: ConstraintAction = Default::default();
        assert!(matches!(action, ConstraintAction::Log));
    }

    #[test]
    fn test_trace_error_display() {
        let err1 = TraceError::InvalidRequest("bad input".to_string());
        assert!(err1.to_string().contains("bad input"));

        let err2 = TraceError::PayloadTooLarge { size: 2000000, max: 1000000 };
        assert!(err2.to_string().contains("2000000"));

        let err3 = TraceError::EvaluationTimeout(15000);
        assert!(err3.to_string().contains("15000"));
    }
}

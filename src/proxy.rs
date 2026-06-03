//! HTTP proxy logic for handling ingress and egress requests.
//!
//! This module implements the core request flow:
//! 1. Receive incoming request
//! 2. Parse and validate payload
//! 3. Evaluate against customer policies
//! 4. Forward (modified or unmodified) to upstream LLM
//! 5. Stream response back to client

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use bytes::Bytes;
use std::time::Instant;
use tracing::{error, info, instrument, warn};

use crate::engine::TrajectoryEngine;
use crate::AppState;
use crate::types::{
    BlockResponse, CustomerId, EvaluationResult, IncomingPayload, RequestContext,
    TelemetryEvent, TraceError, TraceVerdict,
};

/// Header name for customer identification.
const HEADER_CUSTOMER_ID: &str = "x-customer-id";
/// Header name for request tracing.
#[allow(dead_code)]
const HEADER_REQUEST_ID: &str = "x-request-id";

/// Zero-allocation, platform-agnostic payload extraction contract.
///
/// Trace ingests raw request bodies destined for *any* model provider. Rather
/// than branching on provider throughout the hot path, every supported wire
/// shape implements `ModelPayloadAgnostic`, giving the evaluator a single,
/// borrowed view. Implementations return [`Cow`] so the common case (Trace
/// native, or a single-string field) borrows directly with **no allocation**;
/// only shapes that require stitching multiple fields (e.g. multi-message
/// chat transcripts) fall back to an owned string.
pub trait ModelPayloadAgnostic {
    /// The primary user content to be evaluated.
    fn extract_prompt(&self) -> std::borrow::Cow<'_, str>;
    /// Optional system / developer instruction, if present.
    fn extract_system(&self) -> Option<std::borrow::Cow<'_, str>>;
    /// Target model identifier.
    fn extract_model(&self) -> std::borrow::Cow<'_, str>;
}

/// Native Trace payload — already normalized, so every field borrows.
impl<'p> ModelPayloadAgnostic for IncomingPayload<'p> {
    #[inline]
    fn extract_prompt(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Borrowed(self.prompt.as_ref())
    }
    #[inline]
    fn extract_system(&self) -> Option<std::borrow::Cow<'_, str>> {
        self.system.as_ref().map(|s| std::borrow::Cow::Borrowed(s.as_ref()))
    }
    #[inline]
    fn extract_model(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Borrowed(self.target_model.as_ref())
    }
}

/// The wire shape detected for an inbound payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadShape {
    /// Trace-native `{ prompt, system, target_model }`.
    Native,
    /// OpenAI Chat Completions `{ model, messages: [{role, content}] }`.
    OpenAiChat,
    /// Anthropic Messages `{ model, system, messages: [{role, content}] }`.
    AnthropicMessages,
}

/// A borrowed, normalized view over a parsed JSON body that adapts the three
/// supported provider shapes to the [`ModelPayloadAgnostic`] contract without
/// copying the underlying buffer. The only allocation occurs when a chat
/// transcript must be concatenated into a single evaluable prompt.
pub struct AgnosticView<'v> {
    value: &'v serde_json::Value,
    shape: PayloadShape,
}

impl<'v> AgnosticView<'v> {
    /// Detect the provider shape of an already-parsed JSON body.
    pub fn detect(value: &'v serde_json::Value) -> Self {
        let shape = if value.get("prompt").is_some() && value.get("target_model").is_some() {
            PayloadShape::Native
        } else if value.get("messages").is_some() {
            // Anthropic carries a top-level `system`; OpenAI puts it in messages.
            if value.get("system").is_some() {
                PayloadShape::AnthropicMessages
            } else {
                PayloadShape::OpenAiChat
            }
        } else {
            PayloadShape::Native
        };
        Self { value, shape }
    }

    /// The detected wire shape.
    #[inline]
    pub fn shape(&self) -> PayloadShape {
        self.shape
    }

    /// Concatenate the user-role message contents from a chat transcript.
    fn join_user_messages(&self) -> String {
        let mut out = String::new();
        if let Some(arr) = self.value.get("messages").and_then(|m| m.as_array()) {
            for msg in arr {
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
                if role == "user" {
                    if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(content);
                    }
                }
            }
        }
        out
    }
}

impl<'v> ModelPayloadAgnostic for AgnosticView<'v> {
    fn extract_prompt(&self) -> std::borrow::Cow<'_, str> {
        match self.shape {
            PayloadShape::Native => self
                .value
                .get("prompt")
                .and_then(|p| p.as_str())
                .map(std::borrow::Cow::Borrowed)
                .unwrap_or(std::borrow::Cow::Borrowed("")),
            // Chat shapes require stitching → owned, but only here.
            PayloadShape::OpenAiChat | PayloadShape::AnthropicMessages => {
                std::borrow::Cow::Owned(self.join_user_messages())
            }
        }
    }

    fn extract_system(&self) -> Option<std::borrow::Cow<'_, str>> {
        match self.shape {
            PayloadShape::Native | PayloadShape::AnthropicMessages => self
                .value
                .get("system")
                .and_then(|s| s.as_str())
                .map(std::borrow::Cow::Borrowed),
            PayloadShape::OpenAiChat => {
                // OpenAI carries the system instruction as a `system`-role message.
                self.value
                    .get("messages")
                    .and_then(|m| m.as_array())
                    .and_then(|arr| {
                        arr.iter().find(|msg| {
                            msg.get("role").and_then(|r| r.as_str()) == Some("system")
                        })
                    })
                    .and_then(|msg| msg.get("content").and_then(|c| c.as_str()))
                    .map(std::borrow::Cow::Borrowed)
            }
        }
    }

    fn extract_model(&self) -> std::borrow::Cow<'_, str> {
        let key = if self.shape == PayloadShape::Native { "target_model" } else { "model" };
        self.value
            .get(key)
            .and_then(|m| m.as_str())
            .map(std::borrow::Cow::Borrowed)
            .unwrap_or(std::borrow::Cow::Borrowed("unknown"))
    }
}

/// Main handler for proxy requests.
#[instrument(skip(state, body))]
pub async fn handle_proxy_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let start = Instant::now();
    
    // Step 1: Extract customer context
    let customer_id = match extract_customer_id(&headers) {
        Ok(id) => id,
        Err(e) => {
            warn!("Failed to extract customer ID: {}", e);
            return error_response(StatusCode::BAD_REQUEST, &e.to_string());
        }
    };
    
    let request_ctx = RequestContext::new(customer_id);
    info!(request_id = %request_ctx.request_id.0, "Processing request");
    
    // Step 2: Read and parse request body
    let body_bytes = match read_body(body, state.config.max_body_size).await {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to read body: {}", e);
            return error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Request body too large"
            );
        }
    };
    
    // Step 3: Deserialize payload
    let payload: IncomingPayload = match serde_json::from_slice(&body_bytes) {
        Ok(p) => p,
        Err(e) => {
            warn!("Failed to deserialize payload: {}", e);
            return error_response(
                StatusCode::BAD_REQUEST,
                &format!("Invalid JSON payload: {}", e)
            );
        }
    };
    
    let parse_latency = start.elapsed().as_micros() as u64;
    info!(latency_us = parse_latency, "Payload parsed");

    // Step 4: Evaluate against customer policies
    let evaluation_result = match evaluate_payload(&state, customer_id, &payload).await {
        Ok(result) => result,
        Err(e) => {
            error!("Evaluation failed: {}", e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Policy evaluation failed"
            );
        }
    };
    
    // Step 5: Apply verdict
    let response = match evaluation_result.verdict {
        TraceVerdict::Pass => {
            info!("Verdict: PASS - Forwarding to upstream");
            forward_to_upstream(&state, &headers, body_bytes.clone()).await
        }
        TraceVerdict::Block => {
            info!("Verdict: BLOCK - Returning block response");
            block_response(&request_ctx, &evaluation_result)
        }
        TraceVerdict::Modify => {
            info!("Verdict: MODIFY - Applying transformations");
            match &evaluation_result.modified_payload {
                Some(modified) => {
                    let modified_bytes = match serde_json::to_vec(modified) {
                        Ok(b) => Bytes::from(b),
                        Err(e) => {
                            error!("Failed to serialize modified payload: {}", e);
                            return error_response(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Modification failed"
                            );
                        }
                    };
                    forward_to_upstream(&state, &headers, modified_bytes).await
                }
                None => {
                    warn!("Modify verdict but no modified payload present");
                    forward_to_upstream(&state, &headers, body_bytes.clone()).await
                }
            }
        }
    };
    
    let total_latency = start.elapsed().as_micros() as u64;
    info!(
        verdict = ?evaluation_result.verdict,
        total_latency_us = total_latency,
        evaluation_latency_us = evaluation_result.latency_us,
        "Request complete"
    );

    // Record metrics (lock-free atomics — never blocks)
    state.metrics.record_request(&evaluation_result.verdict);
    state.metrics.eval_latency_us.observe(evaluation_result.latency_us);
    state.metrics.total_latency_us.observe(total_latency);

    // Emit telemetry asynchronously — lagging subscribers are dropped, never block.
    let event = TelemetryEvent {
        request_id: request_ctx.request_id,
        customer_id,
        verdict: evaluation_result.verdict,
        triggered_constraints: evaluation_result.triggered_constraints.clone(),
        explanation: evaluation_result.explanation.clone(),
        total_latency_us: total_latency,
        eval_latency_us: evaluation_result.latency_us,
        timestamp: chrono::Utc::now(),
    };
    // send() only errors when there are zero subscribers — that's fine.
    let _ = state.telemetry_tx.send(event);

    // Capture the request into the per-org training corpus. Retained until the
    // next verified Git sync, this data feeds future engine upgrades.
    // Allocation is deferred to after the upstream response path.
    state.shell.corpus().capture(
        customer_id,
        payload.prompt.into_owned(),
        evaluation_result.verdict,
    );

    response
}

/// Extract customer ID from request headers.
fn extract_customer_id(headers: &HeaderMap) -> Result<CustomerId, TraceError> {
    let header_value = headers
        .get(HEADER_CUSTOMER_ID)
        .ok_or_else(|| TraceError::InvalidRequest(
            format!("Missing required header: {}", HEADER_CUSTOMER_ID)
        ))?;
    
    let id_str = header_value
        .to_str()
        .map_err(|_| TraceError::InvalidRequest(
            "Invalid customer ID format".to_string()
        ))?;
    
    let uuid = uuid::Uuid::parse_str(id_str)
        .map_err(|_| TraceError::InvalidRequest(
            format!("Invalid UUID: {}", id_str)
        ))?;
    
    Ok(CustomerId(uuid))
}

/// Read the request body into a Bytes buffer.
async fn read_body(
    body: Body,
    max_size: usize,
) -> Result<Bytes, Box<dyn std::error::Error>> {
    use http_body_util::BodyExt;
    
    let collected = body.collect().await?;
    let bytes = collected.to_bytes();
    
    if bytes.len() > max_size {
        return Err("Body exceeds maximum size".into());
    }
    
    Ok(bytes)
}

/// Evaluate the payload against customer policies.
async fn evaluate_payload(
    state: &AppState,
    customer_id: CustomerId,
    payload: &IncomingPayload<'_>,
) -> Result<EvaluationResult, TraceError> {
    // Get customer policy
    let policy = state.policy_store.get_policy(customer_id).await
        .ok_or(TraceError::PolicyNotFound(customer_id))?;
    
    // Initialize the trajectory engine
    let engine = TrajectoryEngine::new(policy);
    
    // Perform evaluation with timeout
    let eval_start = Instant::now();
    
    let result = engine.evaluate(payload);
    
    let eval_latency = eval_start.elapsed().as_micros() as u64;
    
    // Check if evaluation exceeded the budget
    if eval_latency > state.config.max_evaluation_micros {
        warn!(
            eval_latency_us = eval_latency,
            max_latency_us = state.config.max_evaluation_micros,
            "Evaluation exceeded latency budget"
        );
    }
    
    Ok(EvaluationResult {
        verdict: result.verdict,
        triggered_constraints: result.triggered_constraints,
        explanation: result.explanation,
        modified_payload: result.modified_payload,
        latency_us: eval_latency,
    })
}

/// Forward the request to the upstream LLM endpoint.
async fn forward_to_upstream(
    state: &AppState,
    headers: &HeaderMap,
    body: Bytes,
) -> Response {
    // Build the upstream request
    let mut upstream_request = state.http_client
        .post(&state.config.upstream_url)
        .body(body);
    
    // Forward relevant headers
    for (name, value) in headers.iter() {
        let name_str = name.as_str();
        // Skip hop-by-hop headers
        if !is_hop_by_hop_header(name_str) {
            if let Ok(name) = reqwest::header::HeaderName::from_bytes(name_str.as_bytes()) {
                if let Ok(value) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) {
                    upstream_request = upstream_request.header(name, value);
                }
            }
        }
    }
    
    // Send request to upstream
    match upstream_request.send().await {
        Ok(upstream_response) => {
            let status = upstream_response.status();
            let headers = upstream_response.headers().clone();
            
            // Get response body
            match upstream_response.bytes().await {
                Ok(body) => {
                    let mut response = Response::new(Body::from(body));
                    *response.status_mut() = StatusCode::from_u16(status.as_u16())
                        .unwrap_or(StatusCode::OK);
                    // Convert headers
                    for (name, value) in headers.iter() {
                        if let Ok(name) = header::HeaderName::from_bytes(name.as_str().as_bytes()) {
                            if let Ok(val) = header::HeaderValue::from_bytes(value.as_bytes()) {
                                response.headers_mut().insert(name, val);
                            }
                        }
                    }
                    response
                }
                Err(e) => {
                    error!("Failed to read upstream response body: {}", e);
                    state.metrics.upstream_errors.inc();
                    error_response(
                        StatusCode::BAD_GATEWAY,
                        "Failed to read upstream response"
                    )
                }
            }
        }
        Err(e) => {
            error!("Failed to forward to upstream: {}", e);
            state.metrics.upstream_errors.inc();
            error_response(
                StatusCode::BAD_GATEWAY,
                &format!("Upstream unavailable: {}", e)
            )
        }
    }
}

/// Check if a header is hop-by-hop (should not be forwarded).
fn is_hop_by_hop_header(name: &str) -> bool {
    const HOP_BY_HOP: &[&str] = &[
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailers",
        "transfer-encoding",
        "upgrade",
    ];
    
    HOP_BY_HOP.contains(&name.to_lowercase().as_str())
}

/// Generate a block response for denied requests.
fn block_response(
    ctx: &RequestContext,
    result: &EvaluationResult,
) -> Response {
    let block_response = BlockResponse {
        request_id: ctx.request_id,
        blocked: true,
        reason: result.explanation.clone()
            .unwrap_or_else(|| "Request blocked by policy".to_string()),
        triggered_constraint: result.triggered_constraints.first().copied(),
    };
    
    let body = match serde_json::to_string(&block_response) {
        Ok(json) => Body::from(json),
        Err(_) => Body::from(r#"{"blocked": true}"#),
    };
    
    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::FORBIDDEN;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json")
    );
    
    response
}

/// Generate a generic error response.
fn error_response(status: StatusCode, message: &str) -> Response {
    let body = Body::from(format!(
        r#"{{"error": "{}"}}"#,
        message.replace('"', "\\\"")
    ));
    
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json")
    );
    
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{HeaderMap, HeaderValue};
    use bytes::Bytes;

    #[test]
    fn test_agnostic_detects_native_shape() {
        let v = serde_json::json!({ "prompt": "hello", "target_model": "gpt-4o" });
        let view = AgnosticView::detect(&v);
        assert_eq!(view.shape(), PayloadShape::Native);
        assert_eq!(view.extract_prompt(), "hello");
        assert_eq!(view.extract_model(), "gpt-4o");
    }

    #[test]
    fn test_agnostic_detects_openai_chat() {
        let v = serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                { "role": "system", "content": "be concise" },
                { "role": "user", "content": "first" },
                { "role": "user", "content": "second" }
            ]
        });
        let view = AgnosticView::detect(&v);
        assert_eq!(view.shape(), PayloadShape::OpenAiChat);
        // user messages are stitched
        assert_eq!(view.extract_prompt(), "first\nsecond");
        // system role message is surfaced
        assert_eq!(view.extract_system().as_deref(), Some("be concise"));
        assert_eq!(view.extract_model(), "gpt-4o");
    }

    #[test]
    fn test_agnostic_detects_anthropic_messages() {
        let v = serde_json::json!({
            "model": "claude-3-opus",
            "system": "you are a financial advisor",
            "messages": [ { "role": "user", "content": "what is an ETF?" } ]
        });
        let view = AgnosticView::detect(&v);
        assert_eq!(view.shape(), PayloadShape::AnthropicMessages);
        assert_eq!(view.extract_prompt(), "what is an ETF?");
        assert_eq!(view.extract_system().as_deref(), Some("you are a financial advisor"));
        assert_eq!(view.extract_model(), "claude-3-opus");
    }

    #[test]
    fn test_native_payload_borrows_without_alloc() {
        use std::borrow::Cow;
        let payload = IncomingPayload {
            prompt: Cow::Borrowed("borrowed prompt"),
            system: Some(Cow::Borrowed("sys")),
            context: std::collections::HashMap::new(),
            target_model: Cow::Borrowed("m"),
            parameters: None,
        };
        // Native extraction must stay borrowed (zero-allocation contract).
        assert!(matches!(payload.extract_prompt(), Cow::Borrowed(_)));
        assert!(matches!(payload.extract_model(), Cow::Borrowed(_)));
    }

    #[test]
    fn test_extract_customer_id_valid() {
        let mut headers = HeaderMap::new();
        let valid_uuid = uuid::Uuid::new_v4();
        headers.insert(
            HEADER_CUSTOMER_ID,
            HeaderValue::from_str(&valid_uuid.to_string()).unwrap()
        );

        let result = extract_customer_id(&headers);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, valid_uuid);
    }

    #[test]
    fn test_extract_customer_id_missing() {
        let headers = HeaderMap::new();
        let result = extract_customer_id(&headers);
        
        assert!(result.is_err());
        match result.unwrap_err() {
            TraceError::InvalidRequest(msg) => {
                assert!(msg.contains("Missing required header"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[test]
    fn test_extract_customer_id_invalid_uuid() {
        let mut headers = HeaderMap::new();
        headers.insert(HEADER_CUSTOMER_ID, HeaderValue::from_static("not-a-uuid"));
        
        let result = extract_customer_id(&headers);
        assert!(result.is_err());
        match result.unwrap_err() {
            TraceError::InvalidRequest(msg) => {
                assert!(msg.contains("Invalid UUID"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[test]
    fn test_extract_customer_id_invalid_format() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HEADER_CUSTOMER_ID,
            HeaderValue::from_bytes(&[0x80, 0x81, 0x82]).unwrap()
        );
        
        let result = extract_customer_id(&headers);
        assert!(result.is_err());
        match result.unwrap_err() {
            TraceError::InvalidRequest(msg) => {
                assert!(msg.contains("Invalid customer ID format"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[tokio::test]
    async fn test_read_body_success() {
        let body = Body::from("test body content");
        let result = read_body(body, 1024).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Bytes::from("test body content"));
    }

    #[tokio::test]
    async fn test_read_body_empty() {
        let body = Body::empty();
        let result = read_body(body, 1024).await;
        
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_read_body_exceeds_max_size() {
        let large_content = "x".repeat(200);
        let body = Body::from(large_content);
        let result = read_body(body, 100).await;
        
        assert!(result.is_err());
    }

    #[test]
    fn test_is_hop_by_hop_header() {
        assert!(is_hop_by_hop_header("connection"));
        assert!(is_hop_by_hop_header("keep-alive"));
        assert!(is_hop_by_hop_header("proxy-authenticate"));
        assert!(is_hop_by_hop_header("proxy-authorization"));
        assert!(is_hop_by_hop_header("te"));
        assert!(is_hop_by_hop_header("trailers"));
        assert!(is_hop_by_hop_header("transfer-encoding"));
        assert!(is_hop_by_hop_header("upgrade"));
        
        assert!(is_hop_by_hop_header("Connection"));
        assert!(is_hop_by_hop_header("KEEP-ALIVE"));
    }

    #[test]
    fn test_is_not_hop_by_hop_header() {
        assert!(!is_hop_by_hop_header("content-type"));
        assert!(!is_hop_by_hop_header("authorization"));
        assert!(!is_hop_by_hop_header("x-custom-header"));
    }

    #[test]
    fn test_block_response_generation() {
        let customer_id = CustomerId::new();
        let ctx = RequestContext::new(customer_id);
        let result = EvaluationResult {
            verdict: TraceVerdict::Block,
            triggered_constraints: vec![uuid::Uuid::new_v4()],
            explanation: Some("Test block reason".to_string()),
            modified_payload: None,
            latency_us: 100,
        };

        let response = block_response(&ctx, &result);
        
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }

    #[test]
    fn test_block_response_no_explanation() {
        let customer_id = CustomerId::new();
        let ctx = RequestContext::new(customer_id);
        let result = EvaluationResult {
            verdict: TraceVerdict::Block,
            triggered_constraints: vec![],
            explanation: None,
            modified_payload: None,
            latency_us: 100,
        };

        let response = block_response(&ctx, &result);
        
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_error_response_generation() {
        let response = error_response(StatusCode::BAD_REQUEST, "Invalid input");
        
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }

    #[test]
    fn test_error_response_escapes_quotes() {
        let response = error_response(StatusCode::BAD_REQUEST, r#"Error: "bad" input"#);
        
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

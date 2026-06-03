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
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use std::time::Instant;
use tracing::{error, info, instrument, warn};

use crate::engine::TrajectoryEngine;
use crate::main::AppState;
use crate::types::{
    BlockResponse, CustomerId, EvaluationResult, IncomingPayload, RequestContext, 
    RequestId, TraceError, TraceVerdict
};

/// Header name for customer identification.
const HEADER_CUSTOMER_ID: &str = "x-customer-id";
/// Header name for request tracing.
const HEADER_REQUEST_ID: &str = "x-request-id";

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
            forward_to_upstream(&state, &headers, body_bytes).await
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
                    forward_to_upstream(&state, &headers, body_bytes).await
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
    use axum::body::HttpBody;
    
    let mut body = body;
    let mut buffer = bytes::BytesMut::new();
    
    while let Some(chunk) = body.data().await {
        let chunk = chunk?;
        if buffer.len() + chunk.len() > max_size {
            return Err("Body exceeds maximum size".into());
        }
        buffer.extend_from_slice(&chunk);
    }
    
    Ok(buffer.freeze())
}

/// Evaluate the payload against customer policies.
async fn evaluate_payload(
    state: &AppState,
    customer_id: CustomerId,
    payload: &IncomingPayload<'_>,
) -> Result<EvaluationResult, TraceError> {
    // Get customer policy
    let policy = state.policy_store.get_policy(customer_id).await
        .ok_or_else(|| TraceError::PolicyNotFound(customer_id))?;
    
    // Initialize the trajectory engine
    let engine = TrajectoryEngine::new(policy);
    
    // Perform evaluation with timeout
    let eval_start = Instant::now();
    
    let result = engine.evaluate(payload).await;
    
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
        let name = name.as_str();
        // Skip hop-by-hop headers
        if !is_hop_by_hop_header(name) {
            upstream_request = upstream_request.header(name, value);
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
                    *response.status_mut() = status;
                    *response.headers_mut() = headers;
                    response
                }
                Err(e) => {
                    error!("Failed to read upstream response body: {}", e);
                    error_response(
                        StatusCode::BAD_GATEWAY,
                        "Failed to read upstream response"
                    )
                }
            }
        }
        Err(e) => {
            error!("Failed to forward to upstream: {}", e);
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

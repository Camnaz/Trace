//! Trace - Ultra-low-latency HTTP proxy for LLM request evaluation
//!
//! Entry point for the Stria Systems Trace proxy server.
//! Initializes the Tokio runtime, loads configuration, and starts the Axum server.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{routing::post, Router};
use tokio::signal;
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod types;
mod proxy;
mod engine;
mod policy;

use types::ProxyConfig;
use policy::PolicyStore;

/// Application state shared across all request handlers.
#[derive(Clone)]
pub struct AppState {
    /// Configuration for the proxy.
    config: Arc<ProxyConfig>,
    /// The policy store for customer constraints.
    policy_store: PolicyStore,
    /// HTTP client for forwarding to upstream LLM.
    http_client: reqwest::Client,
}

#[tokio::main]
async fn main() {
    // Initialize tracing/logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .json()
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");

    info!("Starting Trace proxy server...");

    // Load configuration
    let config = load_config();
    info!("Configuration loaded: bind={}:{} upstream={}", 
          config.bind_address, config.port, config.upstream_url);

    // Initialize the policy store
    let policy_store = PolicyStore::new();
    info!("Policy store initialized");

    // Build the HTTP client for upstream forwarding
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(config.timeout_ms))
        .http2_prior_knowledge()
        .build()
        .expect("Failed to build HTTP client");

    // Create shared application state
    let state = AppState {
        config: Arc::new(config),
        policy_store,
        http_client,
    };

    // Build the Axum router
    let app = build_router(state);

    // Parse bind address
    let addr: SocketAddr = format!("{}:{}", 
        state.config.bind_address, 
        state.config.port
    ).parse()
        .expect("Invalid bind address");

    info!("Server listening on http://{}", addr);

    // Start the server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(addr).await
        .expect("Failed to bind to address");
    
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server error");

    info!("Server shutdown complete");
}

/// Build the Axum router with all routes and middleware.
fn build_router(state: AppState) -> Router {
    let middleware_stack = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(std::time::Duration::from_millis(
            state.config.timeout_ms
        )))
        .into_inner();

    Router::new()
        // Main proxy endpoint
        .route("/v1/proxy", post(proxy::handle_proxy_request))
        // Health check
        .route("/health", axum::routing::get(health_check))
        // Admin endpoints for policy management
        .route("/admin/v1/policies/:customer_id", 
            post(admin::update_policy)
            .get(admin::get_policy)
            .delete(admin::delete_policy))
        .layer(middleware_stack)
        .with_state(state)
}

/// Load configuration from environment and config files.
fn load_config() -> ProxyConfig {
    // In production, this would load from environment variables,
    // config files, or a secrets manager.
    // For now, use sensible defaults or env vars.
    
    ProxyConfig {
        bind_address: std::env::var("TRACE_BIND_ADDRESS")
            .unwrap_or_else(|_| "0.0.0.0".to_string()),
        port: std::env::var("TRACE_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080),
        upstream_url: std::env::var("TRACE_UPSTREAM_URL")
            .unwrap_or_else(|_| "http://localhost:11434".to_string()),
        max_body_size: std::env::var("TRACE_MAX_BODY_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1024 * 1024),
        timeout_ms: std::env::var("TRACE_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30000),
        max_evaluation_micros: std::env::var("TRACE_MAX_EVAL_US")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(15000),
    }
}

/// Simple health check endpoint.
async fn health_check() -> &'static str {
    "OK"
}

/// Graceful shutdown signal handler.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(unix)]
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    
    #[cfg(not(unix))]
    tokio::select! {
        _ = ctrl_c => {},
    }

    tracing::info!("Shutdown signal received, starting graceful shutdown...");
}

/// Admin API handlers for policy management.
mod admin {
    use super::*;
    use axum::{extract::{Path, State}, Json};
    use types::{CustomerId, CustomerPolicy};
    use uuid::Uuid;

    /// Update or create a policy for a customer.
    pub async fn update_policy(
        State(state): State<AppState>,
        Path(customer_id): Path<Uuid>,
        Json(policy): Json<CustomerPolicy>,
    ) -> impl axum::response::IntoResponse {
        let customer_id = CustomerId(customer_id);
        
        info!("Updating policy for customer: {}", customer_id.0);
        
        state.policy_store.set_policy(customer_id, policy);
        
        (axum::http::StatusCode::OK, "Policy updated")
    }

    /// Get the current policy for a customer.
    pub async fn get_policy(
        State(state): State<AppState>,
        Path(customer_id): Path<Uuid>,
    ) -> impl axum::response::IntoResponse {
        let customer_id = CustomerId(customer_id);
        
        match state.policy_store.get_policy(customer_id).await {
            Some(policy) => {
                let json = serde_json::to_string(&policy)
                    .unwrap_or_else(|_| "{}".to_string());
                (
                    axum::http::StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    json,
                )
            }
            None => (
                axum::http::StatusCode::NOT_FOUND,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"error": "Policy not found"}"#.to_string(),
            ),
        }
    }

    /// Delete a customer's policy.
    pub async fn delete_policy(
        State(state): State<AppState>,
        Path(customer_id): Path<Uuid>,
    ) -> impl axum::response::IntoResponse {
        let customer_id = CustomerId(customer_id);
        
        info!("Deleting policy for customer: {}", customer_id.0);
        
        state.policy_store.remove_policy(customer_id);
        
        (axum::http::StatusCode::NO_CONTENT, "")
    }
}

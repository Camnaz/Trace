//! Trace - Ultra-low-latency HTTP proxy for LLM request evaluation
//!
//! Library crate for the Stria Systems Trace proxy.

pub mod types;
pub mod proxy;
pub mod engine;
pub mod policy;
pub mod config;
pub mod metrics;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{routing::post, Router};
use tokio::signal;
use tokio::sync::broadcast;
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use types::{ProxyConfig, TelemetryEvent};
use policy::PolicyStore;

/// Capacity of the telemetry broadcast channel.
/// Lagging subscribers simply lose old events — the proxy never blocks.
const TELEMETRY_CHANNEL_CAPACITY: usize = 512;

/// Application state shared across all request handlers.
#[derive(Clone)]
pub struct AppState {
    /// Configuration for the proxy.
    pub config: Arc<ProxyConfig>,
    /// The policy store for customer constraints.
    pub policy_store: PolicyStore,
    /// HTTP client for forwarding to upstream LLM.
    pub http_client: reqwest::Client,
    /// Broadcast sender for real-time telemetry events (SSE, dashboards, CI).
    pub telemetry_tx: broadcast::Sender<TelemetryEvent>,
    /// Handle to the background training shell (boundary compilation + corpus).
    pub shell: engine::ShellHandle,
    /// Per-org real-time synthetic traffic factory.
    pub factory: engine::FactoryControl,
    /// Agentic onboarding agent (Onyx) for natural-language first setup.
    pub onboard: engine::OnboardAgent,
    /// Metrics registry for Prometheus-compatible observability.
    pub metrics: Arc<metrics::Metrics>,
}

/// Initialize and run the Trace proxy server.
pub async fn run() {
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
    let config = config::load_config();
    info!("Configuration loaded: bind={}:{} upstream={}", 
          config.bind_address, config.port, config.upstream_url);

    // Initialize the policy store
    let policy_store = PolicyStore::new();
    info!("Policy store initialized");

    // Build the HTTP client for upstream forwarding
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(config.timeout_ms))
        .build()
        .expect("Failed to build HTTP client");

    // Create the telemetry broadcast channel
    let (telemetry_tx, _) = broadcast::channel(TELEMETRY_CHANNEL_CAPACITY);

    // Spawn the background training shell loop (boundary compilation + corpus)
    let shell = engine::shell::spawn(policy_store.clone());
    info!("Training shell loop spawned");

    // Initialize the real-time data factory
    let factory = engine::FactoryControl::new(
        policy_store.clone(),
        telemetry_tx.clone(),
        shell.corpus().clone(),
    );
    info!("Data factory initialized");

    // Initialize the agentic onboarding agent (Onyx)
    let onboard = engine::OnboardAgent::new();
    info!("Onboarding agent initialized");

    // Initialize metrics registry
    let metrics = metrics::Metrics::new();
    info!("Metrics registry initialized");

    // Create shared application state
    let state = AppState {
        config: Arc::new(config),
        policy_store,
        http_client,
        telemetry_tx,
        shell,
        factory,
        onboard,
        metrics,
    };

    // Build the Axum router
    let app = build_router(state);

    // Parse bind address
    let addr: SocketAddr = format!("{}:{}", 
        "0.0.0.0", 
        8080
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
        // Real-time telemetry stream (Server-Sent Events)
        .route("/v1/events", axum::routing::get(telemetry::sse_handler))
        // Health check
        .route("/health", axum::routing::get(health_check))
        // Prometheus-compatible metrics
        .route("/metrics", axum::routing::get(metrics::serve_metrics))
        // Admin endpoints for policy management
        .route("/admin/v1/policies",
            axum::routing::get(admin::list_policies))
        .route("/admin/v1/policies/:customer_id", 
            axum::routing::post(admin::update_policy)
            .get(admin::get_policy)
            .delete(admin::delete_policy))
        // Automated synthetic stress test → Policy Leakage Risk %
        .route("/admin/v1/stress-test/:customer_id",
            axum::routing::post(stress::run_stress_test))
        // Training shell: submit a natural-language boundary directive
        .route("/admin/v1/boundaries/:org_id",
            axum::routing::post(shell_api::submit_boundary))
        // Training shell: read per-org training progress
        .route("/admin/v1/training/:org_id",
            axum::routing::get(shell_api::training_state))
        // Batch verification + gated Git-sync
        .route("/admin/v1/verify/:org_id",
            axum::routing::post(verify_api::run_verify))
        // Provision: synthesize a policy from NL description + explicit terms
        .route("/admin/v1/provision/:org_id",
            axum::routing::post(provision_api::provision_policy))
        // Factory: start / stop / status
        .route("/admin/v1/factory/:org_id/start",
            axum::routing::post(factory_api::start_factory))
        .route("/admin/v1/factory/:org_id/stop",
            axum::routing::post(factory_api::stop_factory))
        .route("/admin/v1/factory/:org_id/status",
            axum::routing::get(factory_api::factory_status))
        // Sectors & scenarios
        .route("/admin/v1/sectors",
            axum::routing::get(sectors_api::list_sectors))
        .route("/admin/v1/sectors/:sector_id/scenarios",
            axum::routing::get(sectors_api::list_scenarios))
        // Suggest guardrails from natural-language description
        .route("/admin/v1/suggest-guardrails",
            axum::routing::post(sectors_api::suggest_guardrails))
        // Agentic Onboarding API (Onyx)
        .route("/admin/v1/onboard/start/:org_id",
            axum::routing::post(onboard_api::start_onboarding))
        .route("/admin/v1/onboard/chat/:org_id",
            axum::routing::post(onboard_api::chat))
        .route("/admin/v1/onboard/status/:org_id",
            axum::routing::get(onboard_api::status))
        .route("/admin/v1/onboard/finalize/:org_id",
            axum::routing::post(onboard_api::finalize))
        // Policy Studio web UI
        .route("/", axum::routing::get(ui::serve_ui))
        .route("/ui", axum::routing::get(ui::serve_ui))
        .layer(middleware_stack)
        .with_state(state)
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

    #[cfg(unix)]
    {
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install signal handler")
                .recv()
                .await;
        };

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    }
    
    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = ctrl_c => {},
        }
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
                let json = serde_json::to_string(policy.as_ref())
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

    /// List all customer policies currently loaded in the store.
    pub async fn list_policies(
        State(state): State<AppState>,
    ) -> impl axum::response::IntoResponse {
        let ids = state.policy_store.customer_ids();
        let mut policies = Vec::with_capacity(ids.len());

        for cid in ids {
            if let Some(p) = state.policy_store.get_policy(cid).await {
                policies.push(p.as_ref().clone());
            }
        }

        match serde_json::to_string(&policies) {
            Ok(json) => (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                json,
            ),
            Err(_) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"error":"serialization failed"}"#.to_string(),
            ),
        }
    }
}

/// Automated synthetic stress-testing.
///
/// Generates adversarial probes directly from a customer's active policy and
/// runs them through the live engine, returning a single high-order metric:
/// **Policy Leakage Risk %**.
mod stress {
    use super::*;
    use axum::extract::{Path, State};
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::time::Instant;
    use uuid::Uuid;
    use engine::{generate_probes, ProbeTechnique, TrajectoryEngine};
    use types::{CustomerId, IncomingPayload, TraceVerdict};

    #[derive(serde::Serialize)]
    struct ProbeOutcome {
        prompt_preview: String,
        targeted_constraint: String,
        constraint_id: Uuid,
        technique: ProbeTechnique,
        expected: TraceVerdict,
        actual: TraceVerdict,
        breached: bool,
        latency_us: u64,
    }

    #[derive(serde::Serialize)]
    struct StressReport {
        customer_id: Uuid,
        total_probes: usize,
        breached: usize,
        /// Fraction of adversarial probes that leaked, as a percentage.
        leakage_risk_pct: f64,
        avg_latency_us: u64,
        probes: Vec<ProbeOutcome>,
    }

    /// Run the synthetic stress test for a customer and report leakage risk.
    pub async fn run_stress_test(
        State(state): State<AppState>,
        Path(customer_id): Path<Uuid>,
    ) -> impl axum::response::IntoResponse {
        let cid = CustomerId(customer_id);

        let policy = match state.policy_store.get_policy(cid).await {
            Some(p) => p,
            None => {
                return (
                    axum::http::StatusCode::NOT_FOUND,
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    r#"{"error":"Policy not found"}"#.to_string(),
                );
            }
        };

        let probes = generate_probes(&policy);
        let engine = TrajectoryEngine::new(policy.clone());

        let mut outcomes = Vec::with_capacity(probes.len());
        let mut breached = 0usize;
        let mut latency_sum = 0u64;

        for probe in &probes {
            let payload = IncomingPayload {
                prompt: Cow::Owned(probe.prompt.clone()),
                system: None,
                context: HashMap::new(),
                target_model: Cow::Borrowed("stress-test"),
                parameters: None,
            };

            let start = Instant::now();
            let result = engine.evaluate(&payload);
            let latency_us = start.elapsed().as_micros() as u64;
            latency_sum += latency_us;

            // A breach: the probe should have been caught but slipped through.
            let is_breach = match probe.expected {
                TraceVerdict::Block => result.verdict != TraceVerdict::Block,
                TraceVerdict::Modify => result.verdict == TraceVerdict::Pass,
                TraceVerdict::Pass => false,
            };
            if is_breach {
                breached += 1;
            }

            outcomes.push(ProbeOutcome {
                prompt_preview: preview(&probe.prompt),
                targeted_constraint: probe.targeted_constraint.clone(),
                constraint_id: probe.constraint_id,
                technique: probe.technique,
                expected: probe.expected,
                actual: result.verdict,
                breached: is_breach,
                latency_us,
            });
        }

        let total = outcomes.len();
        let leakage = if total == 0 { 0.0 } else { breached as f64 / total as f64 * 100.0 };
        let avg_latency = if total == 0 { 0 } else { latency_sum / total as u64 };

        info!(
            customer_id = %customer_id,
            total_probes = total,
            breached = breached,
            leakage_risk_pct = leakage,
            "Stress test complete"
        );

        let report = StressReport {
            customer_id,
            total_probes: total,
            breached,
            leakage_risk_pct: (leakage * 10.0).round() / 10.0,
            avg_latency_us: avg_latency,
            probes: outcomes,
        };

        match serde_json::to_string(&report) {
            Ok(json) => (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                json,
            ),
            Err(_) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"error":"serialization failed"}"#.to_string(),
            ),
        }
    }

    fn preview(s: &str) -> String {
        const MAX: usize = 120;
        if s.len() <= MAX {
            s.to_string()
        } else {
            format!("{}…", &s[..MAX])
        }
    }
}

/// Training shell HTTP API — submit boundaries, read training progress.
mod shell_api {
    use super::*;
    use axum::extract::{Path, State};
    use axum::Json;
    use types::{BoundaryDirective, ConstraintAction, CustomerId};
    use uuid::Uuid;

    #[derive(serde::Deserialize)]
    pub struct BoundaryRequest {
        /// Natural-language boundary text.
        pub text: String,
        /// Optional action (defaults to block).
        #[serde(default)]
        pub action: ConstraintAction,
    }

    /// Submit a natural-language boundary directive for compilation.
    pub async fn submit_boundary(
        State(state): State<AppState>,
        Path(org_id): Path<Uuid>,
        Json(req): Json<BoundaryRequest>,
    ) -> impl axum::response::IntoResponse {
        let directive = BoundaryDirective {
            org_id: CustomerId(org_id),
            text: req.text,
            action: req.action,
        };
        match state.shell.submit(directive).await {
            Ok(()) => (
                axum::http::StatusCode::ACCEPTED,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"status":"accepted","detail":"directive queued for compilation"}"#.to_string(),
            ),
            Err(e) => (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                format!(r#"{{"error":{:?}}}"#, e),
            ),
        }
    }

    /// Read the current training state for an organization.
    pub async fn training_state(
        State(state): State<AppState>,
        Path(org_id): Path<Uuid>,
    ) -> impl axum::response::IntoResponse {
        let org = CustomerId(org_id);
        let ts = state.shell.training_state(org).unwrap_or_default();
        let pending = state.shell.corpus().pending(org);
        let body = serde_json::json!({
            "org_id": org_id,
            "directives_compiled": ts.directives_compiled,
            "last_accuracy_pct": ts.last_accuracy_pct,
            "iterations": ts.iterations,
            "active_rules": ts.active_rules,
            "pending_corpus_samples": pending,
        });
        (
            axum::http::StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            body.to_string(),
        )
    }
}

/// Verification + Git-sync HTTP API.
mod verify_api {
    use super::*;
    use axum::extract::{Path, State};
    use types::CustomerId;
    use uuid::Uuid;

    /// Run the adversarial batch verification and (if stable) the Git-sync gate.
    pub async fn run_verify(
        State(state): State<AppState>,
        Path(org_id): Path<Uuid>,
    ) -> impl axum::response::IntoResponse {
        let org = CustomerId(org_id);

        match engine::run_verification(&state.policy_store, org).await {
            Some(report) => {
                // On a verified-stable sync, drain the capture corpus into the
                // next engine build window.
                if report.synced {
                    let drained = state.shell.corpus().drain(org).len();
                    info!(org = %org, drained_samples = drained, "Corpus drained at sync");
                }
                let json = serde_json::to_string(&report)
                    .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string());
                (
                    axum::http::StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    json,
                )
            }
            None => (
                axum::http::StatusCode::NOT_FOUND,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"error":"Policy not found for organization"}"#.to_string(),
            ),
        }
    }
}

/// Server-Sent Events handler for real-time telemetry.
mod telemetry {
    use super::*;
    use axum::extract::State;
    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures_util::stream::Stream;
    use std::convert::Infallible;
    use tokio_stream::wrappers::BroadcastStream;
    use tokio_stream::StreamExt as _;

    /// Subscribe to the telemetry broadcast and stream events as SSE.
    pub async fn sse_handler(
        State(state): State<AppState>,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
        let rx = state.telemetry_tx.subscribe();
        let stream = BroadcastStream::new(rx)
            .filter_map(|result| {
                match result {
                    Ok(event) => {
                        let data = serde_json::to_string(&event)
                            .unwrap_or_else(|_| "{}".to_string());
                        Some(Ok(Event::default().event("trace").data(data)))
                    }
                    // Lagged — subscriber fell behind; skip and continue
                    Err(_) => None,
                }
            });

        Sse::new(stream).keep_alive(KeepAlive::default())
    }
}

/// Provision API — synthesize a policy from natural-language + explicit terms.
mod provision_api {
    use super::*;
    use axum::extract::{Path, State};
    use axum::Json;
    use types::CustomerId;
    use uuid::Uuid;

    #[derive(serde::Deserialize)]
    pub struct ProvisionRequest {
        /// Natural-language company description.
        pub description: String,
        /// Explicit terms to block.
        #[serde(default)]
        pub explicit_terms: Vec<String>,
    }

    #[derive(serde::Serialize)]
    pub struct ProvisionResponse {
        pub org_id: Uuid,
        pub policy: crate::types::CustomerPolicy,
    }

    pub async fn provision_policy(
        State(state): State<AppState>,
        Path(org_id): Path<Uuid>,
        Json(req): Json<ProvisionRequest>,
    ) -> impl axum::response::IntoResponse {
        let org = CustomerId(org_id);
        let policy = engine::synthesize_policy(org, &req.description, &req.explicit_terms);
        state.policy_store.set_policy(org, policy.clone());

        let resp = ProvisionResponse { org_id, policy };
        match serde_json::to_string(&resp) {
            Ok(json) => (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                json,
            ),
            Err(_) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"error":"serialization failed"}"#.to_string(),
            ),
        }
    }
}

/// Factory API — start, stop, and query the real-time synthetic traffic generator.
mod factory_api {
    use super::*;
    use axum::extract::{Path, State};
    use axum::Json;
    use types::CustomerId;
    use uuid::Uuid;

    #[derive(serde::Deserialize)]
    pub struct StartRequest {
        /// Requests per second to generate (1–100).
        #[serde(default = "default_rate")]
        pub rate: u64,
        /// Sector to run factory scenarios from.
        #[serde(default)]
        pub sector: String,
    }

    fn default_rate() -> u64 {
        10
    }

    fn parse_sector(s: &str) -> engine::Sector {
        match s.to_lowercase().as_str() {
            "financial_services" | "financial" | "finance" => engine::Sector::FinancialServices,
            "healthcare" | "health" | "medical" => engine::Sector::Healthcare,
            "legal" | "law" => engine::Sector::Legal,
            _ => engine::Sector::Generic,
        }
    }

    pub async fn start_factory(
        State(state): State<AppState>,
        Path(org_id): Path<Uuid>,
        Json(req): Json<StartRequest>,
    ) -> impl axum::response::IntoResponse {
        let org = CustomerId(org_id);
        let sector = if req.sector.is_empty() {
            None
        } else {
            Some(parse_sector(&req.sector))
        };
        state.factory.start(org, req.rate.clamp(1, 100), sector);
        (
            axum::http::StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            format!(
                r#"{{"status":"started","org_id":"{}","rate_per_sec":{}}}"#,
                org_id, req.rate
            ),
        )
    }

    pub async fn stop_factory(
        State(state): State<AppState>,
        Path(org_id): Path<Uuid>,
    ) -> impl axum::response::IntoResponse {
        let org = CustomerId(org_id);
        state.factory.stop(org);
        (
            axum::http::StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            format!(r#"{{"status":"stopped","org_id":"{}"}}"#, org_id),
        )
    }

    pub async fn factory_status(
        State(state): State<AppState>,
        Path(org_id): Path<Uuid>,
    ) -> impl axum::response::IntoResponse {
        let org = CustomerId(org_id);
        let status = state.factory.status(org);
        match serde_json::to_string(&status) {
            Ok(json) => (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                json,
            ),
            Err(_) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"error":"serialization failed"}"#.to_string(),
            ),
        }
    }
}

/// Sectors & Scenarios API — list sectors, list scenarios, suggest guardrails.
mod sectors_api {
    use super::*;
    use axum::extract::Path;
    use axum::Json;
    use engine::sectors::{Sector, Scenario};

    #[derive(serde::Serialize)]
    struct SectorListItem {
        id: String,
        name: String,
    }

    pub async fn list_sectors() -> impl axum::response::IntoResponse {
        let sectors: Vec<SectorListItem> = Sector::all().iter().map(|s| SectorListItem {
            id: format!("{:?}", s).to_lowercase(),
            name: s.name().to_string(),
        }).collect();
        match serde_json::to_string(&sectors) {
            Ok(json) => (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                json,
            ),
            Err(_) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"error":"serialization failed"}"#.to_string(),
            ),
        }
    }

    pub async fn list_scenarios(
        Path(sector_id): Path<String>,
    ) -> impl axum::response::IntoResponse {
        let sector = match sector_id.as_str() {
            "financial_services" => Sector::FinancialServices,
            "healthcare" => Sector::Healthcare,
            "legal" => Sector::Legal,
            _ => Sector::Generic,
        };
        let scenarios: Vec<Scenario> = sector.scenarios().to_vec();
        match serde_json::to_string(&scenarios) {
            Ok(json) => (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                json,
            ),
            Err(_) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"error":"serialization failed"}"#.to_string(),
            ),
        }
    }

    #[derive(serde::Deserialize)]
    pub struct SuggestRequest {
        pub sector: String,
        pub description: String,
    }

    #[derive(serde::Serialize)]
    pub struct SuggestedGuardrail {
        pub name: String,
        pub patterns: Vec<String>,
        pub action: String,
        pub reason: String,
    }

    /// Suggest guardrails based on sector + natural-language description.
    pub async fn suggest_guardrails(
        Json(req): Json<SuggestRequest>,
    ) -> impl axum::response::IntoResponse {
        let sector = match req.sector.to_lowercase().as_str() {
            "financial_services" => Sector::FinancialServices,
            "healthcare" => Sector::Healthcare,
            "legal" => Sector::Legal,
            _ => Sector::Generic,
        };

        // Start with sector templates
        let mut suggestions: Vec<SuggestedGuardrail> = sector.suggested_guardrails().into_iter().map(|(name, patterns)| SuggestedGuardrail {
            name: name.to_string(),
            patterns: patterns.iter().map(|p| p.to_string()).collect(),
            action: "block".to_string(),
            reason: format!("Common {} risk pattern", sector.name()),
        }).collect();

        // Extract salient terms from the user's description using the shell's directive compiler
        let salient = engine::shell::compile_directive(&req.description);
        if !salient.is_empty() {
            suggestions.push(SuggestedGuardrail {
                name: "Custom Boundary".to_string(),
                patterns: salient,
                action: "block".to_string(),
                reason: "Extracted from your description".to_string(),
            });
        }

        // If the description mentions specific sensitive concepts, add targeted suggestions
        let desc_lower = req.description.to_lowercase();
        if desc_lower.contains("competitor") || desc_lower.contains("competition") {
            suggestions.push(SuggestedGuardrail {
                name: "Competitor Mention Shield".to_string(),
                patterns: vec!["competitor".to_string(), "rival".to_string()],
                action: "block".to_string(),
                reason: "Prevents disclosure of competitive positioning".to_string(),
            });
        }
        if desc_lower.contains("secret") || desc_lower.contains("confidential") || desc_lower.contains("internal") {
            suggestions.push(SuggestedGuardrail {
                name: "Confidential Information Shield".to_string(),
                patterns: vec!["confidential".to_string(), "secret".to_string(), "internal only".to_string(), "proprietary".to_string()],
                action: "block".to_string(),
                reason: "Protects internal and proprietary information".to_string(),
            });
        }
        if desc_lower.contains("patient") || desc_lower.contains("medical") || desc_lower.contains("health") {
            suggestions.push(SuggestedGuardrail {
                name: "PHI / Medical Data Shield".to_string(),
                patterns: vec!["patient id".to_string(), "diagnosis".to_string(), "medical record".to_string(), "prescription".to_string()],
                action: "block".to_string(),
                reason: "Prevents exposure of protected health information".to_string(),
            });
        }

        match serde_json::to_string(&suggestions) {
            Ok(json) => (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                json,
            ),
            Err(_) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"error":"serialization failed"}"#.to_string(),
            ),
        }
    }
}

/// Onboarding API — agentic conversational setup with Onyx.
mod onboard_api {
    use super::*;
    use axum::extract::{Path, State};
    use axum::Json;
    use types::CustomerId;
    use uuid::Uuid;

    #[derive(serde::Deserialize)]
    pub struct ChatRequest {
        pub message: String,
    }

    #[derive(serde::Serialize)]
    pub struct ChatResponse {
        pub reply: String,
        pub stage: String,
        pub detected_sector: Option<String>,
        pub detected_frameworks: Vec<String>,
        pub draft_policy_constraints: usize,
        pub can_finalize: bool,
    }

    #[derive(serde::Serialize)]
    pub struct StatusResponse {
        pub stage: String,
        pub is_complete: bool,
        pub detected_sector: Option<String>,
        pub detected_frameworks: Vec<String>,
        pub draft_policy_constraints: usize,
    }

    #[derive(serde::Serialize)]
    pub struct FinalizeResponse {
        pub activated: bool,
        pub constraints_count: usize,
        pub sector: Option<String>,
        pub frameworks: Vec<String>,
    }

    pub async fn start_onboarding(
        State(state): State<AppState>,
        Path(org_id): Path<Uuid>,
    ) -> impl axum::response::IntoResponse {
        let org = CustomerId(org_id);
        let reply = state.onboard.start(org);
        json_response(&ChatResponse {
            reply: reply.text,
            stage: reply.stage,
            detected_sector: reply.detected_sector,
            detected_frameworks: reply.detected_frameworks,
            draft_policy_constraints: reply.draft_policy_constraints,
            can_finalize: reply.can_finalize,
        })
    }

    pub async fn chat(
        State(state): State<AppState>,
        Path(org_id): Path<Uuid>,
        Json(req): Json<ChatRequest>,
    ) -> impl axum::response::IntoResponse {
        let org = CustomerId(org_id);
        let reply = state.onboard.chat(org, req.message);
        json_response(&ChatResponse {
            reply: reply.text,
            stage: reply.stage,
            detected_sector: reply.detected_sector,
            detected_frameworks: reply.detected_frameworks,
            draft_policy_constraints: reply.draft_policy_constraints,
            can_finalize: reply.can_finalize,
        })
    }

    pub async fn status(
        State(state): State<AppState>,
        Path(org_id): Path<Uuid>,
    ) -> impl axum::response::IntoResponse {
        let org = CustomerId(org_id);
        let policy = state.onboard.take_policy(org);
        let is_complete = state.onboard.is_complete(org);
        json_response(&StatusResponse {
            stage: if is_complete { "complete".into() } else { "in_progress".into() },
            is_complete,
            detected_sector: None,
            detected_frameworks: Vec::new(),
            draft_policy_constraints: policy.as_ref().map(|p| p.constraints.len()).unwrap_or(0),
        })
    }

    pub async fn finalize(
        State(state): State<AppState>,
        Path(org_id): Path<Uuid>,
    ) -> impl axum::response::IntoResponse {
        let org = CustomerId(org_id);
        let policy = state.onboard.take_policy(org);
        let is_complete = state.onboard.is_complete(org);

        if let Some(policy) = policy {
            if is_complete {
                let count = policy.constraints.len();
                let meta = state.onboard.session_meta(org);
                let sector = meta.as_ref().and_then(|m| m.detected_sector.map(|s| s.name().to_string()));
                let frameworks = meta.map(|m| m.detected_frameworks).unwrap_or_default();

                state.policy_store.set_policy(org, policy);

                return json_response(&FinalizeResponse {
                    activated: true,
                    constraints_count: count,
                    sector,
                    frameworks,
                });
            }
        }

        json_response(&FinalizeResponse {
            activated: false,
            constraints_count: 0,
            sector: None,
            frameworks: Vec::new(),
        })
    }

    fn json_response<T: serde::Serialize>(value: &T) -> impl axum::response::IntoResponse {
        match serde_json::to_string(value) {
            Ok(json) => (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                json,
            ),
            Err(_) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"error":"serialization failed"}"#.to_string(),
            ),
        }
    }
}

/// Policy Studio web UI — served as a single embedded HTML file.
mod ui {
    use axum::response::Html;

    const UI_HTML: &str = include_str!("../ui/index.html");

    pub async fn serve_ui() -> Html<&'static str> {
        Html(UI_HTML)
    }
}

//! Trace - Ultra-low-latency HTTP proxy for LLM request evaluation
//!
//! Entry point for the Stria Systems Trace proxy server.

use stria_trace::run;

#[tokio::main]
async fn main() {
    run().await;
}

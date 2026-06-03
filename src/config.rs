//! Configuration management for the Trace proxy.
//!
//! Supports layered configuration from:
//! 1. Default values
//! 2. `trace.toml` or `trace.yaml` config file
//! 3. Environment variables (prefixed with `TRACE_`)

use crate::types::ProxyConfig;
use std::path::Path;

/// Load configuration from file and environment.
///
/// Resolution order (later overrides earlier):
/// 1. Built-in defaults
/// 2. `trace.toml` / `trace.yaml` in the working directory
/// 3. Environment variables prefixed with `TRACE_`
pub fn load_config() -> ProxyConfig {
    let mut builder = config::Config::builder();

    // Layer 1: optional config file
    let file_sources = ["trace.toml", "trace.yaml", "trace.json"];
    for file in &file_sources {
        if Path::new(file).exists() {
            builder = builder.add_source(config::File::with_name(file));
            break;
        }
    }

    // Layer 2: environment variables
    builder = builder.add_source(
        config::Environment::with_prefix("TRACE")
            .separator("_")
            .try_parsing(true),
    );

    match builder.build() {
        Ok(cfg) => match cfg.try_deserialize() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to deserialize config: {e}. Using defaults.");
                default_config()
            }
        },
        Err(e) => {
            tracing::warn!("Failed to build config: {e}. Using defaults.");
            default_config()
        }
    }
}

fn default_config() -> ProxyConfig {
    ProxyConfig {
        bind_address: "0.0.0.0".to_string(),
        port: 8080,
        upstream_url: "http://localhost:11434".to_string(),
        max_body_size: 1024 * 1024,
        timeout_ms: 30000,
        max_evaluation_micros: 15000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_values() {
        let cfg = default_config();
        assert_eq!(cfg.bind_address, "0.0.0.0");
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.max_body_size, 1024 * 1024);
        assert_eq!(cfg.timeout_ms, 30000);
        assert_eq!(cfg.max_evaluation_micros, 15000);
    }
}

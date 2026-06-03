//! Trajectory Engine - Semantic evaluation of LLM payloads.
//!
//! This module provides the interface for evaluating incoming payloads
//! against customer-defined policy constraints.

pub mod evaluator;

pub use evaluator::TrajectoryEngine;

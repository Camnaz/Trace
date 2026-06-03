//! Trajectory Engine - Semantic evaluation of LLM payloads.
//!
//! This module provides the interface for evaluating incoming payloads
//! against customer-defined policy constraints.

pub mod evaluator;
pub mod synthetic;
pub mod shell;
pub mod verify;
pub mod provision;
pub mod factory;
pub mod sectors;
pub mod entropy;
pub mod regulatory;
pub mod onboard;

pub use evaluator::TrajectoryEngine;
pub use synthetic::{generate_probes, ProbeTechnique, SyntheticProbe};
pub use shell::{CorpusStore, ShellHandle, TrainingState};
pub use verify::{run_verification, VerificationReport};
pub use provision::synthesize_policy;
pub use factory::{FactoryControl, FactoryStatus};
pub use sectors::{sector_prompt_pool, find_scenario, Scenario, ScenarioKind, Sector};
pub use onboard::{OnboardAgent, AgentReply, OnboardStage, OnboardSession};

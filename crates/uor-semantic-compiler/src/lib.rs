//! Offline model-observation compiler and measured parity evaluator.
//!
//! This crate may allocate and use rich tooling because it never participates
//! in deployed inference. The generated artifact is consumed by the strict
//! `uor-semantic` runtime through borrowed bytes and fixed-capacity scratch.

#![deny(missing_docs)]

mod compiler;
mod observation;
mod parity;
mod rollout;
mod sha256;

pub use compiler::{CompileError, CompiledArtifact, CompilerConfig, compile};
pub use observation::{
    Observation, ObservationCorpus, ObservationError, ObservationMetadata, ObservedEmission,
};
pub use parity::{
    ParityError, ParityReport, ParityThresholds, RolloutParityReport, evaluate,
    evaluate_graph_only, evaluate_rollouts,
};
pub use rollout::{MAX_ROLLOUT_TOKENS, Rollout, RolloutCorpus, RolloutError, RolloutMetadata};

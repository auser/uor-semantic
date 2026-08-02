//! Bounded semantic paths and overlapping region routing.
//!
//! `uor-semantic` is a strict `no_std`, no-heap runtime core. It represents a
//! semantic coordinate as one or more divisible paths under a content-addressed
//! codebook identity, and routes packed binary context codes into a bounded
//! cloud of overlapping regions using masked Hamming distance.
//!
//! # Memory profile
//!
//! The crate is strict heapless: it does not import `alloc`, owns no heap-backed
//! type, and accepts caller-owned arrays and slices. Capacity exhaustion and
//! bounded top-K truncation are explicit outcomes.
//!
//! # Runtime operation profile
//!
//! The reference routing kernel uses bitwise XOR and AND, population count,
//! integer addition and subtraction, comparisons, bounded branches, and table
//! access. The repository conformance gate scans the shipped source for the
//! forbidden operation families declared in `AGENTS.md`.
//!
//! # Example
//!
//! ```
//! use uor_semantic::{
//!     CandidateSet, Depth, OperationCensus, PathId, ReferenceRouter, Region,
//!     RegionId, RouteCloud,
//! };
//!
//! # fn main() -> Result<(), uor_semantic::CandidateSetError> {
//! let regions = [Region::new(
//!     RegionId::new(1),
//!     PathId::new(11),
//!     Depth::new(4),
//!     [0b0011],
//!     [0b1111],
//!     1,
//! )];
//! let candidates = CandidateSet::<1, 4>::new(&regions)?;
//! let mut cloud = RouteCloud::<4>::new();
//! let mut census = OperationCensus::new();
//!
//! let summary = ReferenceRouter::route(
//!     &[0b0010],
//!     candidates,
//!     &mut cloud,
//!     &mut census,
//! );
//!
//! assert_eq!(summary.retained(), 1);
//! assert_eq!(cloud.first().map(|entry| entry.region_id()), Some(RegionId::new(1)));
//! # Ok(())
//! # }
//! ```

#![no_std]
#![forbid(unsafe_code)]
#![deny(clippy::alloc_instead_of_core)]
#![deny(clippy::float_arithmetic)]
#![deny(clippy::std_instead_of_core)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![deny(clippy::unwrap_used)]

mod address;
mod artifact;
mod census;
mod cloud;
mod compatibility;
mod generation;
mod graph;
mod ids;
mod path;
mod r4g1;
mod region;
mod router;
mod scoring;

pub use address::{AddressInsertError, AddressedPath, SemanticAddressBundle};
pub use artifact::{
    ARTIFACT_MAGIC, ARTIFACT_VERSION, ArtifactError, ArtifactPredictScratch, ArtifactView,
    EMISSION_RECORD_BYTES, EXACT_RECORD_BYTES, ExactPolicy, HEADER_BYTES, INDEX_BUCKETS,
    MAX_ARTIFACT_BYTES, MAX_CONTEXT_TOKENS, MAX_EMISSION_RECORDS, MAX_EXACT_RECORDS,
    MAX_REGION_INDEX_ENTRIES, MAX_REGION_RECORDS, MAX_ROUTE_DEPTH, Prediction, PredictionSource,
    PredictionSummary, R4G1_SIGNATURE_WORDS, REGION_RECORD_BYTES, SIGNATURE_WORDS, TokenScore,
    context_hash, context_signature, context_signature_r4g1,
};
pub use census::{OperationBudget, OperationCensus};
pub use cloud::{RegionMembership, ResolutionStatus, RouteCloud};
pub use compatibility::{
    CompatibilityError, CompatibilityFormat, CompatibilityManifest, CompatibilityPrediction,
    CompatibilityWitness, IdentityField, R4Status, WitnessField,
};
pub use generation::{
    GenerationError, GenerationState, GenerationStop, GenerationSummary, generate_greedy_into,
};
pub use graph::{OverlapEdge, RefinementEdge, ScoreQ, TransitionEdge};
pub use ids::{CodebookId, Depth, MembershipMargin, PathId, RegionId, SemanticSlot};
pub use path::{PathError, SemanticPath, SemanticPathView};
pub use r4g1::{
    R4G1Edge, R4G1Emission, R4G1Emissions, R4G1Error, R4G1Graph, R4G1Identity, R4G1Node,
    R4G1Predictions, R4G1RangeField, R4G1RouteCandidates, R4G1Section, R4G1Structure,
};
pub use region::{Region, masked_hamming};
pub use router::{CandidateSet, CandidateSetError, ReferenceRouter, RouteSummary};
pub use scoring::{ResidualContribution, ResidualContributionKind, ScoreAccumulator, ScoringError};

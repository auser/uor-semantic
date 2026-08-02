//! Empirical benchmark reporting without a machine-independent time claim.

use core::fmt;
use std::hint::black_box;
use std::time::Instant;

use uor_semantic::{CandidateSet, OperationCensus, ReferenceRouter, RouteCloud};

use crate::fixture::{AMBIGUOUS_STOP, REGIONS};

/// Configuration for one empirical routing benchmark.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BenchmarkConfig {
    samples: usize,
    iterations_per_sample: usize,
}

impl BenchmarkConfig {
    /// Creates a benchmark configuration after checking non-zero dimensions.
    pub const fn new(samples: usize, iterations_per_sample: usize) -> Result<Self, BenchmarkError> {
        if samples == 0 {
            return Err(BenchmarkError::ZeroSamples);
        }
        if iterations_per_sample == 0 {
            return Err(BenchmarkError::ZeroIterations);
        }
        if samples.checked_mul(iterations_per_sample).is_none() {
            return Err(BenchmarkError::RouteCountOverflow);
        }
        Ok(Self {
            samples,
            iterations_per_sample,
        })
    }

    /// Returns the number of independent timing samples.
    pub const fn samples(self) -> usize {
        self.samples
    }

    /// Returns the route calls performed in each sample.
    pub const fn iterations_per_sample(self) -> usize {
        self.iterations_per_sample
    }
}

/// Invalid benchmark configuration or fixture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BenchmarkError {
    /// At least one timing sample is required.
    ZeroSamples,
    /// At least one route call per sample is required.
    ZeroIterations,
    /// The requested total route count does not fit in `usize`.
    RouteCountOverflow,
    /// The compiled fixture violated its declared candidate bound.
    InvalidFixture,
}

impl fmt::Display for BenchmarkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroSamples => formatter.write_str("benchmark samples must be non-zero"),
            Self::ZeroIterations => formatter.write_str("benchmark iterations must be non-zero"),
            Self::RouteCountOverflow => formatter.write_str("benchmark route count exceeds usize"),
            Self::InvalidFixture => formatter.write_str("benchmark fixture is invalid"),
        }
    }
}

impl std::error::Error for BenchmarkError {}

/// One hardware-specific empirical benchmark report.
///
/// Timing fields are observations, not portable guarantees. Deterministic
/// operation budgets are enforced separately by conformance tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenchmarkReport {
    /// Target architecture reported by the Rust standard library.
    pub target_arch: &'static str,
    /// Number of independent samples.
    pub samples: usize,
    /// Calls in each sample.
    pub iterations_per_sample: usize,
    /// Total route calls represented by the report.
    pub total_routes: usize,
    /// Smallest observed nanoseconds per route.
    pub min_ns_per_route: u128,
    /// Median observed nanoseconds per route.
    pub median_ns_per_route: u128,
    /// Largest observed nanoseconds per route.
    pub max_ns_per_route: u128,
    /// Candidate regions scanned per route.
    pub candidates_per_route: u64,
    /// Packed words processed per route.
    pub words_per_route: u64,
}

impl BenchmarkReport {
    /// Renders a stable single-object JSON representation without a serializer
    /// dependency in the shipped crate.
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\n",
                "  \"target_arch\": \"{}\",\n",
                "  \"samples\": {},\n",
                "  \"iterations_per_sample\": {},\n",
                "  \"total_routes\": {},\n",
                "  \"min_ns_per_route\": {},\n",
                "  \"median_ns_per_route\": {},\n",
                "  \"max_ns_per_route\": {},\n",
                "  \"candidates_per_route\": {},\n",
                "  \"words_per_route\": {}\n",
                "}}"
            ),
            self.target_arch,
            self.samples,
            self.iterations_per_sample,
            self.total_routes,
            self.min_ns_per_route,
            self.median_ns_per_route,
            self.max_ns_per_route,
            self.candidates_per_route,
            self.words_per_route,
        )
    }
}

/// Runs the deterministic fixture and records hardware-specific elapsed time.
///
/// The function intentionally does not compare timing with a universal
/// threshold. Use the emitted report with a pinned runner and a pinned baseline
/// when enforcing project-specific regressions.
pub fn run_route_benchmark(config: BenchmarkConfig) -> Result<BenchmarkReport, BenchmarkError> {
    let candidates =
        CandidateSet::<1, 3>::new(&REGIONS).map_err(|_error| BenchmarkError::InvalidFixture)?;
    let mut cloud = RouteCloud::<3>::new();
    let mut census = OperationCensus::new();
    let mut samples = Vec::with_capacity(config.samples);

    for _sample in 0..config.samples {
        let started = Instant::now();
        for _iteration in 0..config.iterations_per_sample {
            let summary = ReferenceRouter::route(
                black_box(&AMBIGUOUS_STOP),
                candidates,
                &mut cloud,
                &mut census,
            );
            let _ = black_box(summary);
            let _ = black_box(&cloud);
            let _ = black_box(&census);
        }
        let elapsed = started.elapsed().as_nanos();
        samples.push(elapsed / config.iterations_per_sample as u128);
    }

    samples.sort_unstable();
    let Some(min_ns_per_route) = samples.first().copied() else {
        return Err(BenchmarkError::ZeroSamples);
    };
    let Some(median_ns_per_route) = samples.get(samples.len() / 2).copied() else {
        return Err(BenchmarkError::ZeroSamples);
    };
    let Some(max_ns_per_route) = samples.last().copied() else {
        return Err(BenchmarkError::ZeroSamples);
    };

    let total_routes = config
        .samples
        .checked_mul(config.iterations_per_sample)
        .ok_or(BenchmarkError::RouteCountOverflow)?;

    Ok(BenchmarkReport {
        target_arch: std::env::consts::ARCH,
        samples: config.samples,
        iterations_per_sample: config.iterations_per_sample,
        total_routes,
        min_ns_per_route,
        median_ns_per_route,
        max_ns_per_route,
        candidates_per_route: census.candidate_scans(),
        words_per_route: census.word_steps(),
    })
}

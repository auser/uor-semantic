//! Allocation-free greedy token generation over a validated artifact.

use core::fmt;

use crate::{
    ArtifactError, ArtifactPredictScratch, ArtifactView, ExactPolicy, Prediction, PredictionSource,
};

/// Failure to initialize fixed-capacity generation state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationError {
    /// The state capacity exceeds the artifact format context limit.
    StateCapacityTooLarge {
        /// Requested compile-time state capacity.
        capacity: usize,
    },
    /// Artifact prediction failed.
    Artifact(ArtifactError),
}

impl fmt::Display for GenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateCapacityTooLarge { capacity } => write!(
                formatter,
                "generation state capacity {capacity} exceeds the artifact context limit"
            ),
            Self::Artifact(error) => write!(formatter, "artifact prediction failed: {error}"),
        }
    }
}

impl core::error::Error for GenerationError {}

impl From<ArtifactError> for GenerationError {
    fn from(error: ArtifactError) -> Self {
        Self::Artifact(error)
    }
}

/// Reason greedy generation stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationStop {
    /// Every caller-owned output slot was written.
    OutputFull,
    /// The artifact had no exact or graph evidence for the next token.
    Novel,
}

/// Summary of one bounded greedy-generation call.
#[must_use = "generation length and stop reason must be inspected"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationSummary {
    written: usize,
    stop: GenerationStop,
    exact_steps: usize,
    graph_steps: usize,
}

impl GenerationSummary {
    /// Returns output tokens written.
    pub const fn written(self) -> usize {
        self.written
    }

    /// Returns the stop reason.
    pub const fn stop(self) -> GenerationStop {
        self.stop
    }

    /// Returns steps served by exact compiled contexts.
    pub const fn exact_steps(self) -> usize {
        self.exact_steps
    }

    /// Returns steps served by overlapping semantic regions.
    pub const fn graph_steps(self) -> usize {
        self.graph_steps
    }
}

/// Fixed-capacity rolling token context.
#[must_use = "generation state carries the current context"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationState<const CAPACITY: usize> {
    tokens: [u32; CAPACITY],
    len: usize,
}

impl<const CAPACITY: usize> GenerationState<CAPACITY> {
    /// Creates empty state when capacity fits the artifact format.
    pub const fn new() -> Result<Self, GenerationError> {
        if CAPACITY > crate::MAX_CONTEXT_TOKENS {
            return Err(GenerationError::StateCapacityTooLarge { capacity: CAPACITY });
        }
        Ok(Self {
            tokens: [0u32; CAPACITY],
            len: 0,
        })
    }

    /// Replaces state with the suffix of `prompt` that fits capacity.
    pub fn seed(&mut self, prompt: &[u32]) {
        self.len = 0;
        if CAPACITY == 0 {
            return;
        }
        let start = prompt.len().saturating_sub(CAPACITY);
        let mut index = start;
        while index < prompt.len() {
            self.push(prompt[index]);
            index += 1;
        }
    }

    /// Returns initialized context tokens.
    pub fn as_slice(&self) -> &[u32] {
        &self.tokens[..self.len]
    }

    /// Pushes one token, dropping the oldest token at capacity.
    pub fn push(&mut self, token: u32) {
        if CAPACITY == 0 {
            return;
        }
        if self.len < CAPACITY {
            self.tokens[self.len] = token;
            self.len += 1;
            return;
        }
        let mut index = 1usize;
        while index < CAPACITY {
            self.tokens[index - 1] = self.tokens[index];
            index += 1;
        }
        self.tokens[CAPACITY - 1] = token;
    }
}

/// Generates greedy token IDs into caller-owned output without heap allocation.
pub fn generate_greedy_into<
    const CONTEXT: usize,
    const MAX_ACTIVE: usize,
    const MAX_OUTPUT_CANDIDATES: usize,
>(
    artifact: &ArtifactView<'_>,
    state: &mut GenerationState<CONTEXT>,
    output: &mut [u32],
    scratch: &mut ArtifactPredictScratch<MAX_ACTIVE>,
    prediction: &mut Prediction<MAX_OUTPUT_CANDIDATES>,
) -> Result<GenerationSummary, GenerationError> {
    let mut written = 0usize;
    let mut exact_steps = 0usize;
    let mut graph_steps = 0usize;

    while written < output.len() {
        let summary = artifact.predict(
            state.as_slice(),
            ExactPolicy::PreferExact,
            scratch,
            prediction,
        )?;
        let Some(next) = prediction.first() else {
            return Ok(GenerationSummary {
                written,
                stop: GenerationStop::Novel,
                exact_steps,
                graph_steps,
            });
        };
        match summary.source() {
            PredictionSource::Exact => exact_steps = exact_steps.saturating_add(1),
            PredictionSource::Graph => graph_steps = graph_steps.saturating_add(1),
            PredictionSource::Novel => {
                return Ok(GenerationSummary {
                    written,
                    stop: GenerationStop::Novel,
                    exact_steps,
                    graph_steps,
                });
            }
        }
        output[written] = next.token();
        state.push(next.token());
        written += 1;
    }

    Ok(GenerationSummary {
        written,
        stop: GenerationStop::OutputFull,
        exact_steps,
        graph_steps,
    })
}

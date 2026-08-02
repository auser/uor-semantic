//! Bounded fixed-point residual scoring for the R4G1 compatibility boundary.
//!
//! The scorer is intentionally small and allocation-free. It applies already
//! quantized signed contributions with saturation, tracks contribution IDs so
//! overlapping evidence is not counted twice, and exposes the target's
//! deterministic score-descending/ID-ascending ordering rule.

use core::cmp::Ordering;
use core::fmt;

/// Semantic class of a fixed-point residual contribution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResidualContributionKind {
    /// Root-node base prior.
    RootPrior,
    /// Hierarchical child correction.
    ChildCorrection,
    /// Interaction residual between co-occurring concepts.
    InteractionResidual,
    /// Goal-satisfaction reward.
    GoalReward,
    /// Constraint or hazard penalty.
    ConstraintPenalty,
    /// Uncertainty or variance penalty.
    UncertaintyPenalty,
    /// Token-emission residual.
    TokenEmission,
}

/// One pre-quantized signed residual contribution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResidualContribution {
    /// Semantic class of the contribution.
    pub kind: ResidualContributionKind,
    /// Stable identity used for overlap de-duplication.
    pub contribution_id: u32,
    /// Signed fixed-point value to add to the accumulator.
    pub raw_value: i32,
}

/// Failure while applying bounded compatibility scoring.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScoringError {
    /// The fixed-capacity evidence ledger cannot retain another unique ID.
    EvidenceCapacityExceeded,
}

impl fmt::Display for ScoringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EvidenceCapacityExceeded => {
                formatter.write_str("R4G1 scoring evidence capacity exceeded")
            }
        }
    }
}

impl core::error::Error for ScoringError {}

/// Fixed-capacity R4G1 residual score accumulator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreAccumulator<const MAX_EVIDENCE: usize = 32> {
    current_score: i32,
    tracked_evidence: [u32; MAX_EVIDENCE],
    evidence_count: usize,
}

impl<const MAX_EVIDENCE: usize> Default for ScoreAccumulator<MAX_EVIDENCE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const MAX_EVIDENCE: usize> ScoreAccumulator<MAX_EVIDENCE> {
    /// Creates an empty accumulator with a zero score.
    pub const fn new() -> Self {
        Self {
            current_score: 0,
            tracked_evidence: [0; MAX_EVIDENCE],
            evidence_count: 0,
        }
    }

    /// Returns the saturated accumulated score.
    pub const fn score(&self) -> i32 {
        self.current_score
    }

    /// Returns the number of unique accepted contribution IDs.
    pub const fn evidence_count(&self) -> usize {
        self.evidence_count
    }

    /// Returns whether a contribution ID has already been accepted.
    pub fn contains_evidence(&self, contribution_id: u32) -> bool {
        let mut index = 0usize;
        while index < self.evidence_count {
            if self.tracked_evidence[index] == contribution_id {
                return true;
            }
            index += 1;
        }
        false
    }

    /// Adds one unique signed contribution.
    ///
    /// Returns `Ok(false)` for a duplicate ID, `Ok(true)` when the evidence
    /// was applied, and a typed capacity error when a new ID cannot fit.
    pub fn accumulate(&mut self, contribution: ResidualContribution) -> Result<bool, ScoringError> {
        if self.contains_evidence(contribution.contribution_id) {
            return Ok(false);
        }
        if self.evidence_count >= MAX_EVIDENCE {
            return Err(ScoringError::EvidenceCapacityExceeded);
        }
        self.current_score = self.current_score.saturating_add(contribution.raw_value);
        self.tracked_evidence[self.evidence_count] = contribution.contribution_id;
        self.evidence_count += 1;
        Ok(true)
    }

    /// Compares candidates by descending score, then ascending candidate ID.
    pub fn compare_candidates(score_a: i32, id_a: u32, score_b: i32, id_b: u32) -> Ordering {
        match score_b.cmp(&score_a) {
            Ordering::Equal => id_a.cmp(&id_b),
            ordering => ordering,
        }
    }
}

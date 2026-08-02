//! Compact relationship records for the semantic-region graph.

use crate::RegionId;

/// Signed fixed-point score stored without floating-point arithmetic.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ScoreQ(i32);

impl ScoreQ {
    /// Creates a fixed-point score from its canonical stored integer.
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    /// Returns the canonical stored integer.
    pub const fn raw(self) -> i32 {
        self.0
    }
}

/// Parent-child relationship implementing coarse-to-fine refinement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RefinementEdge {
    /// Broader semantic region.
    pub parent: RegionId,
    /// More precise semantic region.
    pub child: RegionId,
}

/// Lateral relationship between co-active or adjacent semantic regions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverlapEdge {
    /// Canonically smaller endpoint identifier.
    pub left: RegionId,
    /// Canonically larger endpoint identifier.
    pub right: RegionId,
    /// Fixed-point overlap strength.
    pub strength: ScoreQ,
}

/// Directed predictive relationship between semantic regions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionEdge {
    /// Current semantic region.
    pub from: RegionId,
    /// Predicted successor region.
    pub to: RegionId,
    /// Fixed-point transition score.
    pub score: ScoreQ,
}

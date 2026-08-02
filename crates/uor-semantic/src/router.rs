//! Bounded reference routing over validated candidate sets.

use core::fmt;

use crate::region::masked_hamming_counted;
use crate::{
    MembershipMargin, OperationCensus, Region, RegionMembership, ResolutionStatus, RouteCloud,
};

/// Failure to construct a bounded candidate set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateSetError {
    /// The supplied region slice exceeds its declared maximum.
    CapacityExceeded {
        /// Number of supplied regions.
        provided: usize,
        /// Declared maximum regions.
        maximum: usize,
    },
    /// Two supplied region descriptors use the same stable identifier.
    DuplicateRegion {
        /// Repeated identifier.
        region_id: crate::RegionId,
    },
}

impl fmt::Display for CandidateSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapacityExceeded { provided, maximum } => write!(
                formatter,
                "candidate count {provided} exceeds declared maximum {maximum}"
            ),
            Self::DuplicateRegion { region_id } => write!(
                formatter,
                "candidate set repeats region identifier {}",
                region_id.get()
            ),
        }
    }
}

impl core::error::Error for CandidateSetError {}

/// Borrowed candidate regions with an explicit compile-time scan bound.
#[must_use = "validated candidates carry the routing scan bound"]
#[derive(Clone, Copy, Debug)]
pub struct CandidateSet<'a, const WORDS: usize, const MAX_CANDIDATES: usize> {
    regions: &'a [Region<WORDS>],
}

impl<'a, const WORDS: usize, const MAX_CANDIDATES: usize> CandidateSet<'a, WORDS, MAX_CANDIDATES> {
    /// Validates and borrows a candidate slice.
    ///
    /// # Allocation
    ///
    /// Performs no heap allocation.
    ///
    /// # Errors
    ///
    /// Returns [`CandidateSetError::CapacityExceeded`] when the slice is too
    /// long and [`CandidateSetError::DuplicateRegion`] when identifiers repeat.
    /// The duplicate check performs bounded quadratic setup work; repeated
    /// routing over the validated value is linear in the supplied candidate
    /// count.
    pub fn new(regions: &'a [Region<WORDS>]) -> Result<Self, CandidateSetError> {
        if regions.len() > MAX_CANDIDATES {
            return Err(CandidateSetError::CapacityExceeded {
                provided: regions.len(),
                maximum: MAX_CANDIDATES,
            });
        }

        let mut left = 0usize;
        while left < regions.len() {
            let mut right = left + 1;
            while right < regions.len() {
                if regions[left].id() == regions[right].id() {
                    return Err(CandidateSetError::DuplicateRegion {
                        region_id: regions[left].id(),
                    });
                }
                right += 1;
            }
            left += 1;
        }

        Ok(Self { regions })
    }

    /// Returns the validated number of candidate regions.
    pub const fn len(&self) -> usize {
        self.regions.len()
    }

    /// Returns whether the candidate set is empty.
    pub const fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    pub(crate) const fn regions(&self) -> &'a [Region<WORDS>] {
        self.regions
    }
}

/// Summary of one bounded route resolution.
#[must_use = "routing status and truncation must be inspected"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteSummary {
    resolution: ResolutionStatus,
    candidates_scanned: usize,
    matched: usize,
    retained: usize,
    truncated: usize,
}

impl RouteSummary {
    /// Returns the resolution class.
    pub const fn resolution(self) -> ResolutionStatus {
        self.resolution
    }

    /// Returns the validated candidates inspected.
    pub const fn candidates_scanned(self) -> usize {
        self.candidates_scanned
    }

    /// Returns the total regions whose inclusive boundary accepted the input.
    pub const fn matched(self) -> usize {
        self.matched
    }

    /// Returns the memberships retained in caller-owned storage.
    pub const fn retained(self) -> usize {
        self.retained
    }

    /// Returns accepted memberships omitted by the declared top-K capacity.
    pub const fn truncated(self) -> usize {
        self.truncated
    }

    /// Returns whether the active cloud omitted any accepted membership.
    pub const fn was_truncated(self) -> bool {
        self.truncated != 0
    }
}

/// Normative safe scalar routing implementation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReferenceRouter;

impl ReferenceRouter {
    /// Routes one packed context code into a bounded overlapping cloud.
    ///
    /// # Allocation
    ///
    /// Performs no heap allocation. The output cloud and operation census are
    /// caller-owned and reused in place.
    ///
    /// # Bounded work
    ///
    /// At most `MAX_CANDIDATES` regions and `WORDS` packed words per region are
    /// inspected. At most `MAX_ACTIVE` memberships are retained.
    ///
    /// # Determinism
    ///
    /// Input order cannot change retained ordering because every accepted
    /// membership is inserted under a total order.
    pub fn route<const WORDS: usize, const MAX_CANDIDATES: usize, const MAX_ACTIVE: usize>(
        code: &[u64; WORDS],
        candidates: CandidateSet<'_, WORDS, MAX_CANDIDATES>,
        cloud: &mut RouteCloud<MAX_ACTIVE>,
        census: &mut OperationCensus,
    ) -> RouteSummary {
        cloud.clear();
        census.clear();

        let mut matched = 0usize;
        for region in candidates.regions() {
            census.record_candidate_scan();
            let distance = masked_hamming_counted(code, region.prototype(), region.mask(), census);
            census.record_comparison();
            if distance > region.radius() {
                continue;
            }

            matched += 1;
            debug_assert!(distance <= region.radius());
            let margin = MembershipMargin::new(region.radius().saturating_sub(distance));
            let membership = RegionMembership::new(
                region.id(),
                region.path_id(),
                region.depth(),
                margin,
                distance,
            );
            let report = cloud.insert_ranked(membership);
            census.record_insert(report.comparisons, report.writes);
        }

        let retained = cloud.len();
        let truncated = matched.saturating_sub(retained);
        let resolution = if matched == 0 {
            ResolutionStatus::Novel
        } else if matched == 1 && retained == 1 {
            ResolutionStatus::Supported
        } else {
            ResolutionStatus::Boundary
        };

        RouteSummary {
            resolution,
            candidates_scanned: candidates.len(),
            matched,
            retained,
            truncated,
        }
    }
}

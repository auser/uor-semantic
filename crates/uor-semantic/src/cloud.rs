//! Fixed-capacity clouds of overlapping semantic memberships.

use crate::{Depth, MembershipMargin, PathId, RegionId};

/// Outcome class for one route resolution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolutionStatus {
    /// Exactly one retained region covers the input.
    Supported,
    /// Several regions cover the input, or capacity truncated the active cloud.
    Boundary,
    /// Only a broader region could cover the input.
    BackedOff,
    /// No calibrated region covers the input.
    Novel,
    /// Retained regions carry incompatible predictions.
    Contradictory,
}

/// One accepted relationship between an input context and a semantic region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionMembership {
    region_id: RegionId,
    path_id: PathId,
    depth: Depth,
    margin: MembershipMargin,
    distance: u64,
}

impl RegionMembership {
    pub(crate) const EMPTY: Self = Self {
        region_id: RegionId::new(0),
        path_id: PathId::new(0),
        depth: Depth::new(0),
        margin: MembershipMargin::new(0),
        distance: 0,
    };

    pub(crate) const fn new(
        region_id: RegionId,
        path_id: PathId,
        depth: Depth,
        margin: MembershipMargin,
        distance: u64,
    ) -> Self {
        Self {
            region_id,
            path_id,
            depth,
            margin,
            distance,
        }
    }

    /// Returns the semantic region identifier.
    pub const fn region_id(self) -> RegionId {
        self.region_id
    }

    /// Returns the refinement path identifier.
    pub const fn path_id(self) -> PathId {
        self.path_id
    }

    /// Returns the region depth.
    pub const fn depth(self) -> Depth {
        self.depth
    }

    /// Returns the accepted membership margin.
    pub const fn margin(self) -> MembershipMargin {
        self.margin
    }

    /// Returns the masked Hamming distance to the region prototype.
    pub const fn distance(self) -> u64 {
        self.distance
    }
}

/// Caller-owned, fixed-capacity active semantic cloud.
///
/// Memberships are stored under the canonical order: descending margin,
/// descending depth, ascending region ID, then ascending path ID.
#[must_use = "a route cloud contains the retained semantic memberships"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteCloud<const CAPACITY: usize> {
    entries: [RegionMembership; CAPACITY],
    len: usize,
}

impl<const CAPACITY: usize> RouteCloud<CAPACITY> {
    /// Creates an empty route cloud.
    pub const fn new() -> Self {
        Self {
            entries: [RegionMembership::EMPTY; CAPACITY],
            len: 0,
        }
    }

    /// Returns the number of retained memberships.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns whether the active cloud is empty.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Borrows the retained memberships in canonical order.
    pub fn as_slice(&self) -> &[RegionMembership] {
        &self.entries[..self.len]
    }

    /// Returns the highest-ranked retained membership.
    pub fn first(&self) -> Option<RegionMembership> {
        self.as_slice().first().copied()
    }

    /// Clears initialized length while retaining caller-owned storage.
    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub(crate) fn insert_ranked(&mut self, candidate: RegionMembership) -> InsertReport {
        if CAPACITY == 0 {
            return InsertReport {
                comparisons: 0,
                writes: 0,
            };
        }

        let mut position = 0usize;
        let mut comparisons = 0u64;
        while position < self.len {
            comparisons = comparisons.saturating_add(1);
            if precedes(candidate, self.entries[position]) {
                break;
            }
            position += 1;
        }

        if self.len < CAPACITY {
            let mut index = self.len;
            let mut writes = 1u64;
            while index > position {
                self.entries[index] = self.entries[index - 1];
                writes = writes.saturating_add(1);
                index -= 1;
            }
            self.entries[position] = candidate;
            self.len += 1;
            debug_assert!(self.len <= CAPACITY);
            return InsertReport {
                comparisons,
                writes,
            };
        }

        if position == CAPACITY {
            return InsertReport {
                comparisons,
                writes: 0,
            };
        }

        let mut index = self.len - 1;
        let mut writes = 1u64;
        while index > position {
            self.entries[index] = self.entries[index - 1];
            writes = writes.saturating_add(1);
            index -= 1;
        }
        self.entries[position] = candidate;
        InsertReport {
            comparisons,
            writes,
        }
    }
}

impl<const CAPACITY: usize> Default for RouteCloud<CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InsertReport {
    pub(crate) comparisons: u64,
    pub(crate) writes: u64,
}

fn precedes(left: RegionMembership, right: RegionMembership) -> bool {
    if left.margin != right.margin {
        return left.margin > right.margin;
    }
    if left.depth != right.depth {
        return left.depth > right.depth;
    }
    if left.region_id != right.region_id {
        return left.region_id < right.region_id;
    }
    left.path_id < right.path_id
}

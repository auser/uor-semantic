//! Packed binary semantic regions and masked Hamming distance.

use crate::{Depth, OperationCensus, PathId, RegionId};

/// One calibrated region in a packed binary semantic code space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Region<const WORDS: usize> {
    id: RegionId,
    path_id: PathId,
    depth: Depth,
    prototype: [u64; WORDS],
    mask: [u64; WORDS],
    radius: u64,
}

impl<const WORDS: usize> Region<WORDS> {
    /// Creates a region descriptor.
    pub const fn new(
        id: RegionId,
        path_id: PathId,
        depth: Depth,
        prototype: [u64; WORDS],
        mask: [u64; WORDS],
        radius: u64,
    ) -> Self {
        Self {
            id,
            path_id,
            depth,
            prototype,
            mask,
            radius,
        }
    }

    /// Returns the region identifier.
    pub const fn id(&self) -> RegionId {
        self.id
    }

    /// Returns the refinement path identifier.
    pub const fn path_id(&self) -> PathId {
        self.path_id
    }

    /// Returns the region depth.
    pub const fn depth(&self) -> Depth {
        self.depth
    }

    /// Borrows the packed prototype words.
    pub const fn prototype(&self) -> &[u64; WORDS] {
        &self.prototype
    }

    /// Borrows the packed comparison mask.
    pub const fn mask(&self) -> &[u64; WORDS] {
        &self.mask
    }

    /// Returns the inclusive calibrated Hamming radius.
    pub const fn radius(&self) -> u64 {
        self.radius
    }
}

/// Computes masked Hamming distance using the normative scalar kernel.
///
/// # Allocation
///
/// Performs no heap allocation.
///
/// # Determinism
///
/// Words are consumed from index zero upward and distance accumulation uses
/// saturating unsigned addition. The result is architecture-independent.
pub fn masked_hamming<const WORDS: usize>(
    code: &[u64; WORDS],
    prototype: &[u64; WORDS],
    mask: &[u64; WORDS],
) -> u64 {
    let mut distance = 0u64;
    let mut index = 0usize;
    while index < WORDS {
        let delta = (code[index] ^ prototype[index]) & mask[index];
        distance = distance.saturating_add(u64::from(delta.count_ones()));
        index += 1;
    }
    distance
}

pub(crate) fn masked_hamming_counted<const WORDS: usize>(
    code: &[u64; WORDS],
    prototype: &[u64; WORDS],
    mask: &[u64; WORDS],
    census: &mut OperationCensus,
) -> u64 {
    let mut distance = 0u64;
    let mut index = 0usize;
    while index < WORDS {
        let delta = (code[index] ^ prototype[index]) & mask[index];
        distance = distance.saturating_add(u64::from(delta.count_ones()));
        census.record_word_step();
        index += 1;
    }
    distance
}

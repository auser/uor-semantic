//! Content-identified bundles of overlapping semantic paths.

use core::fmt;

use crate::{CodebookId, MembershipMargin, SemanticPath};

/// One path retained in a semantic address bundle.
#[must_use = "an addressed path carries semantic membership evidence"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressedPath<const MAX_DEPTH: usize> {
    path: SemanticPath<MAX_DEPTH>,
    margin: MembershipMargin,
}

impl<const MAX_DEPTH: usize> AddressedPath<MAX_DEPTH> {
    /// Creates a path and its non-negative membership margin.
    pub const fn new(path: SemanticPath<MAX_DEPTH>, margin: MembershipMargin) -> Self {
        Self { path, margin }
    }

    /// Borrows the divisible path.
    pub const fn path(&self) -> &SemanticPath<MAX_DEPTH> {
        &self.path
    }

    /// Returns the membership margin used for canonical ordering.
    pub const fn margin(&self) -> MembershipMargin {
        self.margin
    }

    const EMPTY: Self = Self {
        path: SemanticPath::new(),
        margin: MembershipMargin::new(0),
    };
}

/// Failure to add an overlapping path to a semantic address bundle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressInsertError {
    /// The bundle is already at its fixed path capacity.
    CapacityExceeded {
        /// Configured path capacity.
        capacity: usize,
    },
    /// The exact path is already present.
    DuplicatePath,
}

impl fmt::Display for AddressInsertError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapacityExceeded { capacity } => {
                write!(
                    formatter,
                    "semantic address capacity {capacity} is exhausted"
                )
            }
            Self::DuplicatePath => formatter.write_str("semantic path is already present"),
        }
    }
}

impl core::error::Error for AddressInsertError {}

/// A content-identified, bounded collection of overlapping semantic paths.
///
/// Entries are stored canonically by descending membership margin and then by
/// lexicographic slot order. Each path remains independently divisible.
#[must_use = "a semantic address bundle is caller-owned state"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemanticAddressBundle<const MAX_PATHS: usize, const MAX_DEPTH: usize> {
    codebook_id: CodebookId,
    paths: [AddressedPath<MAX_DEPTH>; MAX_PATHS],
    len: usize,
}

impl<const MAX_PATHS: usize, const MAX_DEPTH: usize> SemanticAddressBundle<MAX_PATHS, MAX_DEPTH> {
    /// Creates an empty bundle under a pinned codebook identity.
    pub const fn new(codebook_id: CodebookId) -> Self {
        Self {
            codebook_id,
            paths: [AddressedPath::<MAX_DEPTH>::EMPTY; MAX_PATHS],
            len: 0,
        }
    }

    /// Returns the codebook identity that gives every slot sequence meaning.
    pub const fn codebook_id(&self) -> CodebookId {
        self.codebook_id
    }

    /// Returns the number of initialized paths.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns whether no semantic path is present.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Borrows the canonical initialized path sequence.
    pub fn paths(&self) -> &[AddressedPath<MAX_DEPTH>] {
        &self.paths[..self.len]
    }

    /// Inserts a path into canonical order.
    ///
    /// # Allocation
    ///
    /// Performs no heap allocation. At most `MAX_PATHS` fixed-array entries are
    /// inspected and shifted.
    ///
    /// # Errors
    ///
    /// Returns [`AddressInsertError::DuplicatePath`] for an exact duplicate and
    /// [`AddressInsertError::CapacityExceeded`] when the bundle is full. The
    /// bundle is unchanged on either error.
    pub fn insert(&mut self, entry: AddressedPath<MAX_DEPTH>) -> Result<(), AddressInsertError> {
        let mut position = 0usize;
        while position < self.len {
            if self.paths[position].path == entry.path {
                return Err(AddressInsertError::DuplicatePath);
            }
            if precedes(&entry, &self.paths[position]) {
                break;
            }
            position += 1;
        }

        if self.len == MAX_PATHS {
            return Err(AddressInsertError::CapacityExceeded {
                capacity: MAX_PATHS,
            });
        }

        debug_assert!(self.len < self.paths.len());
        let mut index = self.len;
        while index > position {
            self.paths[index] = self.paths[index - 1];
            index -= 1;
        }
        self.paths[position] = entry;
        self.len += 1;
        debug_assert!(self.len <= MAX_PATHS);
        Ok(())
    }
}

fn precedes<const MAX_DEPTH: usize>(
    left: &AddressedPath<MAX_DEPTH>,
    right: &AddressedPath<MAX_DEPTH>,
) -> bool {
    if left.margin != right.margin {
        return left.margin > right.margin;
    }

    let left_slots = left.path.as_slice();
    let right_slots = right.path.as_slice();
    let shared = core::cmp::min(left_slots.len(), right_slots.len());
    let mut index = 0usize;
    while index < shared {
        if left_slots[index] != right_slots[index] {
            return left_slots[index] < right_slots[index];
        }
        index += 1;
    }
    left_slots.len() < right_slots.len()
}

//! Divisible fixed-capacity semantic paths.

use core::fmt;

use crate::SemanticSlot;

/// Failure to modify or view a semantic path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathError {
    /// The fixed path capacity has been reached.
    CapacityExceeded {
        /// Configured capacity.
        capacity: usize,
    },
    /// A requested prefix is deeper than the initialized path.
    PrefixOutOfBounds {
        /// Requested prefix depth.
        requested: usize,
        /// Initialized path depth.
        available: usize,
    },
}

impl fmt::Display for PathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapacityExceeded { capacity } => {
                write!(formatter, "semantic path capacity {capacity} is exhausted")
            }
            Self::PrefixOutOfBounds {
                requested,
                available,
            } => write!(
                formatter,
                "requested prefix depth {requested} exceeds initialized depth {available}"
            ),
        }
    }
}

impl core::error::Error for PathError {}

/// A fixed-capacity sequence of coarse-to-fine semantic branch choices.
///
/// Every initialized prefix is a valid semantic direction under the codebook
/// that owns the path.
#[must_use = "a semantic path is caller-owned state"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemanticPath<const MAX_DEPTH: usize> {
    slots: [SemanticSlot; MAX_DEPTH],
    len: usize,
}

impl<const MAX_DEPTH: usize> SemanticPath<MAX_DEPTH> {
    /// Creates an empty path.
    pub const fn new() -> Self {
        Self {
            slots: [SemanticSlot::new(0); MAX_DEPTH],
            len: 0,
        }
    }

    /// Returns the initialized depth.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns whether the path is the codebook root.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Borrows the initialized slot sequence.
    pub fn as_slice(&self) -> &[SemanticSlot] {
        &self.slots[..self.len]
    }

    /// Appends one refinement choice.
    ///
    /// # Allocation
    ///
    /// Performs no heap allocation.
    ///
    /// # Errors
    ///
    /// Returns [`PathError::CapacityExceeded`] without changing the path when
    /// `MAX_DEPTH` initialized slots are already present.
    pub fn push(&mut self, slot: SemanticSlot) -> Result<(), PathError> {
        if self.len == MAX_DEPTH {
            return Err(PathError::CapacityExceeded {
                capacity: MAX_DEPTH,
            });
        }

        debug_assert!(self.len < self.slots.len());
        self.slots[self.len] = slot;
        self.len += 1;
        debug_assert!(self.len <= MAX_DEPTH);
        Ok(())
    }

    /// Returns a borrowed prefix view.
    ///
    /// A depth of zero returns the root; a depth equal to [`Self::len`] returns
    /// the complete initialized path.
    ///
    /// # Errors
    ///
    /// Returns [`PathError::PrefixOutOfBounds`] when `depth` exceeds the
    /// initialized path depth.
    pub fn prefix(&self, depth: usize) -> Result<SemanticPathView<'_>, PathError> {
        if depth > self.len {
            return Err(PathError::PrefixOutOfBounds {
                requested: depth,
                available: self.len,
            });
        }

        debug_assert!(depth <= self.len);
        Ok(SemanticPathView {
            slots: &self.slots[..depth],
        })
    }
}

impl<const MAX_DEPTH: usize> Default for SemanticPath<MAX_DEPTH> {
    fn default() -> Self {
        Self::new()
    }
}

/// Borrowed view of one valid semantic-path prefix.
#[must_use = "a semantic prefix view must be inspected"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemanticPathView<'a> {
    slots: &'a [SemanticSlot],
}

impl<'a> SemanticPathView<'a> {
    /// Returns the prefix depth.
    pub const fn len(&self) -> usize {
        self.slots.len()
    }

    /// Returns whether this view denotes the codebook root.
    pub const fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Borrows the prefix slots.
    pub const fn slots(&self) -> &'a [SemanticSlot] {
        self.slots
    }
}

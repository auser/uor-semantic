//! Strongly typed identifiers and scalar values.

/// Stable identifier for a semantic region inside one codebook.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RegionId(u32);

impl RegionId {
    /// Creates an identifier from its canonical integer representation.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the canonical integer representation.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Stable identifier for a refinement path inside one codebook.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PathId(u32);

impl PathId {
    /// Creates an identifier from its canonical integer representation.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the canonical integer representation.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One branch choice in a divisible semantic path.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SemanticSlot(u16);

impl SemanticSlot {
    /// Creates a slot value.
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the canonical integer representation.
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Refinement depth of a semantic region or membership.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Depth(u8);

impl Depth {
    /// Creates a depth value. Depth zero denotes the codebook root.
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// Returns the canonical integer representation.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Non-negative distance from a region boundary for an accepted membership.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MembershipMargin(u64);

impl MembershipMargin {
    /// Creates a margin value.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the canonical integer representation.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Opaque 256-bit identity of the codebook that gives route slots meaning.
///
/// This type intentionally stores identity bytes without depending on a
/// particular content-addressing implementation. An outer adapter can copy the
/// digest bytes produced by `uor-addr` into this value while the strict runtime
/// remains dependency-free.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodebookId([u8; 32]);

impl CodebookId {
    /// Creates an identity from canonical digest bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrows the canonical digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

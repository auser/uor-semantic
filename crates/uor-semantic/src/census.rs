//! Deterministic work accounting for the reference routing kernel.

/// Counts semantic-kernel operations performed by one routing call.
///
/// The census counts the explicit operations of the normative reference
/// algorithm. It does not claim to be an instruction counter and does not count
/// instrumentation arithmetic, loop-control lowering, or address-generation
/// instructions introduced by a compiler.
#[must_use = "the operation census is conformance evidence"]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationCensus {
    candidate_scans: u64,
    word_steps: u64,
    xors: u64,
    ands: u64,
    popcounts: u64,
    distance_adds: u64,
    comparisons: u64,
    table_reads: u64,
    table_writes: u64,
}

impl OperationCensus {
    /// Creates an empty census.
    pub const fn new() -> Self {
        Self {
            candidate_scans: 0,
            word_steps: 0,
            xors: 0,
            ands: 0,
            popcounts: 0,
            distance_adds: 0,
            comparisons: 0,
            table_reads: 0,
            table_writes: 0,
        }
    }

    /// Resets every counter for caller-owned state reuse.
    pub fn clear(&mut self) {
        self.candidate_scans = 0;
        self.word_steps = 0;
        self.xors = 0;
        self.ands = 0;
        self.popcounts = 0;
        self.distance_adds = 0;
        self.comparisons = 0;
        self.table_reads = 0;
        self.table_writes = 0;
    }

    /// Returns the number of candidate regions inspected.
    pub const fn candidate_scans(&self) -> u64 {
        self.candidate_scans
    }

    /// Returns the number of packed words processed.
    pub const fn word_steps(&self) -> u64 {
        self.word_steps
    }

    /// Returns the number of explicit XOR operations.
    pub const fn xors(&self) -> u64 {
        self.xors
    }

    /// Returns the number of explicit AND operations.
    pub const fn ands(&self) -> u64 {
        self.ands
    }

    /// Returns the number of population-count operations.
    pub const fn popcounts(&self) -> u64 {
        self.popcounts
    }

    /// Returns the number of distance-accumulation additions.
    pub const fn distance_adds(&self) -> u64 {
        self.distance_adds
    }

    /// Returns the number of semantic and ordering comparisons.
    pub const fn comparisons(&self) -> u64 {
        self.comparisons
    }

    /// Returns the number of modeled table reads.
    pub const fn table_reads(&self) -> u64 {
        self.table_reads
    }

    /// Returns the number of modeled table writes.
    pub const fn table_writes(&self) -> u64 {
        self.table_writes
    }

    pub(crate) fn record_candidate_scan(&mut self) {
        self.candidate_scans = self.candidate_scans.saturating_add(1);
    }

    pub(crate) fn record_word_step(&mut self) {
        self.word_steps = self.word_steps.saturating_add(1);
        self.xors = self.xors.saturating_add(1);
        self.ands = self.ands.saturating_add(1);
        self.popcounts = self.popcounts.saturating_add(1);
        self.distance_adds = self.distance_adds.saturating_add(1);
        self.table_reads = self.table_reads.saturating_add(3);
    }

    pub(crate) fn record_comparison(&mut self) {
        self.comparisons = self.comparisons.saturating_add(1);
    }

    pub(crate) fn record_insert(&mut self, comparisons: u64, writes: u64) {
        self.comparisons = self.comparisons.saturating_add(comparisons);
        self.table_writes = self.table_writes.saturating_add(writes);
    }
}

/// Upper bounds for a deterministic operation census.
///
/// Fields are public so callers use named construction rather than a long list
/// of same-typed positional arguments.
#[must_use = "an operation budget must be applied to a census"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationBudget {
    /// Maximum candidate regions inspected.
    pub candidate_scans: u64,
    /// Maximum packed words processed.
    pub word_steps: u64,
    /// Maximum explicit XOR operations.
    pub xors: u64,
    /// Maximum explicit AND operations.
    pub ands: u64,
    /// Maximum population-count operations.
    pub popcounts: u64,
    /// Maximum distance-accumulation additions.
    pub distance_adds: u64,
    /// Maximum semantic and ordering comparisons.
    pub comparisons: u64,
    /// Maximum modeled table reads.
    pub table_reads: u64,
    /// Maximum modeled table writes.
    pub table_writes: u64,
}

impl OperationBudget {
    /// Returns whether every observed counter is within its declared bound.
    pub const fn permits(&self, observed: &OperationCensus) -> bool {
        observed.candidate_scans <= self.candidate_scans
            && observed.word_steps <= self.word_steps
            && observed.xors <= self.xors
            && observed.ands <= self.ands
            && observed.popcounts <= self.popcounts
            && observed.distance_adds <= self.distance_adds
            && observed.comparisons <= self.comparisons
            && observed.table_reads <= self.table_reads
            && observed.table_writes <= self.table_writes
    }
}

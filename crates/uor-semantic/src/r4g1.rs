//! Bounded, zero-copy R4G1 header and HEAD identity adaptation.
//!
//! This is deliberately a stage-1 identity view. It validates the fixed
//! container envelope, canonical section-table ordering, section bounds, and
//! the draft-line HEAD identity prefix. It does not yet validate R4G1 section
//! semantics beyond bounded graph records, or BLAKE3 CIDs.

use core::fmt;

use crate::{CodebookId, CompatibilityFormat, CompatibilityManifest};

const HEADER_BYTES: usize = 88;
const SECTION_ENTRY_BYTES: usize = 16;
const HEAD_SECTION_ID: u32 = 1;
const HEAD_BYTES: usize = 224;
const FORMAT_MAJOR: u8 = 0;
const LITTLE_ENDIAN: u8 = 1;
const MIN_ALIGNMENT_LOG2: u8 = 3;
const MAX_ALIGNMENT_LOG2: u8 = 31;
const MANDATORY_FLAG_MASK: u32 = 0x0000_FFFF;

/// Failure while parsing a bounded R4G1 identity envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum R4G1Error {
    /// The fixed 88-byte container header is incomplete.
    HeaderTooShort,
    /// The four-byte container magic is not `R4G1`.
    InvalidMagic,
    /// The container major version is not the supported draft-line version.
    UnsupportedMajor {
        /// Version found in the container.
        found: u8,
    },
    /// The container does not declare little-endian encoding.
    UnsupportedEndianness {
        /// Marker found in the container.
        found: u8,
    },
    /// The section alignment is outside the R4G1 fixed-width range.
    UnsupportedAlignment {
        /// Alignment exponent found in the container.
        found: u8,
    },
    /// Declared total length differs from the supplied byte slice.
    LengthMismatch,
    /// The section table cannot fit in the supplied bytes.
    SectionTableOutOfBounds,
    /// The header declares an unknown mandatory feature bit.
    InvalidFlags,
    /// A section offset does not satisfy the declared alignment.
    UnalignedSection,
    /// A section overlaps the header/table or extends beyond the bytes.
    SectionOutOfBounds,
    /// Section identifiers are not strictly increasing.
    NonCanonicalSectionTable,
    /// More than one HEAD section was declared.
    DuplicateHead,
    /// The mandatory HEAD section is absent.
    MissingHead,
    /// The HEAD section is not exactly the draft-line 224-byte payload.
    HeadLengthMismatch,
    /// A graph section length does not equal its HEAD-declared record count.
    SectionLengthMismatch {
        /// Section whose record length was invalid.
        section: R4G1Section,
    },
    /// A graph section is shorter than its required fixed prefix.
    SectionTooShort {
        /// Section whose fixed prefix is absent.
        section: R4G1Section,
    },
    /// A storage descriptor has an unsupported width or fixed-point shift.
    InvalidStorageDescriptor {
        /// Section containing the descriptor.
        section: R4G1Section,
    },
    /// The optional EXCT section is not a bounded RX1-framed table.
    InvalidExct,
    /// A HEAD signature width declaration is inconsistent.
    InvalidHeadBounds,
    /// An unknown section without the optional-section bit was declared.
    UnknownMandatorySection {
        /// Raw section identifier.
        id: u32,
    },
    /// A required R4G1 section was not declared.
    MissingRequiredSection {
        /// Required section identifier.
        id: u32,
    },
    /// Two non-empty section bodies overlap.
    SectionsOverlap,
    /// A node range exceeds its HEAD-declared bound.
    NodeRangeOutOfBounds {
        /// Node record index.
        node: u32,
        /// Range field that exceeded its target.
        field: R4G1RangeField,
    },
    /// A node depth is outside the HEAD-declared depth count.
    NodeDepthOutOfBounds {
        /// Node record index.
        node: u32,
    },
    /// A node carries an undefined v0 flags byte.
    NodeFlagsInvalid {
        /// Node record index.
        node: u32,
    },
    /// A node range exceeds a declared bounded-work constant.
    NodeBoundExceeded {
        /// Node record index.
        node: u32,
        /// Bounded field that was exceeded.
        field: R4G1RangeField,
    },
    /// An edge endpoint is outside the declared node count.
    EdgeEndpointOutOfBounds {
        /// Edge record index.
        edge: u32,
    },
    /// A canonical EDGE record carries nonzero flags in the v0 format.
    EdgeFlagsInvalid {
        /// Canonical edge record index.
        edge: u32,
    },
    /// Canonical EDGE records are not strictly ordered by wire key.
    EdgeCanonicalOrderViolation {
        /// First record in the violated adjacent pair.
        previous: u32,
        /// Second record in the violated adjacent pair.
        edge: u32,
    },
    /// An unknown mandatory edge-kind discriminant was encountered.
    UnknownEdgeKind {
        /// Canonical edge record index.
        edge: u32,
        /// Raw edge-kind discriminant.
        kind: u8,
    },
    /// A reverse-index entry references an absent canonical edge.
    ReverseIndexOutOfBounds {
        /// Reverse-index position.
        index: u32,
        /// Referenced canonical edge index.
        edge_id: u32,
    },
    /// A canonical edge requiring reverse coverage is absent from the index.
    ReverseIndexMissing {
        /// Canonical edge record index.
        edge: u32,
    },
    /// A node's forward range resolves to an edge targeting another node.
    ReverseRangeTargetMismatch {
        /// Node whose forward range was inspected.
        node: u32,
        /// Reverse-index position.
        index: u32,
        /// Referenced canonical edge index.
        edge_id: u32,
        /// Actual edge destination.
        edge_dst: u32,
    },
    /// A checked fixed-width range calculation overflowed.
    RangeOverflow,
    /// A ROUT bytecode opcode is not defined by the v0 instruction set.
    UnknownRoutingOp {
        /// Byte offset of the unknown opcode.
        offset: u32,
        /// Raw opcode byte.
        opcode: u8,
    },
    /// A ROUT instruction is truncated at the end of the section.
    TruncatedRoutingOp {
        /// Byte offset of the truncated instruction.
        offset: u32,
        /// Raw opcode byte.
        opcode: u8,
    },
    /// A ROUT program has neither HALT nor a terminal LEAF.
    RoutingProgramUnterminated,
    /// A ROUT program exceeds HEAD.D.
    RoutingProgramTooDeep {
        /// Number of decoded instructions.
        ops: u32,
        /// Declared maximum instruction count.
        max: u32,
    },
    /// A ROUT instruction operand exceeds HEAD bounds.
    RoutingOperandOutOfBounds {
        /// Instruction index.
        op_index: u32,
    },
    /// A ROUT forward jump targets outside the decoded program.
    RoutingJumpOutOfBounds {
        /// Jumping instruction index.
        op_index: u32,
        /// Decoded target instruction index.
        target: u32,
    },
    /// A ROUT LEAF shortlist range exceeds the trailing table.
    RoutingShortlistOutOfBounds {
        /// LEAF instruction index.
        op_index: u32,
    },
    /// The caller supplied a signature with a different word width.
    SignatureWidthMismatch {
        /// Supplied signature width.
        provided: u16,
        /// HEAD-declared signature width.
        expected: u16,
    },
    /// A ROUT shortlist is not a whole number of fixed-width node IDs.
    RoutingTableMalformed,
    /// A ROUT shortlist names a node outside the validated NODE section.
    RoutingCandidateOutOfBounds {
        /// Candidate node ID.
        candidate: u32,
    },
    /// The caller-owned route buffer cannot retain another candidate.
    RouteCapacityExceeded,
    /// The EMIT section uses a storage descriptor outside this runtime slice.
    EmissionEncodingUnsupported,
    /// The caller-owned emission buffer cannot retain another entry.
    EmissionCapacityExceeded,
    /// A node index is outside the validated NODE section.
    NodeOutOfBounds {
        /// Requested node index.
        node: u32,
    },
    /// The EMIT root-prior header or entry range is malformed.
    RootPriorMalformed,
}

impl fmt::Display for R4G1Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::HeaderTooShort => "R4G1 header is shorter than 88 bytes",
            Self::InvalidMagic => "R4G1 magic is invalid",
            Self::UnsupportedMajor { .. } => "R4G1 major version is unsupported",
            Self::UnsupportedEndianness { .. } => "R4G1 endianness is unsupported",
            Self::UnsupportedAlignment { .. } => "R4G1 section alignment is unsupported",
            Self::LengthMismatch => "R4G1 declared length differs from supplied bytes",
            Self::SectionTableOutOfBounds => "R4G1 section table is out of bounds",
            Self::InvalidFlags => "R4G1 declares an unknown mandatory feature",
            Self::UnalignedSection => "R4G1 section offset is not aligned",
            Self::SectionOutOfBounds => "R4G1 section range is out of bounds",
            Self::NonCanonicalSectionTable => "R4G1 section table is not canonical",
            Self::DuplicateHead => "R4G1 declares more than one HEAD section",
            Self::MissingHead => "R4G1 mandatory HEAD section is missing",
            Self::HeadLengthMismatch => "R4G1 HEAD is not exactly 224 bytes",
            Self::SectionLengthMismatch { .. } => "R4G1 graph section length is invalid",
            Self::SectionTooShort { .. } => "R4G1 graph section is too short",
            Self::InvalidStorageDescriptor { .. } => "R4G1 storage descriptor is invalid",
            Self::InvalidExct => "R4G1 EXCT RX1 framing is invalid",
            Self::InvalidHeadBounds => "R4G1 HEAD bounds are inconsistent",
            Self::UnknownMandatorySection { .. } => "R4G1 unknown mandatory section",
            Self::MissingRequiredSection { .. } => "R4G1 required section is missing",
            Self::SectionsOverlap => "R4G1 section bodies overlap",
            Self::NodeRangeOutOfBounds { .. } => "R4G1 node range is out of bounds",
            Self::NodeDepthOutOfBounds { .. } => "R4G1 node depth is out of bounds",
            Self::NodeFlagsInvalid { .. } => "R4G1 node flags are invalid",
            Self::NodeBoundExceeded { .. } => "R4G1 node exceeds a HEAD bound",
            Self::EdgeEndpointOutOfBounds { .. } => "R4G1 edge endpoint is out of bounds",
            Self::EdgeFlagsInvalid { .. } => "R4G1 edge flags are invalid",
            Self::EdgeCanonicalOrderViolation { .. } => {
                "R4G1 canonical EDGE records are out of order"
            }
            Self::UnknownEdgeKind { .. } => "R4G1 edge kind is unknown and mandatory",
            Self::ReverseIndexOutOfBounds { .. } => "R4G1 reverse index entry is out of bounds",
            Self::ReverseIndexMissing { .. } => "R4G1 canonical edge is absent from reverse index",
            Self::ReverseRangeTargetMismatch { .. } => {
                "R4G1 node reverse range targets the wrong destination"
            }
            Self::RangeOverflow => "R4G1 range arithmetic overflowed",
            Self::UnknownRoutingOp { .. } => "R4G1 ROUT opcode is unknown",
            Self::TruncatedRoutingOp { .. } => "R4G1 ROUT instruction is truncated",
            Self::RoutingProgramUnterminated => "R4G1 ROUT program is unterminated",
            Self::RoutingProgramTooDeep { .. } => "R4G1 ROUT program exceeds HEAD depth",
            Self::RoutingOperandOutOfBounds { .. } => "R4G1 ROUT operand is out of bounds",
            Self::RoutingJumpOutOfBounds { .. } => "R4G1 ROUT jump is out of bounds",
            Self::RoutingShortlistOutOfBounds { .. } => {
                "R4G1 ROUT shortlist range is out of bounds"
            }
            Self::SignatureWidthMismatch { .. } => "R4G1 signature width does not match HEAD",
            Self::RoutingTableMalformed => "R4G1 ROUT shortlist table is malformed",
            Self::RoutingCandidateOutOfBounds { .. } => "R4G1 ROUT shortlist names an unknown node",
            Self::RouteCapacityExceeded => "R4G1 route candidate capacity was exceeded",
            Self::EmissionEncodingUnsupported => "R4G1 EMIT encoding is unsupported",
            Self::EmissionCapacityExceeded => "R4G1 emission capacity was exceeded",
            Self::NodeOutOfBounds { .. } => "R4G1 node index is out of bounds",
            Self::RootPriorMalformed => "R4G1 EMIT root prior is malformed",
        };
        formatter.write_str(message)
    }
}

/// Known R4G1 section identifiers exposed by the borrowed structure view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum R4G1Section {
    /// Identity and bounded-work HEAD section.
    Head,
    /// Token-code and rolling-state CODE section.
    Code,
    /// Packed semantic-region NODE section.
    Node,
    /// Refinement, overlap, and predictive EDGE section.
    Edge,
    /// Decision-program and shortlist ROUT section.
    Rout,
    /// Prior and residual emission EMIT section.
    Emit,
    /// Optional exact-context EXCT section.
    Exct,
    /// Provenance-root PROV section.
    Prov,
    /// Optional certification CERT section.
    Cert,
    /// Optional patch-chain PTCH section.
    Ptch,
    /// Optional per-section hash SECT section.
    Sect,
    /// Optional route-translation RTNX section.
    Rtnx,
}

/// Node range addressed by R4G1 semantic validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum R4G1RangeField {
    /// Refinement-child edge range.
    Child,
    /// Reverse-index incoming-edge range.
    Forward,
    /// EMIT residual byte range.
    Emission,
    /// ROUT prototype word range.
    Prototype,
    /// ROUT mask word range.
    Mask,
}

/// Typed v0 R4G1 NODE record decoded on demand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct R4G1Node {
    /// First child edge index.
    pub child_start: u32,
    /// Child edge count.
    pub child_len: u16,
    /// First reverse-index entry.
    pub forward_start: u32,
    /// Reverse-index entry count.
    pub forward_len: u16,
    /// EMIT remainder byte offset.
    pub emission_start: u32,
    /// EMIT remainder byte length.
    pub emission_len: u16,
    /// ROUT prototype word offset.
    pub prototype_word_start: u32,
    /// ROUT mask word offset.
    pub mask_word_start: u32,
    /// Calibrated region radius.
    pub radius: u16,
    /// Region refinement depth.
    pub depth: u8,
    /// v0 node flags.
    pub flags: u8,
}

/// Typed v0 R4G1 EDGE record decoded on demand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct R4G1Edge {
    /// Source node index.
    pub src: u32,
    /// Destination node index.
    pub dst: u32,
    /// Fixed-point score representation.
    pub score_q: i32,
    /// Edge-kind discriminant.
    pub kind: u8,
    /// v0 edge flags.
    pub flags: u8,
    /// Reserved or edge-algebra contribution identifier.
    pub reserved: u16,
}

/// One fixed-point token emission decoded from an R4G1 EMIT range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct R4G1Emission {
    /// Token identifier.
    pub token: u32,
    /// Decoded fixed-point score.
    pub score_q: i32,
}

/// Caller-owned bounded candidates returned by ROUT execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct R4G1RouteCandidates<const CAPACITY: usize> {
    entries: [u32; CAPACITY],
    len: usize,
}

impl<const CAPACITY: usize> R4G1RouteCandidates<CAPACITY> {
    /// Creates an empty route result.
    pub const fn new() -> Self {
        Self {
            entries: [0; CAPACITY],
            len: 0,
        }
    }

    /// Returns retained node IDs in ROUT order.
    pub fn as_slice(&self) -> &[u32] {
        &self.entries[..self.len]
    }

    /// Clears retained candidates while preserving caller-owned storage.
    pub fn clear(&mut self) {
        self.len = 0;
    }

    fn push(&mut self, candidate: u32) -> Result<(), R4G1Error> {
        if self.len >= CAPACITY {
            return Err(R4G1Error::RouteCapacityExceeded);
        }
        self.entries[self.len] = candidate;
        self.len += 1;
        Ok(())
    }
}

impl<const CAPACITY: usize> Default for R4G1RouteCandidates<CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

/// Caller-owned bounded EMIT entries returned for one R4G1 node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct R4G1Emissions<const CAPACITY: usize> {
    entries: [R4G1Emission; CAPACITY],
    len: usize,
}

impl<const CAPACITY: usize> R4G1Emissions<CAPACITY> {
    /// Creates an empty emission result.
    pub const fn new() -> Self {
        Self {
            entries: [R4G1Emission {
                token: 0,
                score_q: 0,
            }; CAPACITY],
            len: 0,
        }
    }

    /// Returns decoded entries in their canonical wire order.
    pub fn as_slice(&self) -> &[R4G1Emission] {
        &self.entries[..self.len]
    }

    /// Clears decoded entries while preserving caller-owned storage.
    pub fn clear(&mut self) {
        self.len = 0;
    }

    fn push(&mut self, entry: R4G1Emission) -> Result<(), R4G1Error> {
        if self.len >= CAPACITY {
            return Err(R4G1Error::EmissionCapacityExceeded);
        }
        self.entries[self.len] = entry;
        self.len += 1;
        Ok(())
    }
}

impl<const CAPACITY: usize> Default for R4G1Emissions<CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

/// Caller-owned bounded top-K token scores produced by R4G1 prediction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct R4G1Predictions<const CAPACITY: usize> {
    entries: [R4G1Emission; CAPACITY],
    len: usize,
    truncated: usize,
}

impl<const CAPACITY: usize> R4G1Predictions<CAPACITY> {
    /// Creates an empty prediction result.
    pub const fn new() -> Self {
        Self {
            entries: [R4G1Emission {
                token: 0,
                score_q: i32::MIN,
            }; CAPACITY],
            len: 0,
            truncated: 0,
        }
    }

    /// Returns initialized entries ordered by descending score, then token ID.
    pub fn as_slice(&self) -> &[R4G1Emission] {
        &self.entries[..self.len]
    }

    /// Returns the highest-ranked prediction, when one exists.
    pub fn first(&self) -> Option<R4G1Emission> {
        self.as_slice().first().copied()
    }

    /// Returns entries displaced by the caller's top-K capacity.
    pub const fn truncated(&self) -> usize {
        self.truncated
    }

    /// Clears the result while preserving caller-owned storage.
    pub fn clear(&mut self) {
        self.len = 0;
        self.truncated = 0;
    }

    fn accumulate(&mut self, entry: R4G1Emission) {
        let mut index = 0usize;
        while index < self.len {
            if self.entries[index].token == entry.token {
                if entry.score_q > self.entries[index].score_q {
                    self.entries[index] = entry;
                    self.sort_initialized();
                }
                return;
            }
            index += 1;
        }

        if self.len < CAPACITY {
            self.entries[self.len] = entry;
            self.len += 1;
            self.sort_initialized();
            return;
        }

        self.truncated = self.truncated.saturating_add(1);
        if CAPACITY != 0 && precedes(entry, self.entries[CAPACITY - 1]) {
            self.entries[CAPACITY - 1] = entry;
            self.sort_initialized();
        }
    }

    fn sort_initialized(&mut self) {
        let mut outer = 1usize;
        while outer < self.len {
            let mut inner = outer;
            while inner > 0 && precedes(self.entries[inner], self.entries[inner - 1]) {
                self.entries.swap(inner, inner - 1);
                inner -= 1;
            }
            outer += 1;
        }
    }
}

impl<const CAPACITY: usize> Default for R4G1Predictions<CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

fn precedes(left: R4G1Emission, right: R4G1Emission) -> bool {
    left.score_q > right.score_q || (left.score_q == right.score_q && left.token < right.token)
}

impl R4G1Section {
    /// Returns the fixed wire identifier.
    pub const fn raw(self) -> u32 {
        match self {
            Self::Head => 1,
            Self::Code => 2,
            Self::Node => 3,
            Self::Edge => 4,
            Self::Rout => 5,
            Self::Emit => 6,
            Self::Exct => 7,
            Self::Prov => 8,
            Self::Cert => 9,
            Self::Ptch => 10,
            Self::Sect => 11,
            Self::Rtnx => 12,
        }
    }
}

/// Required section bitset for the R4G1 draft-line inventory.
const REQUIRED_SECTIONS: u16 = 0x017E;
const OPTIONAL_SECTION_BIT: u32 = 0x8000_0000;

impl core::error::Error for R4G1Error {}

/// Fixed identities extracted from an R4G1 header and draft-line HEAD.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct R4G1Identity {
    format_minor: u8,
    section_count: u32,
    artifact_id: CodebookId,
    teacher_id: CodebookId,
    tokenizer_id: CodebookId,
    construction_corpus_id: CodebookId,
    certification_corpus_id: CodebookId,
    compiler_id: CodebookId,
    hf_revision: [u8; 20],
}

impl R4G1Identity {
    /// Parses the fixed R4G1 header and HEAD identity fields without copying
    /// the input artifact or allocating.
    pub fn parse(bytes: &[u8]) -> Result<Self, R4G1Error> {
        if bytes.len() < HEADER_BYTES {
            return Err(R4G1Error::HeaderTooShort);
        }
        if &bytes[0..4] != b"R4G1" {
            return Err(R4G1Error::InvalidMagic);
        }
        if bytes[4] != FORMAT_MAJOR {
            return Err(R4G1Error::UnsupportedMajor { found: bytes[4] });
        }
        if bytes[6] != LITTLE_ENDIAN {
            return Err(R4G1Error::UnsupportedEndianness { found: bytes[6] });
        }
        let alignment_log2 = bytes[7];
        if !(MIN_ALIGNMENT_LOG2..=MAX_ALIGNMENT_LOG2).contains(&alignment_log2) {
            return Err(R4G1Error::UnsupportedAlignment {
                found: alignment_log2,
            });
        }
        let declared_length = read_u64(bytes, 8);
        if declared_length != bytes.len() as u64 {
            return Err(R4G1Error::LengthMismatch);
        }

        let section_count = read_u32(bytes, 16);
        let flags = read_u32(bytes, 20);
        if flags & MANDATORY_FLAG_MASK != 0 {
            return Err(R4G1Error::InvalidFlags);
        }

        let table_end = section_table_end(section_count, bytes.len())?;

        let alignment = 1u32
            .checked_shl(u32::from(alignment_log2))
            .ok_or(R4G1Error::RangeOverflow)?;
        let alignment_mask = alignment - 1;
        let mut cursor = HEADER_BYTES;
        let mut index = 0u32;
        let mut previous_id = None;
        let mut head = None;

        while index < section_count {
            let section_id = read_u32(bytes, cursor);
            let offset = read_u32(bytes, cursor + 8);
            let length = read_u32(bytes, cursor + 12);

            if previous_id.is_some_and(|previous| section_id <= previous) {
                return Err(R4G1Error::NonCanonicalSectionTable);
            }
            previous_id = Some(section_id);

            if offset & alignment_mask != 0 {
                return Err(R4G1Error::UnalignedSection);
            }

            let start = usize::try_from(offset).map_err(|_| R4G1Error::RangeOverflow)?;
            let width = usize::try_from(length).map_err(|_| R4G1Error::RangeOverflow)?;
            if start < table_end {
                return Err(R4G1Error::SectionOutOfBounds);
            }
            let end = start.checked_add(width).ok_or(R4G1Error::RangeOverflow)?;
            if end > bytes.len() {
                return Err(R4G1Error::SectionOutOfBounds);
            }

            if section_id == HEAD_SECTION_ID {
                if head.is_some() {
                    return Err(R4G1Error::DuplicateHead);
                }
                head = Some((start, end));
            }

            cursor = cursor
                .checked_add(SECTION_ENTRY_BYTES)
                .ok_or(R4G1Error::RangeOverflow)?;
            index += 1;
        }

        let (head_start, head_end) = head.ok_or(R4G1Error::MissingHead)?;
        if head_end - head_start != HEAD_BYTES {
            return Err(R4G1Error::HeadLengthMismatch);
        }

        let mut hf_revision = [0u8; 20];
        hf_revision.copy_from_slice(&bytes[head_start + 128..head_start + 148]);
        Ok(Self {
            format_minor: bytes[5],
            section_count,
            artifact_id: read_id(bytes, 24),
            teacher_id: read_id(bytes, head_start),
            tokenizer_id: read_id(bytes, head_start + 32),
            construction_corpus_id: read_id(bytes, head_start + 64),
            certification_corpus_id: read_id(bytes, head_start + 96),
            compiler_id: read_id(bytes, head_start + 148),
            hf_revision,
        })
    }

    /// Returns the R4G1 minor version.
    pub const fn format_minor(self) -> u8 {
        self.format_minor
    }

    /// Returns the number of section-table entries.
    pub const fn section_count(self) -> u32 {
        self.section_count
    }

    /// Returns the R4G1 artifact CID as the semantic artifact identity.
    pub const fn artifact_id(self) -> CodebookId {
        self.artifact_id
    }

    /// Returns the teacher model CID as the semantic source identity.
    pub const fn teacher_id(self) -> CodebookId {
        self.teacher_id
    }

    /// Returns the tokenizer identity CID.
    pub const fn tokenizer_id(self) -> CodebookId {
        self.tokenizer_id
    }

    /// Returns the construction corpus root identity.
    pub const fn construction_corpus_id(self) -> CodebookId {
        self.construction_corpus_id
    }

    /// Returns the held-out/certification corpus root identity.
    pub const fn certification_corpus_id(self) -> CodebookId {
        self.certification_corpus_id
    }

    /// Returns the compiler identity CID.
    pub const fn compiler_id(self) -> CodebookId {
        self.compiler_id
    }

    /// Returns the opaque 20-byte pinned Hugging Face revision field.
    pub const fn hf_revision(self) -> [u8; 20] {
        self.hf_revision
    }

    /// Adapts the parsed identities to the semantic compatibility manifest.
    pub const fn to_manifest(self) -> CompatibilityManifest {
        CompatibilityManifest::new(
            CompatibilityFormat::R4G1,
            self.artifact_id,
            self.teacher_id,
            self.tokenizer_id,
            None,
            None,
        )
    }
}

/// Borrowed stage-1 structural view over a complete R4G1 section carrier.
///
/// Construction performs no allocation and requires HEAD, CODE, NODE, EDGE,
/// ROUT, EMIT, and PROV. Section payloads remain in the caller-owned input and
/// are returned as borrowed slices on demand. This view does not yet validate
/// the internal graph records, ROUT bytecode, section checksums, or
/// certificate contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct R4G1Structure<'a> {
    bytes: &'a [u8],
    identity: R4G1Identity,
}

impl<'a> R4G1Structure<'a> {
    /// Parses and structurally validates a complete R4G1 section carrier.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, R4G1Error> {
        let identity = R4G1Identity::parse(bytes)?;
        validate_structure(bytes, identity.section_count())?;
        Ok(Self { bytes, identity })
    }

    /// Returns the validated fixed identities.
    pub const fn identity(&self) -> R4G1Identity {
        self.identity
    }

    /// Returns one validated section body without copying it.
    pub fn section(&self, section: R4G1Section) -> Option<&'a [u8]> {
        let mut cursor = HEADER_BYTES;
        let mut index = 0u32;
        while index < self.identity.section_count {
            let section_id = read_u32(self.bytes, cursor);
            let offset = read_u32(self.bytes, cursor + 8);
            let length = read_u32(self.bytes, cursor + 12);
            if section_id == section.raw() {
                let start = usize::try_from(offset).ok()?;
                let width = usize::try_from(length).ok()?;
                let end = start.checked_add(width)?;
                return self.bytes.get(start..end);
            }
            cursor += SECTION_ENTRY_BYTES;
            index += 1;
        }
        None
    }

    /// Returns the complete borrowed R4G1 byte slice.
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

/// Borrowed R4G1 graph view after HEAD, NODE, and EDGE semantic validation.
///
/// The view validates exact fixed-record section lengths, HEAD signature and
/// bounded-work declarations, every NODE range, ROUT word window, node depth,
/// and EDGE endpoints. It borrows all section bytes and performs no heap
/// allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct R4G1Graph<'a> {
    structure: R4G1Structure<'a>,
    node_count: u32,
    edge_count: u32,
    signature_words: u16,
    depth_count: u8,
    max_program_steps: u32,
}

impl<'a> R4G1Graph<'a> {
    /// Parses a complete R4G1 structure and validates its graph records.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, R4G1Error> {
        let structure = R4G1Structure::parse(bytes)?;
        let head = structure
            .section(R4G1Section::Head)
            .ok_or(R4G1Error::MissingHead)?;
        let signature_words = read_u16(head, 184);
        let signature_bytes = read_u16(head, 212);
        let storage_bytes = u32::from(signature_words) << 3;
        if signature_words == 0
            || u32::from(signature_bytes) > storage_bytes
            || u32::from(signature_bytes) <= storage_bytes.saturating_sub(8)
        {
            return Err(R4G1Error::InvalidHeadBounds);
        }

        let node_count = read_u32(head, 196);
        let edge_count = read_u32(head, 200);
        let depth_count = head[204];
        if node_count != 0 && depth_count == 0 {
            return Err(R4G1Error::InvalidHeadBounds);
        }

        let node_bytes =
            structure
                .section(R4G1Section::Node)
                .ok_or(R4G1Error::SectionTooShort {
                    section: R4G1Section::Node,
                })?;
        if node_bytes.len()
            != expected_record_bytes(node_count, 30, node_bytes.len(), R4G1Section::Node)?
        {
            return Err(R4G1Error::SectionLengthMismatch {
                section: R4G1Section::Node,
            });
        }

        let edge_bytes =
            structure
                .section(R4G1Section::Edge)
                .ok_or(R4G1Error::SectionTooShort {
                    section: R4G1Section::Edge,
                })?;
        if edge_bytes.len()
            != expected_record_bytes(edge_count, 20, edge_bytes.len(), R4G1Section::Edge)?
        {
            return Err(R4G1Error::SectionLengthMismatch {
                section: R4G1Section::Edge,
            });
        }

        let rout = structure
            .section(R4G1Section::Rout)
            .ok_or(R4G1Error::SectionTooShort {
                section: R4G1Section::Rout,
            })?;
        if rout.len() & 7 != 0 {
            return Err(R4G1Error::SectionLengthMismatch {
                section: R4G1Section::Rout,
            });
        }
        validate_rout(rout, read_u32(head, 192), signature_words)?;
        let rout_words = (rout.len() >> 3) as u64;

        let emit = structure
            .section(R4G1Section::Emit)
            .ok_or(R4G1Error::SectionTooShort {
                section: R4G1Section::Emit,
            })?;
        validate_storage_descriptor(emit, R4G1Section::Emit)?;
        if let Some(exct) = structure.section(R4G1Section::Exct) {
            validate_storage_descriptor(exct, R4G1Section::Exct)?;
            validate_exct(exct)?;
        }
        let emit_remainder = emit.len() - 4;
        let max_frontier = read_u16(head, 180);
        let max_emissions = read_u32(head, 188);
        let bounds = NodeBounds {
            edge_count,
            emit_remainder,
            rout_words,
            signature_words,
            depth_count,
            max_frontier,
            max_emissions,
        };

        let mut node_cursor = 0usize;
        let mut node = 0u32;
        while node < node_count {
            let record = decode_node(node_bytes, node_cursor);
            validate_node(node, record, bounds)?;
            node_cursor += 30;
            node += 1;
        }

        validate_edges(edge_bytes, node_bytes, node_count, edge_count)?;

        Ok(Self {
            structure,
            node_count,
            edge_count,
            signature_words,
            depth_count,
            max_program_steps: read_u32(head, 192),
        })
    }

    /// Returns the validated node count.
    pub const fn node_count(self) -> u32 {
        self.node_count
    }

    /// Returns the validated canonical edge count.
    pub const fn edge_count(self) -> u32 {
        self.edge_count
    }

    /// Returns the validated signature width in u64 words.
    pub const fn signature_words(self) -> u16 {
        self.signature_words
    }

    /// Returns the validated depth count.
    pub const fn depth_count(self) -> u8 {
        self.depth_count
    }

    /// Returns the HEAD-declared maximum ROUT instruction steps.
    pub const fn max_program_steps(self) -> u32 {
        self.max_program_steps
    }

    /// Returns the underlying identity view.
    pub const fn identity(self) -> R4G1Identity {
        self.structure.identity()
    }

    /// Decodes one validated NODE record without copying the section.
    pub fn node(&self, index: u32) -> Option<R4G1Node> {
        if index >= self.node_count {
            return None;
        }
        let bytes = self.structure.section(R4G1Section::Node)?;
        let offset = record_offset(index, 30)?;
        Some(decode_node(bytes, offset))
    }

    /// Decodes one validated canonical EDGE record without copying the section.
    pub fn edge(&self, index: u32) -> Option<R4G1Edge> {
        if index >= self.edge_count {
            return None;
        }
        let bytes = self.structure.section(R4G1Section::Edge)?;
        let offset = record_offset(index, 16)?;
        Some(decode_edge(bytes, offset))
    }

    /// Returns one validated reverse-index edge ID without copying the section.
    pub fn reverse_edge_id(&self, index: u32) -> Option<u32> {
        if index >= self.edge_count {
            return None;
        }
        let bytes = self.structure.section(R4G1Section::Edge)?;
        let canonical_bytes = record_offset(self.edge_count, 16)?;
        let offset = canonical_bytes.checked_add(record_offset(index, 4)?)?;
        Some(read_u32(bytes, offset))
    }

    /// Returns one borrowed validated section body.
    pub fn section(&self, section: R4G1Section) -> Option<&'a [u8]> {
        self.structure.section(section)
    }

    /// Executes ROUT v0 and retains candidates whose prototype accepts the signature.
    ///
    /// The caller supplies both the signature storage and the bounded output
    /// buffer. ROUT execution, shortlist decoding, and masked-Hamming checks
    /// perform no allocation and inspect at most the validated program and
    /// shortlist ranges.
    pub fn route<const WORDS: usize, const CAPACITY: usize>(
        &self,
        signature: &[u64; WORDS],
        candidates: &mut R4G1RouteCandidates<CAPACITY>,
    ) -> Result<(), R4G1Error> {
        candidates.clear();
        let provided = if WORDS > usize::from(u16::MAX) {
            u16::MAX
        } else {
            WORDS as u16
        };
        if provided != self.signature_words {
            return Err(R4G1Error::SignatureWidthMismatch {
                provided,
                expected: self.signature_words,
            });
        }
        let rout = self
            .structure
            .section(R4G1Section::Rout)
            .ok_or(R4G1Error::SectionTooShort {
                section: R4G1Section::Rout,
            })?;
        let table_start = route_table_start(rout)?;
        let table = rout
            .get(table_start..)
            .ok_or(R4G1Error::RoutingTableMalformed)?;
        let mut pc = 0usize;
        let mut steps = 0u32;
        while pc < table_start {
            if steps >= self.max_program_steps {
                return Err(R4G1Error::RoutingProgramTooDeep {
                    ops: steps,
                    max: self.max_program_steps,
                });
            }
            steps = steps.saturating_add(1);
            let opcode = rout[pc];
            match opcode {
                ROUT_OP_HALT => return self.route_table(table, signature, candidates),
                ROUT_OP_TEST_POPCOUNT_LE => {
                    let word = usize::from(rout[pc + 1]);
                    let mask = read_u64(rout, pc + 2);
                    let threshold = read_u16(rout, pc + 10);
                    let popcount = (signature[word] & mask).count_ones() as u16;
                    pc += 12;
                    if popcount > threshold && pc < table_start {
                        pc = pc
                            .checked_add(route_op_size(rout[pc])?)
                            .ok_or(R4G1Error::RoutingProgramUnterminated)?;
                    }
                }
                ROUT_OP_JMP_FWD => {
                    let delta = usize::from(read_u16(rout, pc + 1));
                    pc += 3;
                    let mut skipped = 0usize;
                    while skipped < delta {
                        if pc >= table_start {
                            return Err(R4G1Error::RoutingProgramUnterminated);
                        }
                        pc = pc
                            .checked_add(route_op_size(rout[pc])?)
                            .ok_or(R4G1Error::RoutingProgramUnterminated)?;
                        skipped += 1;
                    }
                }
                ROUT_OP_LEAF => {
                    let start = usize::try_from(read_u32(rout, pc + 1))
                        .map_err(|_| R4G1Error::RangeOverflow)?;
                    let length = usize::from(read_u16(rout, pc + 5));
                    let end = start
                        .checked_add(length)
                        .ok_or(R4G1Error::RoutingTableMalformed)?;
                    let shortlist = table
                        .get(start..end)
                        .ok_or(R4G1Error::RoutingTableMalformed)?;
                    return self.route_table(shortlist, signature, candidates);
                }
                _ => {
                    return Err(R4G1Error::UnknownRoutingOp {
                        offset: u32::try_from(pc).unwrap_or(u32::MAX),
                        opcode,
                    });
                }
            }
        }
        self.route_table(table, signature, candidates)
    }

    /// Reads one node's fixed-point EMIT entries into caller-owned storage.
    pub fn node_emissions<const CAPACITY: usize>(
        &self,
        node_index: u32,
        emissions: &mut R4G1Emissions<CAPACITY>,
    ) -> Result<(), R4G1Error> {
        emissions.clear();
        let node = self
            .node(node_index)
            .ok_or(R4G1Error::NodeOutOfBounds { node: node_index })?;
        let emit = self
            .structure
            .section(R4G1Section::Emit)
            .ok_or(R4G1Error::SectionTooShort {
                section: R4G1Section::Emit,
            })?;
        validate_emission_encoding(emit)?;
        let bytes = self.node_emission_bytes(emit, node_index, node)?;
        let mut offset = 0usize;
        while offset < bytes.len() {
            let token = read_u32(bytes, offset);
            let raw_score = read_i32(bytes, offset + 4);
            emissions.push(R4G1Emission {
                token,
                score_q: decode_score(raw_score, emit[1], read_i16(emit, 2))?,
            })?;
            offset += 8;
        }
        Ok(())
    }

    /// Reads the section-level root-prior entries into caller-owned storage.
    pub fn root_emissions<const CAPACITY: usize>(
        &self,
        emissions: &mut R4G1Emissions<CAPACITY>,
    ) -> Result<(), R4G1Error> {
        emissions.clear();
        let bytes = self.root_emission_bytes()?;
        let mut offset = 0usize;
        while offset < bytes.len() {
            emissions.push(R4G1Emission {
                token: read_u32(bytes, offset),
                score_q: read_i32(bytes, offset + 4),
            })?;
            offset += 8;
        }
        Ok(())
    }

    /// Predicts deterministic top-K tokens from root-prior and routed EMIT data.
    ///
    /// The caller owns both bounded buffers. Each token is retained once, using
    /// the highest available fixed-point score from the root prior or a routed
    /// node emission; ties are ordered by ascending token ID. This merge policy
    /// is deliberately conservative for this bridge because exported node
    /// entries are absolute scores, not target-graded residual contributions.
    pub fn predict_top_k<const WORDS: usize, const ROUTE_CAPACITY: usize, const CAPACITY: usize>(
        &self,
        signature: &[u64; WORDS],
        candidates: &mut R4G1RouteCandidates<ROUTE_CAPACITY>,
        predictions: &mut R4G1Predictions<CAPACITY>,
    ) -> Result<(), R4G1Error> {
        predictions.clear();
        self.route(signature, candidates)?;

        let root = self.root_emission_bytes()?;
        accumulate_emission_bytes(root, predictions);

        let emit = self
            .structure
            .section(R4G1Section::Emit)
            .ok_or(R4G1Error::SectionTooShort {
                section: R4G1Section::Emit,
            })?;
        for &candidate in candidates.as_slice() {
            let node = self
                .node(candidate)
                .ok_or(R4G1Error::RoutingCandidateOutOfBounds { candidate })?;
            let bytes = self.node_emission_bytes(emit, candidate, node)?;
            accumulate_emission_bytes(bytes, predictions);
        }
        Ok(())
    }

    fn route_table<const WORDS: usize, const CAPACITY: usize>(
        &self,
        table: &[u8],
        signature: &[u64; WORDS],
        candidates: &mut R4G1RouteCandidates<CAPACITY>,
    ) -> Result<(), R4G1Error> {
        if table.len() & 3 != 0 {
            return Err(R4G1Error::RoutingTableMalformed);
        }
        let mut offset = 0usize;
        while offset < table.len() {
            let candidate = read_u32(table, offset);
            if candidate >= self.node_count {
                return Err(R4G1Error::RoutingCandidateOutOfBounds { candidate });
            }
            let node = self
                .node(candidate)
                .ok_or(R4G1Error::RoutingCandidateOutOfBounds { candidate })?;
            if self.node_accepts(signature, node)? {
                candidates.push(candidate)?;
            }
            offset += 4;
        }
        Ok(())
    }

    fn root_emission_bytes(&self) -> Result<&'a [u8], R4G1Error> {
        let emit = self
            .structure
            .section(R4G1Section::Emit)
            .ok_or(R4G1Error::SectionTooShort {
                section: R4G1Section::Emit,
            })?;
        validate_emission_encoding(emit)?;
        if emit.len() < 20 {
            return Err(R4G1Error::RootPriorMalformed);
        }
        let count = usize::try_from(read_u32(emit, 4)).map_err(|_| R4G1Error::RangeOverflow)?;
        let byte_len = count.checked_shl(3).ok_or(R4G1Error::RangeOverflow)?;
        let end = 20usize
            .checked_add(byte_len)
            .ok_or(R4G1Error::RootPriorMalformed)?;
        emit.get(20..end).ok_or(R4G1Error::RootPriorMalformed)
    }

    fn node_emission_bytes(
        &self,
        emit: &'a [u8],
        node_index: u32,
        node: R4G1Node,
    ) -> Result<&'a [u8], R4G1Error> {
        let start = 4usize
            .checked_add(
                usize::try_from(node.emission_start).map_err(|_| R4G1Error::RangeOverflow)?,
            )
            .ok_or(R4G1Error::RangeOverflow)?;
        let byte_len = usize::from(node.emission_len)
            .checked_shl(3)
            .ok_or(R4G1Error::RangeOverflow)?;
        let end = start
            .checked_add(byte_len)
            .ok_or(R4G1Error::RangeOverflow)?;
        emit.get(start..end).ok_or(R4G1Error::NodeRangeOutOfBounds {
            node: node_index,
            field: R4G1RangeField::Emission,
        })
    }

    fn node_accepts<const WORDS: usize>(
        &self,
        signature: &[u64; WORDS],
        node: R4G1Node,
    ) -> Result<bool, R4G1Error> {
        let rout = self
            .structure
            .section(R4G1Section::Rout)
            .ok_or(R4G1Error::SectionTooShort {
                section: R4G1Section::Rout,
            })?;
        let prototype = usize::try_from(node.prototype_word_start)
            .map_err(|_| R4G1Error::RangeOverflow)?
            .checked_shl(3)
            .ok_or(R4G1Error::RangeOverflow)?;
        let mask = usize::try_from(node.mask_word_start)
            .map_err(|_| R4G1Error::RangeOverflow)?
            .checked_shl(3)
            .ok_or(R4G1Error::RangeOverflow)?;
        let mut distance = 0u16;
        let mut word = 0usize;
        while word < WORDS {
            let prototype_word = read_u64(rout, prototype_offset(prototype, word)?);
            let mask_word = read_u64(rout, prototype_offset(mask, word)?);
            distance = distance.saturating_add(
                ((signature[word] ^ prototype_word) & mask_word).count_ones() as u16,
            );
            word += 1;
        }
        Ok(distance <= node.radius)
    }
}

const ROUT_OP_HALT: u8 = 0x00;
const ROUT_OP_TEST_POPCOUNT_LE: u8 = 0x01;
const ROUT_OP_JMP_FWD: u8 = 0x02;
const ROUT_OP_LEAF: u8 = 0x03;

fn rout_op_size(opcode: u8) -> Option<usize> {
    Some(match opcode {
        ROUT_OP_HALT => 1,
        ROUT_OP_TEST_POPCOUNT_LE => 12,
        ROUT_OP_JMP_FWD => 3,
        ROUT_OP_LEAF => 7,
        _ => return None,
    })
}

fn validate_rout(
    bytes: &[u8],
    max_program_steps: u32,
    signature_words: u16,
) -> Result<(), R4G1Error> {
    let mut cursor = 0usize;
    let mut op_count = 0u32;
    let mut last_opcode = None;
    let mut halted = false;
    while cursor < bytes.len() {
        let opcode = bytes[cursor];
        let size = rout_op_size(opcode).ok_or(R4G1Error::UnknownRoutingOp {
            offset: u32::try_from(cursor).unwrap_or(u32::MAX),
            opcode,
        })?;
        if cursor.saturating_add(size) > bytes.len() {
            return Err(R4G1Error::TruncatedRoutingOp {
                offset: u32::try_from(cursor).unwrap_or(u32::MAX),
                opcode,
            });
        }
        op_count = op_count.checked_add(1).ok_or(R4G1Error::RangeOverflow)?;
        last_opcode = Some(opcode);
        cursor += size;
        if opcode == ROUT_OP_HALT {
            halted = true;
            break;
        }
    }
    if !halted && last_opcode != Some(ROUT_OP_LEAF) {
        return Err(R4G1Error::RoutingProgramUnterminated);
    }
    if op_count > max_program_steps {
        return Err(R4G1Error::RoutingProgramTooDeep {
            ops: op_count,
            max: max_program_steps,
        });
    }
    let table = &bytes[cursor..];

    cursor = 0;
    let mut index = 0u32;
    while index < op_count {
        let opcode = bytes[cursor];
        match opcode {
            ROUT_OP_TEST_POPCOUNT_LE => {
                let word = bytes[cursor + 1];
                let threshold = read_u16(bytes, cursor + 10);
                if u16::from(word) >= signature_words || threshold > 64 {
                    return Err(R4G1Error::RoutingOperandOutOfBounds { op_index: index });
                }
            }
            ROUT_OP_JMP_FWD => {
                let delta = read_u16(bytes, cursor + 1);
                let target = u64::from(index) + 1 + u64::from(delta);
                if target >= u64::from(op_count) {
                    return Err(R4G1Error::RoutingJumpOutOfBounds {
                        op_index: index,
                        target: u32::try_from(target).unwrap_or(u32::MAX),
                    });
                }
            }
            ROUT_OP_LEAF => {
                let start = read_u32(bytes, cursor + 1);
                let len = read_u16(bytes, cursor + 5);
                if (table.is_empty() && len != 0)
                    || (!table.is_empty()
                        && u64::from(start).saturating_add(u64::from(len)) > table.len() as u64)
                {
                    return Err(R4G1Error::RoutingShortlistOutOfBounds { op_index: index });
                }
            }
            ROUT_OP_HALT => {}
            _ => {
                return Err(R4G1Error::UnknownRoutingOp {
                    offset: u32::try_from(cursor).unwrap_or(u32::MAX),
                    opcode,
                });
            }
        }
        cursor += rout_op_size(opcode).ok_or(R4G1Error::UnknownRoutingOp {
            offset: u32::try_from(cursor).unwrap_or(u32::MAX),
            opcode,
        })?;
        index += 1;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct NodeBounds {
    edge_count: u32,
    emit_remainder: usize,
    rout_words: u64,
    signature_words: u16,
    depth_count: u8,
    max_frontier: u16,
    max_emissions: u32,
}

fn validate_node(index: u32, node: R4G1Node, bounds: NodeBounds) -> Result<(), R4G1Error> {
    if range_end(node.child_start, node.child_len) > u64::from(bounds.edge_count) {
        return Err(R4G1Error::NodeRangeOutOfBounds {
            node: index,
            field: R4G1RangeField::Child,
        });
    }
    if range_end(node.forward_start, node.forward_len) > u64::from(bounds.edge_count) {
        return Err(R4G1Error::NodeRangeOutOfBounds {
            node: index,
            field: R4G1RangeField::Forward,
        });
    }
    if range_end(node.emission_start, node.emission_len) > bounds.emit_remainder as u64 {
        return Err(R4G1Error::NodeRangeOutOfBounds {
            node: index,
            field: R4G1RangeField::Emission,
        });
    }
    if u64::from(node.prototype_word_start) + u64::from(bounds.signature_words) > bounds.rout_words
    {
        return Err(R4G1Error::NodeRangeOutOfBounds {
            node: index,
            field: R4G1RangeField::Prototype,
        });
    }
    if u64::from(node.mask_word_start) + u64::from(bounds.signature_words) > bounds.rout_words {
        return Err(R4G1Error::NodeRangeOutOfBounds {
            node: index,
            field: R4G1RangeField::Mask,
        });
    }
    if node.child_len > bounds.max_frontier {
        return Err(R4G1Error::NodeBoundExceeded {
            node: index,
            field: R4G1RangeField::Child,
        });
    }
    if u32::from(node.emission_len) > bounds.max_emissions {
        return Err(R4G1Error::NodeBoundExceeded {
            node: index,
            field: R4G1RangeField::Emission,
        });
    }
    if node.depth >= bounds.depth_count {
        return Err(R4G1Error::NodeDepthOutOfBounds { node: index });
    }
    if node.flags != 0 {
        return Err(R4G1Error::NodeFlagsInvalid { node: index });
    }
    Ok(())
}

fn expected_record_bytes(
    count: u32,
    width: usize,
    available: usize,
    section: R4G1Section,
) -> Result<usize, R4G1Error> {
    let mut bytes = 0usize;
    let mut index = 0u32;
    while index < count {
        if bytes > available.saturating_sub(width) {
            return Err(R4G1Error::SectionLengthMismatch { section });
        }
        bytes = bytes.checked_add(width).ok_or(R4G1Error::RangeOverflow)?;
        index += 1;
    }
    Ok(bytes)
}

fn record_offset(index: u32, width: usize) -> Option<usize> {
    let mut offset = 0usize;
    let mut current = 0u32;
    while current < index {
        offset = offset.checked_add(width)?;
        current += 1;
    }
    Some(offset)
}

fn range_end(start: u32, length: u16) -> u64 {
    u64::from(start) + u64::from(length)
}

fn decode_node(bytes: &[u8], at: usize) -> R4G1Node {
    R4G1Node {
        child_start: read_u32(bytes, at),
        child_len: read_u16(bytes, at + 4),
        forward_start: read_u32(bytes, at + 6),
        forward_len: read_u16(bytes, at + 10),
        emission_start: read_u32(bytes, at + 12),
        emission_len: read_u16(bytes, at + 16),
        prototype_word_start: read_u32(bytes, at + 18),
        mask_word_start: read_u32(bytes, at + 22),
        radius: read_u16(bytes, at + 26),
        depth: bytes[at + 28],
        flags: bytes[at + 29],
    }
}

fn decode_edge(bytes: &[u8], at: usize) -> R4G1Edge {
    R4G1Edge {
        src: read_u32(bytes, at),
        dst: read_u32(bytes, at + 4),
        score_q: read_i32(bytes, at + 8),
        kind: bytes[at + 12],
        flags: bytes[at + 13],
        reserved: read_u16(bytes, at + 14),
    }
}

fn validate_edges(
    bytes: &[u8],
    node_bytes: &[u8],
    node_count: u32,
    edge_count: u32,
) -> Result<(), R4G1Error> {
    let mut previous_key = None;
    let mut edge = 0u32;
    while edge < edge_count {
        let offset = record_offset(edge, 16).ok_or(R4G1Error::RangeOverflow)?;
        let record = decode_edge(bytes, offset);
        if record.src >= node_count || record.dst >= node_count {
            return Err(R4G1Error::EdgeEndpointOutOfBounds { edge });
        }
        if record.flags != 0 {
            return Err(R4G1Error::EdgeFlagsInvalid { edge });
        }
        if record.kind > 8 && record.kind & 0x80 == 0 {
            return Err(R4G1Error::UnknownEdgeKind {
                edge,
                kind: record.kind,
            });
        }
        let key = (record.src, record.kind, record.dst, record.reserved);
        if previous_key.is_some_and(|previous| previous >= key) {
            return Err(R4G1Error::EdgeCanonicalOrderViolation {
                previous: edge.saturating_sub(1),
                edge,
            });
        }
        previous_key = Some(key);
        edge += 1;
    }

    let canonical_bytes = record_offset(edge_count, 16).ok_or(R4G1Error::RangeOverflow)?;
    let mut reverse_position = 0u32;
    while reverse_position < edge_count {
        let reverse_offset = canonical_bytes
            .checked_add(record_offset(reverse_position, 4).ok_or(R4G1Error::RangeOverflow)?)
            .ok_or(R4G1Error::RangeOverflow)?;
        let edge_id = read_u32(bytes, reverse_offset);
        if edge_id >= edge_count {
            return Err(R4G1Error::ReverseIndexOutOfBounds {
                index: reverse_position,
                edge_id,
            });
        }
        reverse_position += 1;
    }

    let mut expected_edge = 0u32;
    while expected_edge < edge_count {
        let mut found = false;
        let mut position = 0u32;
        while position < edge_count {
            let offset = canonical_bytes
                .checked_add(record_offset(position, 4).ok_or(R4G1Error::RangeOverflow)?)
                .ok_or(R4G1Error::RangeOverflow)?;
            if read_u32(bytes, offset) == expected_edge {
                found = true;
                break;
            }
            position += 1;
        }
        if !found {
            return Err(R4G1Error::ReverseIndexMissing {
                edge: expected_edge,
            });
        }
        expected_edge += 1;
    }

    let mut node = 0u32;
    while node < node_count {
        let node_offset = record_offset(node, 30).ok_or(R4G1Error::RangeOverflow)?;
        let record = decode_node(node_bytes, node_offset);
        let mut position = record.forward_start;
        let end = range_end(record.forward_start, record.forward_len);
        while u64::from(position) < end {
            let reverse_offset = canonical_bytes
                .checked_add(record_offset(position, 4).ok_or(R4G1Error::RangeOverflow)?)
                .ok_or(R4G1Error::RangeOverflow)?;
            let edge_id = read_u32(bytes, reverse_offset);
            let edge_offset = record_offset(edge_id, 16).ok_or(R4G1Error::RangeOverflow)?;
            let edge = decode_edge(bytes, edge_offset);
            if edge.dst != node {
                return Err(R4G1Error::ReverseRangeTargetMismatch {
                    node,
                    index: position,
                    edge_id,
                    edge_dst: edge.dst,
                });
            }
            position = position.saturating_add(1);
        }
        node += 1;
    }
    Ok(())
}

fn validate_storage_descriptor(bytes: &[u8], section: R4G1Section) -> Result<(), R4G1Error> {
    if bytes.len() < 4 {
        return Err(R4G1Error::InvalidStorageDescriptor { section });
    }
    let shift = i8::from_le_bytes([bytes[1]]);
    if bytes[0] > 2 || !(-31..=31).contains(&shift) {
        return Err(R4G1Error::InvalidStorageDescriptor { section });
    }
    Ok(())
}

fn validate_exct(bytes: &[u8]) -> Result<(), R4G1Error> {
    if bytes.len() < 12 || bytes.get(4..8) != Some(b"RX1\0") || bytes[8] != 5 {
        return Err(R4G1Error::InvalidExct);
    }
    if bytes[9] != 0 || bytes[10] != 0 || bytes[11] != 0 {
        return Err(R4G1Error::InvalidExct);
    }
    let mut offset = 12usize;
    let mut level = 0u8;
    while level < 5 {
        let key_count = exct_u32(bytes, &mut offset).ok_or(R4G1Error::InvalidExct)?;
        let mut key_index = 0u32;
        while key_index < key_count {
            let key_len = bytes.get(offset).copied().ok_or(R4G1Error::InvalidExct)?;
            offset = offset.checked_add(1).ok_or(R4G1Error::RangeOverflow)?;
            if key_len != level {
                return Err(R4G1Error::InvalidExct);
            }
            exct_advance(bytes, &mut offset, usize::from(key_len))?;
            let _total = exct_u32(bytes, &mut offset).ok_or(R4G1Error::InvalidExct)?;
            let entry_count = exct_u32(bytes, &mut offset).ok_or(R4G1Error::InvalidExct)?;
            let mut entry = 0u32;
            while entry < entry_count {
                exct_advance(bytes, &mut offset, 8)?;
                entry += 1;
            }
            key_index += 1;
        }
        level += 1;
    }
    if offset != bytes.len() {
        return Err(R4G1Error::InvalidExct);
    }
    Ok(())
}

fn exct_u32(bytes: &[u8], offset: &mut usize) -> Option<u32> {
    let start = core::mem::replace(offset, 0);
    let end = start.checked_add(4)?;
    let value = bytes.get(start..end)?;
    let _ = core::mem::replace(offset, end);
    Some(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn exct_advance(bytes: &[u8], offset: &mut usize, width: usize) -> Result<(), R4G1Error> {
    let start = core::mem::replace(offset, 0);
    let end = start.checked_add(width).ok_or(R4G1Error::RangeOverflow)?;
    if end > bytes.len() {
        return Err(R4G1Error::InvalidExct);
    }
    let _ = core::mem::replace(offset, end);
    Ok(())
}

fn validate_structure(bytes: &[u8], section_count: u32) -> Result<(), R4G1Error> {
    let table_end = section_table_end(section_count, bytes.len())?;
    let mut required = 0u16;
    let mut outer_cursor = HEADER_BYTES;
    let mut outer = 0u32;

    while outer < section_count {
        let outer_id = read_u32(bytes, outer_cursor);
        let outer_offset = read_u32(bytes, outer_cursor + 8);
        let outer_length = read_u32(bytes, outer_cursor + 12);
        if !is_known_section(outer_id) && outer_id & OPTIONAL_SECTION_BIT == 0 {
            return Err(R4G1Error::UnknownMandatorySection { id: outer_id });
        }
        if is_required_section(outer_id) {
            required |= 1u16 << outer_id;
        }

        let outer_start = u64::from(outer_offset);
        let outer_end = outer_start + u64::from(outer_length);
        if outer_length != 0 && outer_start < table_end as u64 {
            return Err(R4G1Error::SectionsOverlap);
        }

        let mut inner_cursor = outer_cursor + SECTION_ENTRY_BYTES;
        let mut inner = outer + 1;
        while inner < section_count {
            let inner_offset = u64::from(read_u32(bytes, inner_cursor + 8));
            let inner_length = read_u32(bytes, inner_cursor + 12);
            let inner_end = inner_offset + u64::from(inner_length);
            if outer_length != 0
                && inner_length != 0
                && outer_start < inner_end
                && inner_offset < outer_end
            {
                return Err(R4G1Error::SectionsOverlap);
            }
            inner_cursor += SECTION_ENTRY_BYTES;
            inner += 1;
        }

        outer_cursor += SECTION_ENTRY_BYTES;
        outer += 1;
    }

    if required != REQUIRED_SECTIONS {
        let missing = first_missing_required(required);
        return Err(R4G1Error::MissingRequiredSection { id: missing });
    }
    Ok(())
}

fn is_known_section(id: u32) -> bool {
    (1..=12).contains(&id)
}

fn is_required_section(id: u32) -> bool {
    matches!(id, 1..=6 | 8)
}

fn first_missing_required(present: u16) -> u32 {
    let mut id = 1u32;
    while id <= 8 {
        if is_required_section(id) && present & (1u16 << id) == 0 {
            return id;
        }
        id += 1;
    }
    0
}

fn section_table_end(section_count: u32, available: usize) -> Result<usize, R4G1Error> {
    let mut end = HEADER_BYTES;
    let mut index = 0u32;
    while index < section_count {
        if end > available.saturating_sub(SECTION_ENTRY_BYTES) {
            return Err(R4G1Error::SectionTableOutOfBounds);
        }
        end = end
            .checked_add(SECTION_ENTRY_BYTES)
            .ok_or(R4G1Error::RangeOverflow)?;
        index += 1;
    }
    Ok(end)
}

fn route_table_start(bytes: &[u8]) -> Result<usize, R4G1Error> {
    let mut offset = 0usize;
    while offset < bytes.len() {
        let opcode = bytes[offset];
        let size = route_op_size(opcode)?;
        let end = offset.checked_add(size).ok_or(R4G1Error::RangeOverflow)?;
        if end > bytes.len() {
            return Err(R4G1Error::TruncatedRoutingOp {
                offset: u32::try_from(offset).unwrap_or(u32::MAX),
                opcode,
            });
        }
        offset = end;
        if opcode == ROUT_OP_HALT {
            return Ok(offset);
        }
    }
    Ok(offset)
}

fn route_op_size(opcode: u8) -> Result<usize, R4G1Error> {
    match rout_op_size(opcode) {
        Some(size) => Ok(size),
        None => Err(R4G1Error::UnknownRoutingOp { offset: 0, opcode }),
    }
}

fn prototype_offset(base: usize, word: usize) -> Result<usize, R4G1Error> {
    base.checked_add(word.checked_shl(3).ok_or(R4G1Error::RangeOverflow)?)
        .ok_or(R4G1Error::RangeOverflow)
}

fn validate_emission_encoding(bytes: &[u8]) -> Result<(), R4G1Error> {
    if bytes.len() < 4 || bytes[0] != 2 {
        return Err(R4G1Error::EmissionEncodingUnsupported);
    }
    Ok(())
}

fn accumulate_emission_bytes<const CAPACITY: usize>(
    bytes: &[u8],
    predictions: &mut R4G1Predictions<CAPACITY>,
) {
    let mut offset = 0usize;
    while offset < bytes.len() {
        predictions.accumulate(R4G1Emission {
            token: read_u32(bytes, offset),
            score_q: read_i32(bytes, offset + 4),
        });
        offset += 8;
    }
}

fn decode_score(raw: i32, shift_byte: u8, zero_point: i16) -> Result<i32, R4G1Error> {
    let shift = shift_byte as i8;
    let adjusted = if shift >= 0 {
        match raw.checked_shl(u32::from(shift as u8)) {
            Some(value) => value,
            None if raw.is_negative() => i32::MIN,
            None => i32::MAX,
        }
    } else {
        match raw.checked_shr(u32::from(shift.saturating_neg() as u8)) {
            Some(value) => value,
            None if raw.is_negative() => -1,
            None => 0,
        }
    };
    Ok(adjusted.saturating_add(i32::from(zero_point)))
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn read_i16(bytes: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn read_i32(bytes: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn read_u64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([
        bytes[at],
        bytes[at + 1],
        bytes[at + 2],
        bytes[at + 3],
        bytes[at + 4],
        bytes[at + 5],
        bytes[at + 6],
        bytes[at + 7],
    ])
}

fn read_id(bytes: &[u8], at: usize) -> CodebookId {
    let mut id = [0u8; 32];
    id.copy_from_slice(&bytes[at..at + 32]);
    CodebookId::from_bytes(id)
}

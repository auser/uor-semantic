//! Zero-copy access to compiled semantic generation artifacts.

use core::fmt;

use crate::{CodebookId, Depth, MembershipMargin, PathId, RegionId, ScoreQ};

/// Artifact magic shared by stable semantic artifact formats.
pub const ARTIFACT_MAGIC: &[u8; 8] = b"UORSEM01";
/// Artifact format version.
/// Version two adds the serialized bounded candidate index to every artifact.
pub const ARTIFACT_VERSION: u16 = 2;
/// Packed semantic signature width in machine words.
pub const SIGNATURE_WORDS: usize = 4;
/// Maximum exact-context length stored by the format.
pub const MAX_CONTEXT_TOKENS: usize = 32;
/// Maximum divisible path depth stored by the format.
pub const MAX_ROUTE_DEPTH: usize = 8;
/// Fixed header width.
pub const HEADER_BYTES: usize = 256;
/// Fixed exact-context record width.
pub const EXACT_RECORD_BYTES: usize = 256;
/// Fixed semantic-region record width.
pub const REGION_RECORD_BYTES: usize = 128;
/// Fixed token-emission record width.
pub const EMISSION_RECORD_BYTES: usize = 8;
/// Maximum exact-context records accepted by the zero-copy parser.
pub const MAX_EXACT_RECORDS: usize = 1 << 16;
/// Maximum semantic-region records accepted by the zero-copy parser.
pub const MAX_REGION_RECORDS: usize = 1 << 12;
/// Maximum token-emission records accepted by the zero-copy parser.
pub const MAX_EMISSION_RECORDS: usize = 1 << 18;
/// Maximum packed artifact size accepted by the zero-copy parser.
pub const MAX_ARTIFACT_BYTES: usize = 1 << 26;
/// Number of coarse Hamming buckets in the serialized candidate index.
pub const INDEX_BUCKETS: usize = 1 << 8;
/// Maximum serialized region-index entries accepted by the parser.
pub const MAX_REGION_INDEX_ENTRIES: usize = 1 << 20;
const INDEX_TABLE_BYTES: usize = (INDEX_BUCKETS + 1) << 2;
const INDEX_OFFSET_FIELD: usize = 232;
const INDEX_BYTES_FIELD: usize = 240;

const EXACT_SHIFT: u32 = 8;
const REGION_SHIFT: u32 = 7;
const EMISSION_SHIFT: u32 = 3;
const EXACT_CONTEXT_OFFSET: usize = 48;
const REGION_PROTOTYPE_OFFSET: usize = 20;
const REGION_MASK_OFFSET: usize = 52;
const REGION_PATH_OFFSET: usize = 84;
const TOKENIZER_ID_OFFSET: usize = 128;
const CHAT_TEMPLATE_ID_OFFSET: usize = 160;
const SPECIAL_TOKENS_ID_OFFSET: usize = 192;
const EOS_TOKEN_OFFSET: usize = 224;

/// Failure to validate or access a semantic artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactError {
    /// The byte slice is shorter than the fixed header.
    HeaderTooShort,
    /// The format magic does not match [`ARTIFACT_MAGIC`].
    InvalidMagic,
    /// The artifact version is unsupported.
    UnsupportedVersion {
        /// Version found in the artifact.
        found: u16,
    },
    /// A fixed format dimension does not match this runtime.
    InvalidShape,
    /// An offset, count, or range exceeds addressable memory.
    RangeOverflow,
    /// A section points outside the supplied byte slice.
    SectionOutOfBounds,
    /// The declared artifact length does not equal the supplied byte length.
    LengthMismatch,
    /// The stored codebook identity does not match the artifact bytes.
    IdentityMismatch,
    /// A declared section exceeds a bounded parser resource limit.
    ResourceLimit {
        /// Section whose declared size exceeded its limit.
        section: &'static str,
        /// Declared count or byte length.
        provided: usize,
        /// Maximum accepted value.
        maximum: usize,
    },
    /// The serialized candidate index is malformed.
    InvalidIndex,
    /// A context exceeds [`MAX_CONTEXT_TOKENS`].
    ContextTooLong {
        /// Supplied token count.
        provided: usize,
    },
    /// A record contains an invalid field or references an invalid range.
    InvalidRecord,
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HeaderTooShort => formatter.write_str("artifact is shorter than its header"),
            Self::InvalidMagic => formatter.write_str("artifact magic does not match UORSEM01"),
            Self::UnsupportedVersion { found } => {
                write!(formatter, "artifact version {found} is unsupported")
            }
            Self::InvalidShape => formatter.write_str("artifact fixed dimensions are invalid"),
            Self::RangeOverflow => formatter.write_str("artifact range arithmetic overflowed"),
            Self::SectionOutOfBounds => formatter.write_str("artifact section is out of bounds"),
            Self::LengthMismatch => formatter.write_str("artifact declared length is incorrect"),
            Self::IdentityMismatch => formatter.write_str("artifact codebook identity is invalid"),
            Self::ResourceLimit {
                section,
                provided,
                maximum,
            } => write!(
                formatter,
                "artifact {section} is {provided}; parser maximum is {maximum}"
            ),
            Self::InvalidIndex => formatter.write_str("artifact candidate index is invalid"),
            Self::ContextTooLong { provided } => write!(
                formatter,
                "context contains {provided} tokens; format maximum is {MAX_CONTEXT_TOKENS}"
            ),
            Self::InvalidRecord => formatter.write_str("artifact record is invalid"),
        }
    }
}

impl core::error::Error for ArtifactError {}

/// Origin of one prediction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredictionSource {
    /// An exact compiled context supplied the emissions.
    Exact,
    /// One or more overlapping semantic regions supplied the emissions.
    Graph,
    /// Neither exact nor semantic evidence covered the context.
    Novel,
}

/// One token and its signed fixed-point score.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenScore {
    token: u32,
    score: ScoreQ,
}

impl TokenScore {
    /// Creates a token-score pair.
    pub const fn new(token: u32, score: ScoreQ) -> Self {
        Self { token, score }
    }

    /// Returns the token identifier.
    pub const fn token(self) -> u32 {
        self.token
    }

    /// Returns the fixed-point score.
    pub const fn score(self) -> ScoreQ {
        self.score
    }

    const EMPTY: Self = Self {
        token: 0,
        score: ScoreQ::from_raw(i32::MIN),
    };
}

/// Fixed-capacity prediction written without heap allocation.
#[must_use = "prediction source and truncation must be inspected"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Prediction<const CAPACITY: usize> {
    entries: [TokenScore; CAPACITY],
    len: usize,
    source: PredictionSource,
    truncated: usize,
}

impl<const CAPACITY: usize> Prediction<CAPACITY> {
    /// Creates an empty prediction.
    pub const fn new() -> Self {
        Self {
            entries: [TokenScore::EMPTY; CAPACITY],
            len: 0,
            source: PredictionSource::Novel,
            truncated: 0,
        }
    }

    /// Returns initialized token-score entries in canonical order.
    pub fn as_slice(&self) -> &[TokenScore] {
        &self.entries[..self.len]
    }

    /// Returns the highest-ranked token, when evidence exists.
    pub fn first(&self) -> Option<TokenScore> {
        self.as_slice().first().copied()
    }

    /// Returns the prediction source.
    pub const fn source(&self) -> PredictionSource {
        self.source
    }

    /// Returns entries omitted by fixed output capacity.
    pub const fn truncated(&self) -> usize {
        self.truncated
    }

    /// Clears initialized state while preserving caller-owned storage.
    pub fn clear(&mut self) {
        self.len = 0;
        self.source = PredictionSource::Novel;
        self.truncated = 0;
    }

    fn set_source(&mut self, source: PredictionSource) {
        self.source = source;
    }

    fn insert_exact(&mut self, entry: TokenScore) {
        if self.len < CAPACITY {
            self.entries[self.len] = entry;
            self.len += 1;
        } else {
            self.truncated = self.truncated.saturating_add(1);
        }
    }

    fn accumulate(&mut self, entry: TokenScore) {
        let mut index = 0usize;
        while index < self.len {
            if self.entries[index].token == entry.token {
                let sum = self.entries[index]
                    .score
                    .raw()
                    .saturating_add(entry.score.raw());
                self.entries[index].score = ScoreQ::from_raw(sum);
                self.sort_initialized();
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
        if CAPACITY == 0 {
            return;
        }
        let last = CAPACITY - 1;
        if token_score_precedes(entry, self.entries[last]) {
            self.entries[last] = entry;
            self.sort_initialized();
        }
    }

    fn sort_initialized(&mut self) {
        let mut outer = 1usize;
        while outer < self.len {
            let mut inner = outer;
            while inner > 0 && token_score_precedes(self.entries[inner], self.entries[inner - 1]) {
                self.entries.swap(inner, inner - 1);
                inner -= 1;
            }
            outer += 1;
        }
    }
}

impl<const CAPACITY: usize> Default for Prediction<CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

fn token_score_precedes(left: TokenScore, right: TokenScore) -> bool {
    if left.score != right.score {
        return left.score > right.score;
    }
    left.token < right.token
}

/// Whether prediction may use the exact-context section.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactPolicy {
    /// Prefer exact evidence and use regions only when exact evidence is absent.
    PreferExact,
    /// Ignore exact evidence to measure semantic-region generalization.
    GraphOnly,
}

/// Summary of one artifact prediction.
#[must_use = "prediction coverage must be inspected"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PredictionSummary {
    source: PredictionSource,
    exact_context_len: usize,
    regions_matched: usize,
    regions_retained: usize,
    regions_scanned: usize,
}

impl PredictionSummary {
    /// Returns the evidence source.
    pub const fn source(self) -> PredictionSource {
        self.source
    }

    /// Returns the exact suffix depth used, or zero when no exact record served.
    pub const fn exact_context_len(self) -> usize {
        self.exact_context_len
    }

    /// Returns all regions whose calibrated radius accepted the context.
    pub const fn regions_matched(self) -> usize {
        self.regions_matched
    }

    /// Returns regions retained in bounded scratch storage.
    pub const fn regions_retained(self) -> usize {
        self.regions_retained
    }

    /// Returns the indexed candidate regions inspected for this prediction.
    pub const fn regions_scanned(self) -> usize {
        self.regions_scanned
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ArtifactRouteEntry {
    record_index: usize,
    region_id: RegionId,
    path_id: PathId,
    depth: Depth,
    margin: MembershipMargin,
    distance: u64,
}

impl ArtifactRouteEntry {
    const EMPTY: Self = Self {
        record_index: 0,
        region_id: RegionId::new(0),
        path_id: PathId::new(0),
        depth: Depth::new(0),
        margin: MembershipMargin::new(0),
        distance: 0,
    };
}

/// Caller-owned fixed-capacity scratch for artifact prediction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactPredictScratch<const MAX_ACTIVE: usize> {
    entries: [ArtifactRouteEntry; MAX_ACTIVE],
    len: usize,
}

impl<const MAX_ACTIVE: usize> ArtifactPredictScratch<MAX_ACTIVE> {
    /// Creates empty prediction scratch.
    pub const fn new() -> Self {
        Self {
            entries: [ArtifactRouteEntry::EMPTY; MAX_ACTIVE],
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.len = 0;
    }

    fn as_slice(&self) -> &[ArtifactRouteEntry] {
        &self.entries[..self.len]
    }

    fn insert(&mut self, candidate: ArtifactRouteEntry) {
        if MAX_ACTIVE == 0 {
            return;
        }
        let mut position = 0usize;
        while position < self.len {
            if artifact_entry_precedes(candidate, self.entries[position]) {
                break;
            }
            position += 1;
        }

        if self.len < MAX_ACTIVE {
            let mut index = self.len;
            while index > position {
                self.entries[index] = self.entries[index - 1];
                index -= 1;
            }
            self.entries[position] = candidate;
            self.len += 1;
            return;
        }

        if position == MAX_ACTIVE {
            return;
        }
        let mut index = MAX_ACTIVE - 1;
        while index > position {
            self.entries[index] = self.entries[index - 1];
            index -= 1;
        }
        self.entries[position] = candidate;
    }
}

impl<const MAX_ACTIVE: usize> Default for ArtifactPredictScratch<MAX_ACTIVE> {
    fn default() -> Self {
        Self::new()
    }
}

fn artifact_entry_precedes(left: ArtifactRouteEntry, right: ArtifactRouteEntry) -> bool {
    if left.margin != right.margin {
        return left.margin > right.margin;
    }
    if left.depth != right.depth {
        return left.depth > right.depth;
    }
    if left.distance != right.distance {
        return left.distance < right.distance;
    }
    left.region_id < right.region_id
}

/// Borrowed zero-copy view over one validated artifact.
#[must_use = "artifact validation must not be discarded"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactView<'a> {
    bytes: &'a [u8],
    exact_count: usize,
    region_count: usize,
    emission_count: usize,
    exact_offset: usize,
    region_offset: usize,
    emission_offset: usize,
    index_offset: usize,
    index_entries: usize,
    codebook_id: CodebookId,
    source_id: CodebookId,
    tokenizer_id: CodebookId,
    chat_template_id: CodebookId,
    special_tokens_id: CodebookId,
    eos_token: u32,
}

impl<'a> ArtifactView<'a> {
    /// Validates and borrows an artifact without copying or allocating.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ArtifactError> {
        if bytes.len() < HEADER_BYTES {
            return Err(ArtifactError::HeaderTooShort);
        }
        let magic = bytes
            .get(..ARTIFACT_MAGIC.len())
            .ok_or(ArtifactError::HeaderTooShort)?;
        if magic != ARTIFACT_MAGIC {
            return Err(ArtifactError::InvalidMagic);
        }

        let version = read_u16(bytes, 8)?;
        if version != ARTIFACT_VERSION {
            return Err(ArtifactError::UnsupportedVersion { found: version });
        }
        if usize::from(read_u16(bytes, 10)?) != HEADER_BYTES
            || usize::from(read_u16(bytes, 12)?) != SIGNATURE_WORDS
            || usize::from(read_u16(bytes, 14)?) != MAX_CONTEXT_TOKENS
            || usize::from(read_u16(bytes, 16)?) != MAX_ROUTE_DEPTH
        {
            return Err(ArtifactError::InvalidShape);
        }

        let exact_count = usize_from_u32(read_u32(bytes, 20)?)?;
        let region_count = usize_from_u32(read_u32(bytes, 24)?)?;
        let emission_count = usize_from_u32(read_u32(bytes, 28)?)?;
        check_resource_limit("exact records", exact_count, MAX_EXACT_RECORDS)?;
        check_resource_limit("region records", region_count, MAX_REGION_RECORDS)?;
        check_resource_limit("emission records", emission_count, MAX_EMISSION_RECORDS)?;
        let exact_offset = usize_from_u64(read_u64(bytes, 32)?)?;
        let region_offset = usize_from_u64(read_u64(bytes, 40)?)?;
        let emission_offset = usize_from_u64(read_u64(bytes, 48)?)?;
        let declared_len = usize_from_u64(read_u64(bytes, 56)?)?;
        let index_offset = usize_from_u64(read_u64(bytes, INDEX_OFFSET_FIELD)?)?;
        let index_bytes = usize_from_u64(read_u64(bytes, INDEX_BYTES_FIELD)?)?;
        if declared_len != bytes.len() {
            return Err(ArtifactError::LengthMismatch);
        }
        check_resource_limit("total bytes", bytes.len(), MAX_ARTIFACT_BYTES)?;

        let exact_bytes = exact_count
            .checked_shl(EXACT_SHIFT)
            .ok_or(ArtifactError::RangeOverflow)?;
        let region_bytes = region_count
            .checked_shl(REGION_SHIFT)
            .ok_or(ArtifactError::RangeOverflow)?;
        let emission_bytes = emission_count
            .checked_shl(EMISSION_SHIFT)
            .ok_or(ArtifactError::RangeOverflow)?;
        let exact_end = exact_offset
            .checked_add(exact_bytes)
            .ok_or(ArtifactError::RangeOverflow)?;
        let region_end = region_offset
            .checked_add(region_bytes)
            .ok_or(ArtifactError::RangeOverflow)?;
        let emission_end = emission_offset
            .checked_add(emission_bytes)
            .ok_or(ArtifactError::RangeOverflow)?;

        if exact_offset < HEADER_BYTES
            || region_offset < exact_end
            || emission_offset < region_end
            || index_offset != emission_end
            || index_bytes < INDEX_TABLE_BYTES
            || index_offset
                .checked_add(index_bytes)
                .ok_or(ArtifactError::RangeOverflow)?
                != bytes.len()
        {
            return Err(ArtifactError::SectionOutOfBounds);
        }
        let entry_bytes = index_bytes
            .checked_sub(INDEX_TABLE_BYTES)
            .ok_or(ArtifactError::InvalidIndex)?;
        let index_entries = entry_bytes >> 2;
        if (index_entries << 2) != entry_bytes {
            return Err(ArtifactError::InvalidIndex);
        }
        check_resource_limit(
            "region index entries",
            index_entries,
            MAX_REGION_INDEX_ENTRIES,
        )?;

        let codebook_id = CodebookId::from_bytes(read_array_32(bytes, 64)?);
        let source_id = CodebookId::from_bytes(read_array_32(bytes, 96)?);
        let tokenizer_id = CodebookId::from_bytes(read_array_32(bytes, TOKENIZER_ID_OFFSET)?);
        let chat_template_id =
            CodebookId::from_bytes(read_array_32(bytes, CHAT_TEMPLATE_ID_OFFSET)?);
        let special_tokens_id =
            CodebookId::from_bytes(read_array_32(bytes, SPECIAL_TOKENS_ID_OFFSET)?);
        let eos_token = read_u32(bytes, EOS_TOKEN_OFFSET)?;
        let view = Self {
            bytes,
            exact_count,
            region_count,
            emission_count,
            exact_offset,
            region_offset,
            emission_offset,
            index_offset,
            index_entries,
            codebook_id,
            source_id,
            tokenizer_id,
            chat_template_id,
            special_tokens_id,
            eos_token,
        };
        view.validate_index()?;
        let digest = codebook_digest(bytes);
        if digest.as_slice() != view.codebook_id.as_bytes() {
            return Err(ArtifactError::IdentityMismatch);
        }
        view.validate_records()?;
        Ok(view)
    }

    /// Returns the artifact identity recorded by the compiler.
    pub const fn codebook_id(&self) -> CodebookId {
        self.codebook_id
    }

    /// Returns the source-manifest identity recorded by the compiler.
    pub const fn source_id(&self) -> CodebookId {
        self.source_id
    }

    /// Returns the tokenizer-files identity recorded by the compiler.
    pub const fn tokenizer_id(&self) -> CodebookId {
        self.tokenizer_id
    }

    /// Returns the chat-template identity recorded by the compiler.
    pub const fn chat_template_id(&self) -> CodebookId {
        self.chat_template_id
    }

    /// Returns the special-token-map identity recorded by the compiler.
    pub const fn special_tokens_id(&self) -> CodebookId {
        self.special_tokens_id
    }

    /// Returns the EOS token ID recorded by the compiler.
    pub const fn eos_token(&self) -> u32 {
        self.eos_token
    }

    /// Returns exact-context record count.
    pub const fn exact_count(&self) -> usize {
        self.exact_count
    }

    /// Returns semantic-region record count.
    pub const fn region_count(&self) -> usize {
        self.region_count
    }

    /// Returns token-emission record count.
    pub const fn emission_count(&self) -> usize {
        self.emission_count
    }

    /// Predicts into caller-owned fixed-capacity state.
    ///
    /// The method performs no heap allocation. Exact evidence uses bounded
    /// suffix backoff. Graph evidence scans the artifact-declared region count
    /// and retains at most `MAX_ACTIVE` overlapping regions.
    pub fn predict<const MAX_ACTIVE: usize, const MAX_OUTPUT: usize>(
        &self,
        context: &[u32],
        exact_policy: ExactPolicy,
        scratch: &mut ArtifactPredictScratch<MAX_ACTIVE>,
        prediction: &mut Prediction<MAX_OUTPUT>,
    ) -> Result<PredictionSummary, ArtifactError> {
        if context.len() > MAX_CONTEXT_TOKENS {
            return Err(ArtifactError::ContextTooLong {
                provided: context.len(),
            });
        }
        prediction.clear();
        scratch.clear();

        if exact_policy == ExactPolicy::PreferExact {
            let mut depth = context.len();
            while depth > 0 {
                let start = context.len() - depth;
                if let Some(record) = self.find_exact(&context[start..])? {
                    let mut emission_index = 0usize;
                    while emission_index < record.emission_len() {
                        prediction.insert_exact(
                            self.emission(record.emission_start().saturating_add(emission_index))?,
                        );
                        emission_index += 1;
                    }
                    prediction.set_source(PredictionSource::Exact);
                    return Ok(PredictionSummary {
                        source: PredictionSource::Exact,
                        exact_context_len: depth,
                        regions_matched: 0,
                        regions_retained: 0,
                        regions_scanned: 0,
                    });
                }
                depth -= 1;
            }
        }

        let signature = context_signature(context);
        let bucket = usize::from((signature[0] & 0xff) as u8);
        let (index_start, index_end) = self.index_range(bucket)?;
        let mut matched = 0usize;
        let mut index_position = index_start;
        while index_position < index_end {
            let region_index = self.index_entry(index_position)?;
            let region = self.region(region_index)?;
            let prototype = region.prototype()?;
            let mask = region.mask()?;
            let distance = crate::masked_hamming(&signature, &prototype, &mask);
            if distance <= region.radius() {
                matched = matched.saturating_add(1);
                scratch.insert(ArtifactRouteEntry {
                    record_index: region_index,
                    region_id: region.region_id(),
                    path_id: region.path_id(),
                    depth: region.depth(),
                    margin: MembershipMargin::new(region.radius().saturating_sub(distance)),
                    distance,
                });
            }
            index_position += 1;
        }

        for active in scratch.as_slice() {
            let region = self.region(active.record_index)?;
            let bonus = margin_bonus(active.margin);
            let mut emission_index = 0usize;
            while emission_index < region.emission_len() {
                let entry =
                    self.emission(region.emission_start().saturating_add(emission_index))?;
                prediction.accumulate(TokenScore::new(
                    entry.token(),
                    ScoreQ::from_raw(entry.score().raw().saturating_add(bonus)),
                ));
                emission_index += 1;
            }
        }

        let source = if prediction.first().is_some() {
            PredictionSource::Graph
        } else {
            PredictionSource::Novel
        };
        prediction.set_source(source);
        Ok(PredictionSummary {
            source,
            exact_context_len: 0,
            regions_matched: matched,
            regions_retained: scratch.len,
            regions_scanned: index_end.saturating_sub(index_start),
        })
    }

    fn index_range(&self, bucket: usize) -> Result<(usize, usize), ArtifactError> {
        if bucket >= INDEX_BUCKETS {
            return Err(ArtifactError::InvalidIndex);
        }
        let start = usize_from_u32(read_u32(
            self.bytes,
            self.index_offset.saturating_add(bucket << 2),
        )?)?;
        let end = usize_from_u32(read_u32(
            self.bytes,
            self.index_offset.saturating_add((bucket + 1) << 2),
        )?)?;
        if start > end || end > self.index_entries {
            return Err(ArtifactError::InvalidIndex);
        }
        Ok((start, end))
    }

    fn validate_index(&self) -> Result<(), ArtifactError> {
        let mut previous = 0usize;
        let mut bucket = 0usize;
        while bucket <= INDEX_BUCKETS {
            let value = usize_from_u32(read_u32(
                self.bytes,
                self.index_offset.saturating_add(bucket << 2),
            )?)?;
            if value < previous || value > self.index_entries {
                return Err(ArtifactError::InvalidIndex);
            }
            previous = value;
            bucket += 1;
        }
        let mut position = 0usize;
        while position < self.index_entries {
            self.index_entry(position)?;
            position += 1;
        }
        Ok(())
    }

    fn index_entry(&self, position: usize) -> Result<usize, ArtifactError> {
        if position >= self.index_entries {
            return Err(ArtifactError::InvalidIndex);
        }
        let offset = self
            .index_offset
            .saturating_add(INDEX_TABLE_BYTES)
            .checked_add(
                position
                    .checked_shl(2)
                    .ok_or(ArtifactError::RangeOverflow)?,
            )
            .ok_or(ArtifactError::RangeOverflow)?;
        let index = usize_from_u32(read_u32(self.bytes, offset)?)?;
        if index >= self.region_count {
            return Err(ArtifactError::InvalidIndex);
        }
        Ok(index)
    }

    fn validate_records(&self) -> Result<(), ArtifactError> {
        let mut index = 0usize;
        let mut previous_hash = 0u64;
        while index < self.exact_count {
            let record = self.exact(index)?;
            if record.context_len() > MAX_CONTEXT_TOKENS
                || record
                    .emission_start()
                    .saturating_add(record.emission_len())
                    > self.emission_count
                || (index != 0 && record.hash() < previous_hash)
            {
                return Err(ArtifactError::InvalidRecord);
            }
            previous_hash = record.hash();
            index += 1;
        }

        index = 0;
        while index < self.region_count {
            let record = self.region(index)?;
            if usize::from(record.path_len()) > MAX_ROUTE_DEPTH
                || record
                    .emission_start()
                    .saturating_add(record.emission_len())
                    > self.emission_count
            {
                return Err(ArtifactError::InvalidRecord);
            }
            index += 1;
        }
        Ok(())
    }

    fn find_exact(&self, context: &[u32]) -> Result<Option<ExactRecordView<'a>>, ArtifactError> {
        let target_hash = context_hash(context);
        let mut lower = 0usize;
        let mut upper = self.exact_count;
        while lower < upper {
            let middle = lower + ((upper - lower) >> 1);
            if self.exact(middle)?.hash() < target_hash {
                lower = middle + 1;
            } else {
                upper = middle;
            }
        }

        let mut index = lower;
        while index < self.exact_count {
            let record = self.exact(index)?;
            if record.hash() != target_hash {
                break;
            }
            if record.matches(context)? {
                return Ok(Some(record));
            }
            index += 1;
        }
        Ok(None)
    }

    fn exact(&self, index: usize) -> Result<ExactRecordView<'a>, ArtifactError> {
        if index >= self.exact_count {
            return Err(ArtifactError::InvalidRecord);
        }
        let relative = index
            .checked_shl(EXACT_SHIFT)
            .ok_or(ArtifactError::RangeOverflow)?;
        let start = self
            .exact_offset
            .checked_add(relative)
            .ok_or(ArtifactError::RangeOverflow)?;
        let end = start
            .checked_add(EXACT_RECORD_BYTES)
            .ok_or(ArtifactError::RangeOverflow)?;
        let bytes = self
            .bytes
            .get(start..end)
            .ok_or(ArtifactError::SectionOutOfBounds)?;
        Ok(ExactRecordView { bytes })
    }

    fn region(&self, index: usize) -> Result<RegionRecordView<'a>, ArtifactError> {
        if index >= self.region_count {
            return Err(ArtifactError::InvalidRecord);
        }
        let relative = index
            .checked_shl(REGION_SHIFT)
            .ok_or(ArtifactError::RangeOverflow)?;
        let start = self
            .region_offset
            .checked_add(relative)
            .ok_or(ArtifactError::RangeOverflow)?;
        let end = start
            .checked_add(REGION_RECORD_BYTES)
            .ok_or(ArtifactError::RangeOverflow)?;
        let bytes = self
            .bytes
            .get(start..end)
            .ok_or(ArtifactError::SectionOutOfBounds)?;
        Ok(RegionRecordView { bytes })
    }

    fn emission(&self, index: usize) -> Result<TokenScore, ArtifactError> {
        if index >= self.emission_count {
            return Err(ArtifactError::InvalidRecord);
        }
        let relative = index
            .checked_shl(EMISSION_SHIFT)
            .ok_or(ArtifactError::RangeOverflow)?;
        let start = self
            .emission_offset
            .checked_add(relative)
            .ok_or(ArtifactError::RangeOverflow)?;
        Ok(TokenScore::new(
            read_u32(self.bytes, start)?,
            ScoreQ::from_raw(read_i32(self.bytes, start.saturating_add(4))?),
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExactRecordView<'a> {
    bytes: &'a [u8],
}

impl ExactRecordView<'_> {
    fn hash(&self) -> u64 {
        read_u64_unchecked(self.bytes, 0)
    }

    fn context_len(&self) -> usize {
        usize::from(read_u16_unchecked(self.bytes, 8))
    }

    fn emission_len(&self) -> usize {
        usize::from(read_u16_unchecked(self.bytes, 10))
    }

    fn emission_start(&self) -> usize {
        usize_from_u32_unchecked(read_u32_unchecked(self.bytes, 12))
    }

    fn matches(&self, context: &[u32]) -> Result<bool, ArtifactError> {
        if context.len() != self.context_len() {
            return Ok(false);
        }
        let mut index = 0usize;
        while index < context.len() {
            let relative = index.checked_shl(2).ok_or(ArtifactError::RangeOverflow)?;
            let offset = EXACT_CONTEXT_OFFSET
                .checked_add(relative)
                .ok_or(ArtifactError::RangeOverflow)?;
            if read_u32(self.bytes, offset)? != context[index] {
                return Ok(false);
            }
            index += 1;
        }
        Ok(true)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RegionRecordView<'a> {
    bytes: &'a [u8],
}

impl RegionRecordView<'_> {
    fn region_id(&self) -> RegionId {
        RegionId::new(read_u32_unchecked(self.bytes, 0))
    }

    fn path_id(&self) -> PathId {
        PathId::new(read_u32_unchecked(self.bytes, 4))
    }

    fn depth(&self) -> Depth {
        Depth::new(self.bytes[8])
    }

    fn path_len(&self) -> u8 {
        self.bytes[9]
    }

    fn radius(&self) -> u64 {
        u64::from(read_u16_unchecked(self.bytes, 10))
    }

    fn emission_start(&self) -> usize {
        usize_from_u32_unchecked(read_u32_unchecked(self.bytes, 12))
    }

    fn emission_len(&self) -> usize {
        usize::from(read_u16_unchecked(self.bytes, 16))
    }

    fn prototype(&self) -> Result<[u64; SIGNATURE_WORDS], ArtifactError> {
        read_words_4(self.bytes, REGION_PROTOTYPE_OFFSET)
    }

    fn mask(&self) -> Result<[u64; SIGNATURE_WORDS], ArtifactError> {
        read_words_4(self.bytes, REGION_MASK_OFFSET)
    }

    #[allow(dead_code)]
    fn path_slot(&self, index: usize) -> Result<u16, ArtifactError> {
        if index >= MAX_ROUTE_DEPTH {
            return Err(ArtifactError::InvalidRecord);
        }
        let relative = index.checked_shl(1).ok_or(ArtifactError::RangeOverflow)?;
        read_u16(self.bytes, REGION_PATH_OFFSET.saturating_add(relative))
    }
}

/// Computes the deterministic, runtime-native context hash used by exact lookup.
pub fn context_hash(context: &[u32]) -> u64 {
    let mut state = 0x6a09_e667_f3bc_c909u64;
    let mut position = 0u64;
    let mut index = 0usize;
    while index < context.len() {
        let value = mix64(u64::from(context[index]).wrapping_add(position));
        state = state.rotate_left(7) ^ value;
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        position = position.wrapping_add(1);
        index += 1;
    }
    state ^ position.rotate_left(17)
}

/// Computes the four-word hyperdimensional route signature from token IDs.
///
/// The update uses only rotate, XOR, and integer addition, and is therefore
/// available inside the strict runtime without embedding tables or heap state.
pub fn context_signature(context: &[u32]) -> [u64; SIGNATURE_WORDS] {
    let mut output: [u64; SIGNATURE_WORDS] = [
        0x243f_6a88_85a3_08d3,
        0x1319_8a2e_0370_7344,
        0xa409_3822_299f_31d0,
        0x082e_fa98_ec4e_6c89,
    ];
    let mut position = 0u64;
    let mut index = 0usize;
    while index < context.len() {
        let base = mix64(u64::from(context[index]).wrapping_add(position));
        output[0] = output[0].rotate_left(1) ^ base;
        output[1] = output[1].rotate_left(3) ^ base.rotate_left(11);
        output[2] = output[2].rotate_left(5) ^ base.rotate_left(23);
        output[3] = output[3].rotate_left(7) ^ base.rotate_left(37);
        output[0] = output[0].wrapping_add(output[3].rotate_left(9));
        output[2] = output[2].wrapping_add(output[1].rotate_left(13));
        position = position.wrapping_add(1);
        index += 1;
    }
    output
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_add(0xbf58_476d_1ce4_e5b9);
    value ^= value << 27;
    value = value.rotate_left(31).wrapping_add(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn margin_bonus(margin: MembershipMargin) -> i32 {
    i32::try_from(margin.get()).unwrap_or(i32::MAX)
}

fn check_resource_limit(
    section: &'static str,
    provided: usize,
    maximum: usize,
) -> Result<(), ArtifactError> {
    if provided > maximum {
        return Err(ArtifactError::ResourceLimit {
            section,
            provided,
            maximum,
        });
    }
    Ok(())
}

const SHA_INITIAL: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

const SHA_ROUND: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

fn codebook_digest(bytes: &[u8]) -> [u8; 32] {
    let mut state = SHA_INITIAL;
    let mut offset = 0usize;
    while offset.saturating_add(64) <= bytes.len() {
        let mut block = [0u8; 64];
        let mut index = 0usize;
        while index < 64 {
            block[index] = identity_byte(bytes, offset.saturating_add(index));
            index += 1;
        }
        sha_compress(&mut state, &block);
        offset = offset.saturating_add(64);
    }

    let remaining = bytes.len().saturating_sub(offset);
    let mut block = [0u8; 64];
    let mut index = 0usize;
    while index < remaining {
        block[index] = identity_byte(bytes, offset.saturating_add(index));
        index += 1;
    }
    block[remaining] = 0x80;
    let bit_length = (bytes.len() as u64) << 3;
    if remaining >= 56 {
        sha_compress(&mut state, &block);
        block = [0u8; 64];
    }
    block[56..64].copy_from_slice(&bit_length.to_be_bytes());
    sha_compress(&mut state, &block);

    let mut output = [0u8; 32];
    let mut word = 0usize;
    while word < 8 {
        let start = word << 2;
        output[start..start + 4].copy_from_slice(&state[word].to_be_bytes());
        word += 1;
    }
    output
}

fn identity_byte(bytes: &[u8], index: usize) -> u8 {
    if (64..96).contains(&index) {
        0
    } else {
        bytes[index]
    }
}

fn sha_compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut words = [0u32; 64];
    let mut index = 0usize;
    while index < 16 {
        let start = index << 2;
        words[index] = u32::from_be_bytes([
            block[start],
            block[start + 1],
            block[start + 2],
            block[start + 3],
        ]);
        index += 1;
    }
    while index < 64 {
        let s0 = words[index - 15].rotate_right(7)
            ^ words[index - 15].rotate_right(18)
            ^ (words[index - 15] >> 3);
        let s1 = words[index - 2].rotate_right(17)
            ^ words[index - 2].rotate_right(19)
            ^ (words[index - 2] >> 10);
        words[index] = words[index - 16]
            .wrapping_add(s0)
            .wrapping_add(words[index - 7])
            .wrapping_add(s1);
        index += 1;
    }

    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    let mut f = state[5];
    let mut g = state[6];
    let mut h = state[7];
    index = 0;
    while index < 64 {
        let upper_e = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choice = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(upper_e)
            .wrapping_add(choice)
            .wrapping_add(SHA_ROUND[index])
            .wrapping_add(words[index]);
        let upper_a = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = upper_a.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
        index += 1;
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

fn read_words_4(bytes: &[u8], offset: usize) -> Result<[u64; SIGNATURE_WORDS], ArtifactError> {
    Ok([
        read_u64(bytes, offset)?,
        read_u64(bytes, offset.saturating_add(8))?,
        read_u64(bytes, offset.saturating_add(16))?,
        read_u64(bytes, offset.saturating_add(24))?,
    ])
}

fn read_array_32(bytes: &[u8], offset: usize) -> Result<[u8; 32], ArtifactError> {
    let end = offset.checked_add(32).ok_or(ArtifactError::RangeOverflow)?;
    let source = bytes
        .get(offset..end)
        .ok_or(ArtifactError::SectionOutOfBounds)?;
    let mut output = [0u8; 32];
    output.copy_from_slice(source);
    Ok(output)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ArtifactError> {
    let end = offset.checked_add(2).ok_or(ArtifactError::RangeOverflow)?;
    let source = bytes
        .get(offset..end)
        .ok_or(ArtifactError::SectionOutOfBounds)?;
    Ok(u16::from_le_bytes([source[0], source[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ArtifactError> {
    let end = offset.checked_add(4).ok_or(ArtifactError::RangeOverflow)?;
    let source = bytes
        .get(offset..end)
        .ok_or(ArtifactError::SectionOutOfBounds)?;
    Ok(u32::from_le_bytes([
        source[0], source[1], source[2], source[3],
    ]))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, ArtifactError> {
    let end = offset.checked_add(4).ok_or(ArtifactError::RangeOverflow)?;
    let source = bytes
        .get(offset..end)
        .ok_or(ArtifactError::SectionOutOfBounds)?;
    Ok(i32::from_le_bytes([
        source[0], source[1], source[2], source[3],
    ]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, ArtifactError> {
    let end = offset.checked_add(8).ok_or(ArtifactError::RangeOverflow)?;
    let source = bytes
        .get(offset..end)
        .ok_or(ArtifactError::SectionOutOfBounds)?;
    Ok(u64::from_le_bytes([
        source[0], source[1], source[2], source[3], source[4], source[5], source[6], source[7],
    ]))
}

fn read_u16_unchecked(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32_unchecked(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u64_unchecked(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

fn usize_from_u32(value: u32) -> Result<usize, ArtifactError> {
    usize::try_from(value).map_err(|_| ArtifactError::RangeOverflow)
}

fn usize_from_u64(value: u64) -> Result<usize, ArtifactError> {
    usize::try_from(value).map_err(|_| ArtifactError::RangeOverflow)
}

fn usize_from_u32_unchecked(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

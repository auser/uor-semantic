//! Deterministic binary Hamming-cover compiler.

use core::fmt;
use std::collections::BTreeMap;

use uor_semantic::{
    ARTIFACT_MAGIC, ARTIFACT_VERSION, ArtifactView, CodebookId, EMISSION_RECORD_BYTES,
    EXACT_RECORD_BYTES, HEADER_BYTES, INDEX_BUCKETS, MAX_ARTIFACT_BYTES, MAX_CONTEXT_TOKENS,
    MAX_EMISSION_RECORDS, MAX_EXACT_RECORDS, MAX_REGION_INDEX_ENTRIES, MAX_REGION_RECORDS,
    MAX_ROUTE_DEPTH, REGION_RECORD_BYTES, SIGNATURE_WORDS, context_hash, context_signature,
    masked_hamming,
};

use crate::observation::{Observation, ObservationCorpus, ObservedEmission};
use crate::sha256;

const MAX_REGION_MEMBERSHIPS: usize = 1 << 20;
const INDEX_TABLE_BYTES: usize = (INDEX_BUCKETS + 1) << 2;

/// Offline compiler configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompilerConfig {
    /// Maximum binary regions induced from the observation corpus.
    pub max_regions: usize,
    /// Deterministic assignment/update passes.
    pub iterations: usize,
    /// Additional Hamming radius used to retain boundary overlap.
    pub overlap_margin: u16,
    /// Maximum token emissions stored per region.
    pub max_region_emissions: usize,
    /// Whether exact-context records are retained.
    pub include_exact: bool,
}

impl CompilerConfig {
    /// Accuracy-oriented defaults.
    pub const fn accuracy() -> Self {
        Self {
            // This profile is the current cross-corpus graph-accuracy
            // candidate, retaining full coverage with bounded indexed work.
            max_regions: 48,
            iterations: 16,
            overlap_margin: 16,
            max_region_emissions: 1,
            include_exact: true,
        }
    }
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self::accuracy()
    }
}

/// Deterministic compiler output and measured shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledArtifact {
    /// Packed artifact bytes.
    pub bytes: Vec<u8>,
    /// Artifact identity computed with the identity header field zeroed.
    pub codebook_id: CodebookId,
    /// Canonical input observation count after duplicate elimination.
    pub observations: usize,
    /// Exact-context record count.
    pub exact_records: usize,
    /// Semantic-region record count.
    pub regions: usize,
    /// Token-emission record count.
    pub emissions: usize,
    /// Total observation-to-region memberships retained during calibration.
    pub memberships: usize,
}

/// Failure to compile an observation corpus.
#[derive(Debug)]
pub enum CompileError {
    /// A configuration field is invalid.
    InvalidConfiguration(&'static str),
    /// Canonical corpus is empty.
    EmptyCorpus,
    /// The same context carries conflicting teacher evidence.
    ConflictingContext,
    /// A format count or offset does not fit its declared integer width.
    FormatLimit(&'static str),
    /// The generated artifact failed its own runtime validator.
    SelfValidation(uor_semantic::ArtifactError),
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => {
                write!(formatter, "invalid compiler configuration: {message}")
            }
            Self::EmptyCorpus => formatter.write_str("observation corpus is empty"),
            Self::ConflictingContext => {
                formatter.write_str("one context carries conflicting teacher evidence")
            }
            Self::FormatLimit(message) => write!(formatter, "artifact format limit: {message}"),
            Self::SelfValidation(error) => {
                write!(formatter, "compiled artifact failed validation: {error}")
            }
        }
    }
}

impl std::error::Error for CompileError {}

/// Compiles observations into exact records and overlapping binary regions.
pub fn compile(
    corpus: &ObservationCorpus,
    config: CompilerConfig,
) -> Result<CompiledArtifact, CompileError> {
    validate_config(config)?;
    let observations = canonical_observations(&corpus.observations)?;
    if observations.is_empty() {
        return Err(CompileError::EmptyCorpus);
    }

    let signatures: Vec<[u64; SIGNATURE_WORDS]> = observations
        .iter()
        .map(|observation| context_signature(&observation.context))
        .collect();
    let region_count = config.max_regions.min(observations.len());
    let prototypes = induce_prototypes(&signatures, region_count, config.iterations);
    let (members, memberships) =
        assign_memberships(&signatures, &prototypes, config.overlap_margin)?;
    let paths = derive_paths(&prototypes);
    let regions = compile_regions(
        &observations,
        &signatures,
        &prototypes,
        &members,
        &paths,
        config,
    );
    let exact = if config.include_exact {
        compile_exact(&observations)
    } else {
        Vec::new()
    };

    let mut emissions = Vec::new();
    let exact_records = attach_exact_emissions(exact, &mut emissions)?;
    let region_records = attach_region_emissions(regions, &mut emissions)?;
    let bytes = write_artifact(
        &exact_records,
        &region_records,
        &emissions,
        &corpus.metadata,
    )?;
    let view = ArtifactView::parse(&bytes).map_err(CompileError::SelfValidation)?;

    Ok(CompiledArtifact {
        codebook_id: view.codebook_id(),
        observations: observations.len(),
        exact_records: exact_records.len(),
        regions: region_records.len(),
        emissions: emissions.len(),
        memberships,
        bytes,
    })
}

fn validate_config(config: CompilerConfig) -> Result<(), CompileError> {
    if config.max_regions == 0 {
        return Err(CompileError::InvalidConfiguration(
            "max_regions must be non-zero",
        ));
    }
    if config.iterations == 0 {
        return Err(CompileError::InvalidConfiguration(
            "iterations must be non-zero",
        ));
    }
    if config.max_region_emissions == 0 || config.max_region_emissions > usize::from(u16::MAX) {
        return Err(CompileError::InvalidConfiguration(
            "max_region_emissions is outside the format range",
        ));
    }
    Ok(())
}

fn canonical_observations(input: &[Observation]) -> Result<Vec<Observation>, CompileError> {
    let mut observations = input.to_vec();
    observations.sort_by(|left, right| left.context.cmp(&right.context));
    let mut output: Vec<Observation> = Vec::with_capacity(observations.len());
    for observation in observations {
        if observation.context.len() > MAX_CONTEXT_TOKENS {
            return Err(CompileError::FormatLimit("context is too long"));
        }
        if let Some(previous) = output.last()
            && previous.context == observation.context
        {
            if previous.target != observation.target || previous.emissions != observation.emissions
            {
                return Err(CompileError::ConflictingContext);
            }
            continue;
        }
        output.push(observation);
    }
    Ok(output)
}

fn induce_prototypes(
    signatures: &[[u64; SIGNATURE_WORDS]],
    count: usize,
    iterations: usize,
) -> Vec<[u64; SIGNATURE_WORDS]> {
    let mut prototypes = Vec::with_capacity(count);
    prototypes.push(signatures[0]);
    while prototypes.len() < count {
        let mut best_index = 0usize;
        let mut best_distance = 0u64;
        for (index, signature) in signatures.iter().enumerate() {
            let distance = nearest_distance(signature, &prototypes);
            if distance > best_distance {
                best_distance = distance;
                best_index = index;
            }
        }
        prototypes.push(signatures[best_index]);
    }

    for _pass in 0..iterations {
        let assignments = assign_all(signatures, &prototypes);
        let mut bit_counts = vec![[0u32; 256]; count];
        let mut member_counts = vec![0u32; count];
        for (signature, cluster) in signatures.iter().zip(assignments.iter().copied()) {
            member_counts[cluster] = member_counts[cluster].saturating_add(1);
            for (word, value) in signature.iter().copied().enumerate() {
                for bit in 0..64 {
                    if ((value >> bit) & 1) != 0 {
                        let flat = word * 64 + bit;
                        bit_counts[cluster][flat] = bit_counts[cluster][flat].saturating_add(1);
                    }
                }
            }
        }

        for cluster in 0..count {
            if member_counts[cluster] == 0 {
                continue;
            }
            let mut prototype = [0u64; SIGNATURE_WORDS];
            for (word, prototype_word) in prototype.iter_mut().enumerate() {
                for bit in 0..64 {
                    let flat = word * 64 + bit;
                    if bit_counts[cluster][flat].saturating_mul(2) >= member_counts[cluster] {
                        *prototype_word |= 1u64 << bit;
                    }
                }
            }
            prototypes[cluster] = prototype;
        }
    }
    prototypes
}

fn nearest_distance(
    signature: &[u64; SIGNATURE_WORDS],
    prototypes: &[[u64; SIGNATURE_WORDS]],
) -> u64 {
    let mask = [u64::MAX; SIGNATURE_WORDS];
    prototypes
        .iter()
        .map(|prototype| masked_hamming(signature, prototype, &mask))
        .min()
        .unwrap_or(u64::MAX)
}

fn assign_all(
    signatures: &[[u64; SIGNATURE_WORDS]],
    prototypes: &[[u64; SIGNATURE_WORDS]],
) -> Vec<usize> {
    let mask = [u64::MAX; SIGNATURE_WORDS];
    signatures
        .iter()
        .map(|signature| {
            prototypes
                .iter()
                .enumerate()
                .min_by_key(|(index, prototype)| {
                    (masked_hamming(signature, prototype, &mask), *index)
                })
                .map(|(index, _)| index)
                .unwrap_or(0)
        })
        .collect()
}

fn assign_memberships(
    signatures: &[[u64; SIGNATURE_WORDS]],
    prototypes: &[[u64; SIGNATURE_WORDS]],
    overlap_margin: u16,
) -> Result<(Vec<Vec<usize>>, usize), CompileError> {
    let mask = [u64::MAX; SIGNATURE_WORDS];
    let mut members = vec![Vec::new(); prototypes.len()];
    let mut membership_count = 0usize;
    for (observation_index, signature) in signatures.iter().enumerate() {
        let primary_distance = prototypes
            .iter()
            .map(|prototype| masked_hamming(signature, prototype, &mask))
            .min()
            .unwrap_or(u64::MAX);
        let threshold = primary_distance.saturating_add(u64::from(overlap_margin));
        for (cluster, prototype) in prototypes.iter().enumerate() {
            if masked_hamming(signature, prototype, &mask) <= threshold {
                membership_count = membership_count.saturating_add(1);
                if membership_count > MAX_REGION_MEMBERSHIPS {
                    return Err(CompileError::FormatLimit(
                        "observation-region memberships exceed compiler limit",
                    ));
                }
                members[cluster].push(observation_index);
            }
        }
    }
    Ok((members, membership_count))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DerivedPath {
    slots: [u16; MAX_ROUTE_DEPTH],
    len: u8,
}

fn derive_paths(prototypes: &[[u64; SIGNATURE_WORDS]]) -> Vec<DerivedPath> {
    let mask = [u64::MAX; SIGNATURE_WORDS];
    let empty = DerivedPath {
        slots: [0; MAX_ROUTE_DEPTH],
        len: 0,
    };
    let mut paths = vec![empty; prototypes.len()];
    let mut parents = vec![0usize; prototypes.len()];
    let mut child_counts = vec![0usize; prototypes.len()];
    for index in 0..prototypes.len() {
        if index == 0 {
            paths[index].len = 1;
            continue;
        }

        let mut parent = 0usize;
        let mut best_distance = u64::MAX;
        for candidate in 0..index {
            if usize::from(paths[candidate].len) >= MAX_ROUTE_DEPTH {
                continue;
            }
            let distance = masked_hamming(&prototypes[index], &prototypes[candidate], &mask);
            if distance < best_distance {
                best_distance = distance;
                parent = candidate;
            }
        }
        parents[index] = parent;
        let parent_len = usize::from(paths[parent].len);
        let mut path = paths[parent];
        let slot = u16::try_from(child_counts[parent].saturating_add(1)).unwrap_or(u16::MAX);
        child_counts[parent] = child_counts[parent].saturating_add(1);
        path.slots[parent_len] = slot;
        path.len = u8::try_from(parent_len.saturating_add(1)).unwrap_or(u8::MAX);
        paths[index] = path;
    }
    debug_assert!(parents.len() == paths.len());
    paths
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExactData {
    hash: u64,
    context: Vec<u32>,
    signature: [u64; SIGNATURE_WORDS],
    emissions: Vec<ObservedEmission>,
}

fn compile_exact(observations: &[Observation]) -> Vec<ExactData> {
    let mut exact: Vec<_> = observations
        .iter()
        .map(|observation| ExactData {
            hash: context_hash(&observation.context),
            context: observation.context.clone(),
            signature: context_signature(&observation.context),
            emissions: observation.emissions.clone(),
        })
        .collect();
    exact.sort_by(|left, right| {
        left.hash
            .cmp(&right.hash)
            .then_with(|| left.context.cmp(&right.context))
    });
    exact
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RegionData {
    prototype: [u64; SIGNATURE_WORDS],
    mask: [u64; SIGNATURE_WORDS],
    radius: u16,
    path: [u16; MAX_ROUTE_DEPTH],
    path_len: u8,
    emissions: Vec<ObservedEmission>,
}

fn compile_regions(
    observations: &[Observation],
    signatures: &[[u64; SIGNATURE_WORDS]],
    prototypes: &[[u64; SIGNATURE_WORDS]],
    members: &[Vec<usize>],
    paths: &[DerivedPath],
    config: CompilerConfig,
) -> Vec<RegionData> {
    let mask = [u64::MAX; SIGNATURE_WORDS];
    let mut regions = Vec::with_capacity(prototypes.len());
    for (cluster, prototype) in prototypes.iter().copied().enumerate() {
        let mut radius = 0u64;
        let mut scores: BTreeMap<u32, (i64, u64)> = BTreeMap::new();
        for observation_index in &members[cluster] {
            radius = radius.max(masked_hamming(
                &signatures[*observation_index],
                &prototype,
                &mask,
            ));
            for emission in &observations[*observation_index].emissions {
                let aggregate = scores.entry(emission.token).or_insert((0, 0));
                aggregate.0 = aggregate.0.saturating_add(i64::from(emission.score));
                aggregate.1 = aggregate.1.saturating_add(1);
            }
        }
        radius = radius.saturating_add(u64::from(config.overlap_margin));
        let radius = u16::try_from(radius.min(256)).unwrap_or(256);

        let mut emissions: Vec<ObservedEmission> = scores
            .into_iter()
            .map(|(token, (sum, count))| {
                let average = if count == 0 {
                    0
                } else {
                    sum / i64::try_from(count).unwrap_or(1)
                };
                let score = i32::try_from(average).unwrap_or(if average.is_negative() {
                    i32::MIN
                } else {
                    i32::MAX
                });
                ObservedEmission { token, score }
            })
            .collect();
        emissions.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.token.cmp(&right.token))
        });
        emissions.truncate(config.max_region_emissions);

        regions.push(RegionData {
            prototype,
            mask,
            radius,
            path: paths[cluster].slots,
            path_len: paths[cluster].len,
            emissions,
        });
    }
    regions
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExactRecordData {
    data: ExactData,
    emission_start: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RegionRecordData {
    data: RegionData,
    emission_start: u32,
}

fn attach_exact_emissions(
    input: Vec<ExactData>,
    emissions: &mut Vec<ObservedEmission>,
) -> Result<Vec<ExactRecordData>, CompileError> {
    let mut output = Vec::with_capacity(input.len());
    for data in input {
        let emission_start = u32::try_from(emissions.len())
            .map_err(|_| CompileError::FormatLimit("emission count exceeds u32"))?;
        emissions.extend_from_slice(&data.emissions);
        output.push(ExactRecordData {
            data,
            emission_start,
        });
    }
    Ok(output)
}

fn attach_region_emissions(
    input: Vec<RegionData>,
    emissions: &mut Vec<ObservedEmission>,
) -> Result<Vec<RegionRecordData>, CompileError> {
    let mut output = Vec::with_capacity(input.len());
    for data in input {
        let emission_start = u32::try_from(emissions.len())
            .map_err(|_| CompileError::FormatLimit("emission count exceeds u32"))?;
        emissions.extend_from_slice(&data.emissions);
        output.push(RegionRecordData {
            data,
            emission_start,
        });
    }
    Ok(output)
}

fn write_artifact(
    exact: &[ExactRecordData],
    regions: &[RegionRecordData],
    emissions: &[ObservedEmission],
    metadata: &crate::observation::ObservationMetadata,
) -> Result<Vec<u8>, CompileError> {
    if exact.len() > MAX_EXACT_RECORDS {
        return Err(CompileError::FormatLimit(
            "exact record count exceeds parser limit",
        ));
    }
    if regions.len() > MAX_REGION_RECORDS {
        return Err(CompileError::FormatLimit(
            "region record count exceeds parser limit",
        ));
    }
    if emissions.len() > MAX_EMISSION_RECORDS {
        return Err(CompileError::FormatLimit(
            "emission record count exceeds parser limit",
        ));
    }
    let index = build_region_index(regions)?;
    let index_entries = index.iter().map(Vec::len).sum::<usize>();
    let index_bytes = INDEX_TABLE_BYTES
        .checked_add(
            index_entries
                .checked_shl(2)
                .ok_or(CompileError::FormatLimit("region index size overflow"))?,
        )
        .ok_or(CompileError::FormatLimit("region index size overflow"))?;
    let exact_bytes = exact
        .len()
        .checked_mul(EXACT_RECORD_BYTES)
        .ok_or(CompileError::FormatLimit("exact section size overflow"))?;
    let region_bytes = regions
        .len()
        .checked_mul(REGION_RECORD_BYTES)
        .ok_or(CompileError::FormatLimit("region section size overflow"))?;
    let emission_bytes = emissions
        .len()
        .checked_mul(EMISSION_RECORD_BYTES)
        .ok_or(CompileError::FormatLimit("emission section size overflow"))?;
    let exact_offset = HEADER_BYTES;
    let region_offset = exact_offset
        .checked_add(exact_bytes)
        .ok_or(CompileError::FormatLimit("region offset overflow"))?;
    let emission_offset = region_offset
        .checked_add(region_bytes)
        .ok_or(CompileError::FormatLimit("emission offset overflow"))?;
    let index_offset = emission_offset
        .checked_add(emission_bytes)
        .ok_or(CompileError::FormatLimit("index offset overflow"))?;
    let total_len = index_offset
        .checked_add(index_bytes)
        .ok_or(CompileError::FormatLimit("artifact size overflow"))?;
    if total_len > MAX_ARTIFACT_BYTES {
        return Err(CompileError::FormatLimit(
            "artifact size exceeds parser limit",
        ));
    }

    let mut bytes = vec![0u8; total_len];
    bytes[..ARTIFACT_MAGIC.len()].copy_from_slice(ARTIFACT_MAGIC);
    put_u16(&mut bytes, 8, ARTIFACT_VERSION);
    put_u16(&mut bytes, 10, as_u16(HEADER_BYTES, "header size")?);
    put_u16(&mut bytes, 12, as_u16(SIGNATURE_WORDS, "signature words")?);
    put_u16(
        &mut bytes,
        14,
        as_u16(MAX_CONTEXT_TOKENS, "context tokens")?,
    );
    put_u16(&mut bytes, 16, as_u16(MAX_ROUTE_DEPTH, "route depth")?);
    put_u32(&mut bytes, 20, as_u32(exact.len(), "exact count")?);
    put_u32(&mut bytes, 24, as_u32(regions.len(), "region count")?);
    put_u32(&mut bytes, 28, as_u32(emissions.len(), "emission count")?);
    put_u64(&mut bytes, 32, as_u64(exact_offset, "exact offset")?);
    put_u64(&mut bytes, 40, as_u64(region_offset, "region offset")?);
    put_u64(&mut bytes, 48, as_u64(emission_offset, "emission offset")?);
    put_u64(&mut bytes, 56, as_u64(total_len, "total length")?);
    put_u64(&mut bytes, 232, as_u64(index_offset, "index offset")?);
    put_u64(&mut bytes, 240, as_u64(index_bytes, "index bytes")?);
    bytes[96..128].copy_from_slice(&metadata.source_sha256);
    bytes[128..160].copy_from_slice(&metadata.tokenizer_sha256);
    bytes[160..192].copy_from_slice(&metadata.chat_template_sha256);
    bytes[192..224].copy_from_slice(&metadata.special_tokens_sha256);
    put_u32(&mut bytes, 224, metadata.eos_token);

    for (index, record) in exact.iter().enumerate() {
        let start = exact_offset + index * EXACT_RECORD_BYTES;
        put_u64(&mut bytes, start, record.data.hash);
        put_u16(
            &mut bytes,
            start + 8,
            as_u16(record.data.context.len(), "context length")?,
        );
        put_u16(
            &mut bytes,
            start + 10,
            as_u16(record.data.emissions.len(), "exact emission length")?,
        );
        put_u32(&mut bytes, start + 12, record.emission_start);
        for word in 0..SIGNATURE_WORDS {
            put_u64(
                &mut bytes,
                start + 16 + word * 8,
                record.data.signature[word],
            );
        }
        for (token_index, token) in record.data.context.iter().copied().enumerate() {
            put_u32(&mut bytes, start + 48 + token_index * 4, token);
        }
    }

    for (index, record) in regions.iter().enumerate() {
        let start = region_offset + index * REGION_RECORD_BYTES;
        put_u32(&mut bytes, start, as_u32(index + 1, "region id")?);
        put_u32(&mut bytes, start + 4, as_u32(index + 1, "path id")?);
        bytes[start + 8] = record.data.path_len;
        bytes[start + 9] = record.data.path_len;
        put_u16(&mut bytes, start + 10, record.data.radius);
        put_u32(&mut bytes, start + 12, record.emission_start);
        put_u16(
            &mut bytes,
            start + 16,
            as_u16(record.data.emissions.len(), "region emission length")?,
        );
        for word in 0..SIGNATURE_WORDS {
            put_u64(
                &mut bytes,
                start + 20 + word * 8,
                record.data.prototype[word],
            );
            put_u64(&mut bytes, start + 52 + word * 8, record.data.mask[word]);
        }
        for slot in 0..MAX_ROUTE_DEPTH {
            put_u16(&mut bytes, start + 84 + slot * 2, record.data.path[slot]);
        }
    }

    for (index, emission) in emissions.iter().enumerate() {
        let start = emission_offset + index * EMISSION_RECORD_BYTES;
        put_u32(&mut bytes, start, emission.token);
        put_i32(&mut bytes, start + 4, emission.score);
    }

    let mut position = 0usize;
    for (bucket, entries) in index.iter().enumerate() {
        put_u32(
            &mut bytes,
            index_offset + (bucket << 2),
            as_u32(position, "index position")?,
        );
        for region_index in entries {
            let start = index_offset
                .checked_add(INDEX_TABLE_BYTES)
                .and_then(|value| value.checked_add(position << 2))
                .ok_or(CompileError::FormatLimit("index entry offset overflow"))?;
            put_u32(&mut bytes, start, *region_index);
            position = position.saturating_add(1);
        }
    }
    put_u32(
        &mut bytes,
        index_offset + (INDEX_BUCKETS << 2),
        as_u32(position, "index position")?,
    );

    let codebook_id = sha256::digest(&bytes);
    bytes[64..96].copy_from_slice(&codebook_id);
    Ok(bytes)
}

fn build_region_index(regions: &[RegionRecordData]) -> Result<Vec<Vec<u32>>, CompileError> {
    let mut index = vec![Vec::new(); INDEX_BUCKETS];
    let mut entries = 0usize;
    for (region_index, region) in regions.iter().enumerate() {
        let key = (region.data.prototype[0] & 0xff) as u8;
        for (bucket, bucket_entries) in index.iter_mut().enumerate() {
            let coarse_distance = (bucket as u8 ^ key).count_ones();
            if u64::from(coarse_distance) <= u64::from(region.data.radius) {
                entries = entries.saturating_add(1);
                if entries > MAX_REGION_INDEX_ENTRIES {
                    return Err(CompileError::FormatLimit(
                        "region index entries exceed parser limit",
                    ));
                }
                bucket_entries.push(
                    u32::try_from(region_index)
                        .map_err(|_| CompileError::FormatLimit("region index exceeds u32"))?,
                );
            }
        }
    }
    Ok(index)
}

fn put_u16(output: &mut [u8], offset: usize, value: u16) {
    output[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_i32(output: &mut [u8], offset: usize, value: i32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn as_u16(value: usize, field: &'static str) -> Result<u16, CompileError> {
    u16::try_from(value).map_err(|_| CompileError::FormatLimit(field))
}

fn as_u32(value: usize, field: &'static str) -> Result<u32, CompileError> {
    u32::try_from(value).map_err(|_| CompileError::FormatLimit(field))
}

fn as_u64(value: usize, field: &'static str) -> Result<u64, CompileError> {
    u64::try_from(value).map_err(|_| CompileError::FormatLimit(field))
}

#[cfg(test)]
mod tests {
    use super::{CompilerConfig, assign_memberships, compile, derive_paths};
    use crate::observation::ObservationCorpus;

    const CORPUS: &str = concat!(
        "UOROBS1\n",
        "model=fixture/model\n",
        "revision=0123456789abcdef\n",
        "source_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "max_context=4\n",
        "top_k=3\n",
        "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000002\n",
        "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000003\n",
        "eos_token=2\n",
        "--\n",
        "O|1,2|3|3:100,4:90,5:80\n",
        "O|1,2,3|4|4:110,3:80,5:70\n",
    );

    #[test]
    fn compiler_is_byte_deterministic() {
        let corpus = ObservationCorpus::parse(CORPUS).expect("fixture parses");
        let first = compile(&corpus, CompilerConfig::accuracy()).expect("first compile");
        let second = compile(&corpus, CompilerConfig::accuracy()).expect("second compile");
        assert_eq!(first.bytes, second.bytes);
        assert_eq!(first.exact_records, 2);
    }

    #[test]
    fn compiler_retains_overlapping_region_memberships_sr_03() {
        let signatures = [[0, 0, 0, 0], [1, 0, 0, 0]];
        let prototypes = [[0, 0, 0, 0], [1, 0, 0, 0]];
        let (members, count) = assign_memberships(&signatures, &prototypes, 1)
            .expect("bounded membership assignment succeeds");
        assert_eq!(count, 4);
        assert_eq!(members[0], vec![0, 1]);
        assert_eq!(members[1], vec![0, 1]);
    }

    #[test]
    fn learned_hierarchical_paths_follow_prototype_proximity_sp_02() {
        let prototypes = [[0, 0, 0, 0], [1, 0, 0, 0], [3, 0, 0, 0]];
        let paths = derive_paths(&prototypes);
        assert_eq!(paths[0].len, 1);
        assert_eq!(&paths[0].slots[..1], &[0]);
        assert_eq!(paths[1].len, 2);
        assert_eq!(&paths[1].slots[..2], &[0, 1]);
        assert_eq!(paths[2].len, 3);
        assert_eq!(&paths[2].slots[..3], &[0, 1, 1]);
    }
}

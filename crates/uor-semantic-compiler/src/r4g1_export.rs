//! Canonical structural R4G1 export for compiled semantic artifacts.
//!
//! This bridge deliberately emits the bounded container shape and valid
//! BLAKE3 identities, not a scored R4G1 certificate. The current semantic
//! artifact has no target-compatible refinement/forward edge evidence or
//! residual EXCT table, so those sections are represented conservatively.

use core::fmt;

use uor_semantic::{ArtifactError, ArtifactView, R4G1Graph, R4G1Section, R4G1Structure};

const HEADER_BYTES: usize = 88;
const SECTION_ENTRY_BYTES: usize = 16;
const ALIGNMENT_LOG2: usize = 3;
const HEAD_BYTES: usize = 224;
const NODE_RECORD_BYTES: usize = 30;
const SIGNATURE_WORDS: usize = 4;
const SIGNATURE_BYTES: u16 = 32;
const REGION_RECORD_BYTES: usize = 128;
const EMISSION_RECORD_BYTES: usize = 8;
const REGION_OFFSET_FIELD: usize = 40;
const EMISSION_OFFSET_FIELD: usize = 48;

/// A structurally valid R4G1 container emitted from a semantic artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct R4G1Export {
    /// Canonical R4G1 bytes, including patched CIDs.
    pub bytes: Vec<u8>,
    /// BLAKE3 identity stored in the container header.
    pub artifact_cid: [u8; 32],
    /// BLAKE3 identity of the HEAD section.
    pub head_cid: [u8; 32],
    /// Number of emitted NODE records, including the root node.
    pub node_count: u32,
    /// Number of canonical EDGE records.
    pub edge_count: u32,
}

/// Failure while converting a semantic artifact to structural R4G1 bytes.
#[derive(Debug)]
pub enum R4G1ExportError {
    /// The source artifact is not valid according to the no-heap runtime view.
    Artifact(ArtifactError),
    /// A source value cannot be represented by the fixed R4G1 field.
    FormatLimit(&'static str),
    /// A source token cannot be represented by the signed R4G1 EMIT token field.
    TokenOutOfRange {
        /// Token identifier that exceeded the signed wire range.
        token: u32,
    },
    /// A generated container failed its own structural validation.
    Structural(uor_semantic::R4G1Error),
    /// A generated container has an invalid or tampered CID.
    InvalidCid(&'static str),
}

impl fmt::Display for R4G1ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Artifact(error) => write!(formatter, "source artifact is invalid: {error}"),
            Self::FormatLimit(message) => write!(formatter, "R4G1 export limit: {message}"),
            Self::TokenOutOfRange { token } => {
                write!(
                    formatter,
                    "token {token} does not fit the R4G1 signed token field"
                )
            }
            Self::Structural(error) => {
                write!(formatter, "R4G1 export is structurally invalid: {error}")
            }
            Self::InvalidCid(which) => write!(formatter, "R4G1 {which} CID is invalid"),
        }
    }
}

impl std::error::Error for R4G1ExportError {}

impl From<ArtifactError> for R4G1ExportError {
    fn from(error: ArtifactError) -> Self {
        Self::Artifact(error)
    }
}

/// Converts a validated `.uors` artifact into a deterministic structural R4G1 container.
pub fn export_r4g1(artifact: &CompiledArtifact) -> Result<R4G1Export, R4G1ExportError> {
    let view = ArtifactView::parse(&artifact.bytes)?;
    let region_count = view.region_count();
    let node_count = region_count
        .checked_add(1)
        .ok_or(R4G1ExportError::FormatLimit("node count overflow"))?;
    let node_count_u32 = u32::try_from(node_count)
        .map_err(|_| R4G1ExportError::FormatLimit("node count exceeds u32"))?;

    let region_offset = read_u64(&artifact.bytes, REGION_OFFSET_FIELD)?;
    let emission_offset = read_u64(&artifact.bytes, EMISSION_OFFSET_FIELD)?;
    let region_offset = usize::try_from(region_offset)
        .map_err(|_| R4G1ExportError::FormatLimit("region offset exceeds usize"))?;
    let emission_offset = usize::try_from(emission_offset)
        .map_err(|_| R4G1ExportError::FormatLimit("emission offset exceeds usize"))?;

    let mut emit = vec![2, 0, 0, 0];
    let mut nodes = Vec::with_capacity(node_count);
    nodes.push(Node {
        emission_start: 0,
        emission_len: 0,
        prototype_word_start: 1,
        mask_word_start: 1 + node_count_u32 * SIGNATURE_WORDS as u32,
        radius: 0,
        depth: 0,
    });

    let mut max_emissions = 0u32;
    let mut max_token = None;
    for region_index in 0..region_count {
        let source_start = region_offset
            .checked_add(
                region_index
                    .checked_mul(REGION_RECORD_BYTES)
                    .ok_or(R4G1ExportError::FormatLimit("region offset overflow"))?,
            )
            .ok_or(R4G1ExportError::FormatLimit("region offset overflow"))?;
        let source = artifact
            .bytes
            .get(source_start..source_start + REGION_RECORD_BYTES)
            .ok_or(R4G1ExportError::FormatLimit(
                "region record is out of bounds",
            ))?;
        let emission_start = read_u32(source, 12)? as usize;
        let emission_len = usize::from(read_u16(source, 16)?);
        let source_emission_start = emission_offset
            .checked_add(
                emission_start
                    .checked_mul(EMISSION_RECORD_BYTES)
                    .ok_or(R4G1ExportError::FormatLimit("emission offset overflow"))?,
            )
            .ok_or(R4G1ExportError::FormatLimit("emission offset overflow"))?;
        let emission_bytes =
            emission_len
                .checked_mul(EMISSION_RECORD_BYTES)
                .ok_or(R4G1ExportError::FormatLimit(
                    "emission byte length overflow",
                ))?;
        let source_emissions = artifact
            .bytes
            .get(source_emission_start..source_emission_start + emission_bytes)
            .ok_or(R4G1ExportError::FormatLimit(
                "emission records are out of bounds",
            ))?;
        let exported_emission_start = emit
            .len()
            .checked_sub(4)
            .ok_or(R4G1ExportError::FormatLimit("EMIT descriptor is missing"))?;
        let exported_emission_start = u32::try_from(exported_emission_start)
            .map_err(|_| R4G1ExportError::FormatLimit("emission offset exceeds u32"))?;
        for entry in source_emissions.chunks_exact(EMISSION_RECORD_BYTES) {
            let token = read_u32(entry, 0)?;
            if token > i32::MAX as u32 {
                return Err(R4G1ExportError::TokenOutOfRange { token });
            }
            max_token = Some(max_token.map_or(token, |current: u32| current.max(token)));
            emit.extend_from_slice(entry);
        }
        max_emissions = max_emissions.max(u32::try_from(emission_len).unwrap_or(u32::MAX));

        let prototype_word_start = 1u32
            .checked_add(
                u32::try_from(region_index + 1)
                    .map_err(|_| R4G1ExportError::FormatLimit("node index exceeds u32"))?
                    .saturating_mul(SIGNATURE_WORDS as u32),
            )
            .ok_or(R4G1ExportError::FormatLimit(
                "ROUT prototype offset overflow",
            ))?;
        let mask_word_start = 1u32
            .checked_add(
                node_count_u32
                    .checked_add(u32::try_from(region_index + 1).unwrap_or(u32::MAX))
                    .ok_or(R4G1ExportError::FormatLimit("ROUT mask offset overflow"))?
                    .saturating_mul(SIGNATURE_WORDS as u32),
            )
            .ok_or(R4G1ExportError::FormatLimit("ROUT mask offset overflow"))?;
        nodes.push(Node {
            emission_start: exported_emission_start,
            emission_len: u16::try_from(emission_len)
                .map_err(|_| R4G1ExportError::FormatLimit("emission count exceeds u16"))?,
            prototype_word_start,
            mask_word_start,
            radius: read_u16(source, 10)?,
            depth: source[8],
        });
    }

    let rout_words = 1usize
        .checked_add(
            node_count
                .checked_mul(SIGNATURE_WORDS)
                .ok_or(R4G1ExportError::FormatLimit("ROUT word count overflow"))?,
        )
        .and_then(|words| words.checked_add(node_count.checked_mul(SIGNATURE_WORDS)?))
        .ok_or(R4G1ExportError::FormatLimit("ROUT word count overflow"))?;
    let mut rout = vec![0u8; rout_words * 8];
    rout[0] = 0;
    for (index, node) in nodes.iter().enumerate() {
        let source_start = if index == 0 {
            None
        } else {
            Some(region_offset + (index - 1) * REGION_RECORD_BYTES)
        };
        let prototype_offset = node.prototype_word_start as usize * 8;
        let mask_offset = node.mask_word_start as usize * 8;
        if let Some(source_start) = source_start {
            let source = &artifact.bytes[source_start..source_start + REGION_RECORD_BYTES];
            rout[prototype_offset..prototype_offset + 32].copy_from_slice(&source[20..52]);
            rout[mask_offset..mask_offset + 32].copy_from_slice(&source[52..84]);
        }
    }

    let mut node_section = Vec::with_capacity(node_count * NODE_RECORD_BYTES);
    for node in &nodes {
        node_section.extend_from_slice(&0u32.to_le_bytes());
        node_section.extend_from_slice(&0u16.to_le_bytes());
        node_section.extend_from_slice(&0u32.to_le_bytes());
        node_section.extend_from_slice(&0u16.to_le_bytes());
        node_section.extend_from_slice(&node.emission_start.to_le_bytes());
        node_section.extend_from_slice(&node.emission_len.to_le_bytes());
        node_section.extend_from_slice(&node.prototype_word_start.to_le_bytes());
        node_section.extend_from_slice(&node.mask_word_start.to_le_bytes());
        node_section.extend_from_slice(&node.radius.to_le_bytes());
        node_section.push(node.depth);
        node_section.push(0);
    }

    let max_depth = nodes.iter().map(|node| node.depth).max().unwrap_or(0);
    let depth_count = max_depth
        .checked_add(1)
        .ok_or(R4G1ExportError::FormatLimit("depth count overflow"))?;
    let vocab_size = max_token.map_or(0, |token| token.saturating_add(1));
    let mut head = vec![0u8; HEAD_BYTES];
    head[0..32].copy_from_slice(view.codebook_id().as_bytes());
    head[32..64].copy_from_slice(view.tokenizer_id().as_bytes());
    head[64..96].copy_from_slice(view.source_id().as_bytes());
    head[128..148].fill(0);
    head[148..180].copy_from_slice(blake3::hash(b"uor-semantic structural-r4g1-v1").as_bytes());
    put_u16(&mut head, 180, 64);
    put_u16(&mut head, 182, 64);
    put_u16(&mut head, 184, SIGNATURE_WORDS as u16);
    put_u16(&mut head, 186, 1);
    put_u32(&mut head, 188, max_emissions.max(1));
    put_u32(&mut head, 192, 1);
    put_u32(&mut head, 196, node_count_u32);
    put_u32(&mut head, 200, 0);
    head[204] = depth_count;
    put_u16(&mut head, 212, SIGNATURE_BYTES);
    put_u32(&mut head, 220, vocab_size);

    let prov = b"uor-semantic structural-r4g1-v1\n";
    let sections = [
        (1u32, head.as_slice()),
        (2u32, &[0u8][..]),
        (3u32, node_section.as_slice()),
        (4u32, &[][..]),
        (5u32, rout.as_slice()),
        (6u32, emit.as_slice()),
        (8u32, prov.as_slice()),
    ];
    let mut bytes = emit_container(&sections)?;
    let head_offset = section_offset(&bytes, 1)?;
    let head_hash = blake3::hash(&bytes[head_offset..head_offset + HEAD_BYTES]);
    bytes[56..88].copy_from_slice(head_hash.as_bytes());
    let artifact_hash = blake3::hash(&bytes[56..]);
    bytes[24..56].copy_from_slice(artifact_hash.as_bytes());
    let mut artifact_cid = [0u8; 32];
    artifact_cid.copy_from_slice(artifact_hash.as_bytes());
    let mut head_cid = [0u8; 32];
    head_cid.copy_from_slice(head_hash.as_bytes());

    let graph = R4G1Graph::parse(&bytes).map_err(R4G1ExportError::Structural)?;
    let _ = graph;
    verify_r4g1_cids(&bytes)?;
    Ok(R4G1Export {
        bytes,
        artifact_cid,
        head_cid,
        node_count: node_count_u32,
        edge_count: 0,
    })
}

/// Verifies the canonical BLAKE3 identities in an R4G1 container.
pub fn verify_r4g1_cids(bytes: &[u8]) -> Result<(), R4G1ExportError> {
    let structure = R4G1Structure::parse(bytes).map_err(R4G1ExportError::Structural)?;
    let head = structure
        .section(R4G1Section::Head)
        .ok_or(R4G1ExportError::InvalidCid("HEAD"))?;
    let expected_head = blake3::hash(head);
    if bytes.get(56..88) != Some(expected_head.as_bytes()) {
        return Err(R4G1ExportError::InvalidCid("HEAD"));
    }
    let expected_artifact = blake3::hash(&bytes[56..]);
    if bytes.get(24..56) != Some(expected_artifact.as_bytes()) {
        return Err(R4G1ExportError::InvalidCid("artifact"));
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Node {
    emission_start: u32,
    emission_len: u16,
    prototype_word_start: u32,
    mask_word_start: u32,
    radius: u16,
    depth: u8,
}

fn emit_container(sections: &[(u32, &[u8])]) -> Result<Vec<u8>, R4G1ExportError> {
    let table_end = HEADER_BYTES
        .checked_add(
            sections
                .len()
                .checked_mul(SECTION_ENTRY_BYTES)
                .ok_or(R4G1ExportError::FormatLimit("section table overflow"))?,
        )
        .ok_or(R4G1ExportError::FormatLimit("section table overflow"))?;
    let mut offsets = Vec::with_capacity(sections.len());
    let mut cursor = align_up(table_end, 1usize << ALIGNMENT_LOG2)?;
    for (_, payload) in sections {
        let offset = cursor;
        cursor = cursor
            .checked_add(payload.len())
            .ok_or(R4G1ExportError::FormatLimit("section body overflow"))?;
        offsets.push((offset, payload.len()));
        cursor = align_up(cursor, 1usize << ALIGNMENT_LOG2)?;
    }
    let total_len = offsets
        .last()
        .map_or(table_end, |(offset, length)| offset + length);
    let total_len_u64 = u64::try_from(total_len)
        .map_err(|_| R4G1ExportError::FormatLimit("container length exceeds u64"))?;
    let mut bytes = vec![0u8; total_len];
    bytes[0..4].copy_from_slice(b"R4G1");
    bytes[4] = 0;
    bytes[5] = 0;
    bytes[6] = 1;
    bytes[7] = ALIGNMENT_LOG2 as u8;
    bytes[8..16].copy_from_slice(&total_len_u64.to_le_bytes());
    bytes[16..20].copy_from_slice(&(sections.len() as u32).to_le_bytes());
    for (index, ((id, payload), (offset, length))) in sections.iter().zip(offsets).enumerate() {
        let entry = HEADER_BYTES + index * SECTION_ENTRY_BYTES;
        bytes[entry..entry + 4].copy_from_slice(&id.to_le_bytes());
        bytes[entry + 8..entry + 12].copy_from_slice(&(offset as u32).to_le_bytes());
        bytes[entry + 12..entry + 16].copy_from_slice(&(length as u32).to_le_bytes());
        bytes[offset..offset + length].copy_from_slice(payload);
    }
    Ok(bytes)
}

fn align_up(value: usize, alignment: usize) -> Result<usize, R4G1ExportError> {
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .ok_or(R4G1ExportError::FormatLimit("alignment overflow"))
}

fn section_offset(bytes: &[u8], wanted: u32) -> Result<usize, R4G1ExportError> {
    let section_count = read_u32(bytes, 16)? as usize;
    for index in 0..section_count {
        let entry = HEADER_BYTES + index * SECTION_ENTRY_BYTES;
        if read_u32(bytes, entry)? == wanted {
            return usize::try_from(read_u32(bytes, entry + 8)?)
                .map_err(|_| R4G1ExportError::FormatLimit("section offset exceeds usize"));
        }
    }
    Err(R4G1ExportError::FormatLimit("HEAD section is missing"))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, R4G1ExportError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(R4G1ExportError::FormatLimit(
            "source field is out of bounds",
        ))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, R4G1ExportError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(R4G1ExportError::FormatLimit(
            "source field is out of bounds",
        ))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, R4G1ExportError> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or(R4G1ExportError::FormatLimit(
            "source field is out of bounds",
        ))?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

use crate::CompiledArtifact;

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

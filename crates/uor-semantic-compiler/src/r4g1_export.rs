//! Canonical structural R4G1 export for compiled semantic artifacts.
//!
//! This bridge deliberately emits the bounded container shape and valid
//! BLAKE3 identities, not a scored R4G1 certificate. The emitted EMIT root
//! prior and RX1-framed EXCT table are structural evidence derived from the
//! semantic artifact; their synthetic route-code keys are not target graded
//! codes and therefore do not claim scored R4G1 equivalence. Predictive
//! kind-2 edges are emitted only from deterministic continuation evidence. The
//! replay certificate below checks this exporter/source boundary, not target
//! runtime equivalence.

use core::fmt;
use std::collections::BTreeMap;

use uor_semantic::{
    ArtifactError, ArtifactView, R4G1Graph, R4G1Section, R4G1Structure, ResidualContribution,
    ResidualContributionKind, ScoreAccumulator,
};

const HEADER_BYTES: usize = 88;
const SECTION_ENTRY_BYTES: usize = 16;
const ALIGNMENT_LOG2: usize = 3;
const HEAD_BYTES: usize = 224;
const NODE_RECORD_BYTES: usize = 30;
const SIGNATURE_WORDS: usize = 4;
const SIGNATURE_BYTES: u16 = 32;
const REGION_RECORD_BYTES: usize = 128;
const EMISSION_RECORD_BYTES: usize = 8;
const EDGE_RECORD_BYTES: usize = 16;
const REVERSE_INDEX_ENTRY_BYTES: usize = 4;
const EXCT_LEVELS: usize = 5;
const EXACT_RECORD_BYTES: usize = 256;
const EXACT_OFFSET_FIELD: usize = 32;
const MAX_PREDICTIVE_EDGES_PER_SOURCE: usize = 8;
const MAX_PREDICTIVE_EVIDENCE: usize = 64;
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

/// Deterministic replay evidence for an emitted R4G1 predictive graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct R4G1ReplayReport {
    /// Number of predictive transitions recomputed from the source artifact.
    pub expected_transitions: u32,
    /// Number of predictive edges present in the R4G1 graph.
    pub emitted_predictive_edges: u32,
    /// Recomputed transitions whose source, destination, and kind were emitted.
    pub matched_transitions: u32,
    /// Matched transitions whose fixed-point score exactly agrees.
    pub score_matches: u32,
}

impl R4G1ReplayReport {
    /// Returns score agreement in basis points over recomputed transitions.
    pub const fn score_agreement_basis_points(self) -> u16 {
        if self.expected_transitions == 0 {
            return 10_000;
        }
        let value = self.score_matches.saturating_mul(10_000) / self.expected_transitions;
        if value > 10_000 { 10_000 } else { value as u16 }
    }

    /// Returns whether every recomputed transition and score was emitted exactly once.
    pub const fn is_complete(self) -> bool {
        self.expected_transitions == self.emitted_predictive_edges
            && self.expected_transitions == self.matched_transitions
            && self.expected_transitions == self.score_matches
    }

    /// Serializes the bounded report for CLI and archival use.
    pub fn to_json(self) -> String {
        format!(
            "{{\"expected_transitions\":{},\"emitted_predictive_edges\":{},\"matched_transitions\":{},\"score_matches\":{},\"score_agreement_basis_points\":{},\"complete\":{}}}",
            self.expected_transitions,
            self.emitted_predictive_edges,
            self.matched_transitions,
            self.score_matches,
            self.score_agreement_basis_points(),
            self.is_complete(),
        )
    }
}

/// Failure while replaying an R4G1 predictive graph against its source artifact.
#[derive(Debug)]
pub enum R4G1ReplayError {
    /// The source semantic artifact is invalid.
    Artifact(ArtifactError),
    /// The R4G1 graph is structurally or semantically invalid.
    Structural(uor_semantic::R4G1Error),
    /// The graph could not be compared with the source evidence.
    FormatLimit(&'static str),
    /// The R4G1 root prior is malformed or not canonically ordered.
    InvalidRootPrior,
    /// Recomputing source transitions failed.
    Export(R4G1ExportError),
}

impl fmt::Display for R4G1ReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Artifact(error) => write!(formatter, "source artifact is invalid: {error}"),
            Self::Structural(error) => write!(formatter, "R4G1 graph is invalid: {error}"),
            Self::FormatLimit(message) => write!(formatter, "R4G1 replay limit: {message}"),
            Self::InvalidRootPrior => formatter.write_str("R4G1 EMIT root prior is invalid"),
            Self::Export(error) => {
                write!(formatter, "R4G1 transition recomputation failed: {error}")
            }
        }
    }
}

impl std::error::Error for R4G1ReplayError {}

/// Recomputes bounded predictive transitions and compares them with an R4G1 export.
///
/// This is an exporter/source replay certificate: it proves that the emitted
/// predictive edges and fixed-point scores cover the source artifact's bounded
/// transition evidence. It is not a certificate of replay equivalence with the
/// separate R4 target runtime or teacher model.
pub fn replay_r4g1(artifact: &[u8], r4g1: &[u8]) -> Result<R4G1ReplayReport, R4G1ReplayError> {
    let view = ArtifactView::parse(artifact).map_err(R4G1ReplayError::Artifact)?;
    let graph = R4G1Graph::parse(r4g1).map_err(R4G1ReplayError::Structural)?;
    let root_scores = replay_root_scores(&graph)?;
    let region_offset =
        usize::try_from(read_u64(artifact, REGION_OFFSET_FIELD).map_err(|error| {
            R4G1ReplayError::FormatLimit(match error {
                R4G1ExportError::FormatLimit(message) => message,
                _ => "region offset is unavailable",
            })
        })?)
        .map_err(|_| R4G1ReplayError::FormatLimit("region offset exceeds usize"))?;
    let emission_offset =
        usize::try_from(read_u64(artifact, EMISSION_OFFSET_FIELD).map_err(|error| {
            R4G1ReplayError::FormatLimit(match error {
                R4G1ExportError::FormatLimit(message) => message,
                _ => "emission offset is unavailable",
            })
        })?)
        .map_err(|_| R4G1ReplayError::FormatLimit("emission offset exceeds usize"))?;
    let expected = build_predictive_edges(
        artifact,
        &view,
        region_offset,
        emission_offset,
        &root_scores,
    )
    .map_err(R4G1ReplayError::Export)?;

    let mut emitted_predictive_edges = 0u32;
    let mut edge_index = 0u32;
    while edge_index < graph.edge_count() {
        if graph
            .edge(edge_index)
            .ok_or(R4G1ReplayError::FormatLimit(
                "edge disappeared during replay",
            ))?
            .kind
            == 2
        {
            emitted_predictive_edges = emitted_predictive_edges.saturating_add(1);
        }
        edge_index += 1;
    }

    let mut matched_transitions = 0u32;
    let mut score_matches = 0u32;
    for expected_edge in &expected {
        let mut candidate = 0u32;
        while candidate < graph.edge_count() {
            let edge = graph.edge(candidate).ok_or(R4G1ReplayError::FormatLimit(
                "edge disappeared during replay",
            ))?;
            if edge.kind == expected_edge.kind
                && edge.src == expected_edge.src
                && edge.dst == expected_edge.dst
            {
                matched_transitions = matched_transitions.saturating_add(1);
                if edge.score_q == expected_edge.score_q {
                    score_matches = score_matches.saturating_add(1);
                }
                break;
            }
            candidate += 1;
        }
    }

    Ok(R4G1ReplayReport {
        expected_transitions: u32::try_from(expected.len())
            .map_err(|_| R4G1ReplayError::FormatLimit("expected transition count exceeds u32"))?,
        emitted_predictive_edges,
        matched_transitions,
        score_matches,
    })
}

fn replay_root_scores(graph: &R4G1Graph<'_>) -> Result<BTreeMap<u32, (i64, u32)>, R4G1ReplayError> {
    let emit = graph
        .section(R4G1Section::Emit)
        .ok_or(R4G1ReplayError::InvalidRootPrior)?;
    if emit.len() < 20 {
        return Err(R4G1ReplayError::InvalidRootPrior);
    }
    let count = usize::try_from(read_u32_export(emit, 4)?)
        .map_err(|_| R4G1ReplayError::FormatLimit("R4G1 root-prior count exceeds usize"))?;
    let end = 20usize
        .checked_add(
            count
                .checked_mul(8)
                .ok_or(R4G1ReplayError::InvalidRootPrior)?,
        )
        .ok_or(R4G1ReplayError::InvalidRootPrior)?;
    if end > emit.len() {
        return Err(R4G1ReplayError::InvalidRootPrior);
    }
    let mut scores = BTreeMap::new();
    let mut previous = None;
    let mut index = 0usize;
    while index < count {
        let offset = 20 + index * 8;
        let token = read_u32_export(emit, offset)?;
        if previous.is_some_and(|value| token <= value) {
            return Err(R4G1ReplayError::InvalidRootPrior);
        }
        previous = Some(token);
        let score = read_i32_export(emit, offset + 4)?;
        scores.insert(token, (i64::from(score), 1));
        index += 1;
    }
    Ok(scores)
}

fn read_u32_export(bytes: &[u8], offset: usize) -> Result<u32, R4G1ReplayError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(R4G1ReplayError::InvalidRootPrior)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_i32_export(bytes: &[u8], offset: usize) -> Result<i32, R4G1ReplayError> {
    Ok(i32::from_le_bytes(
        read_u32_export(bytes, offset)?.to_le_bytes(),
    ))
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

    let mut region_emit = Vec::new();
    let mut root_scores: BTreeMap<u32, (i64, u32)> = BTreeMap::new();
    let mut nodes = Vec::with_capacity(node_count);
    nodes.push(Node {
        child_start: 0,
        child_len: 0,
        forward_start: 0,
        forward_len: 0,
        emission_start: 0,
        emission_len: 0,
        prototype_word_start: 1,
        mask_word_start: 1 + node_count_u32 * SIGNATURE_WORDS as u32,
        radius: 0,
        depth: 0,
        path: [0; 8],
        path_len: 0,
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
        let exported_emission_start = region_emit.len();
        let exported_emission_start = u32::try_from(exported_emission_start)
            .map_err(|_| R4G1ExportError::FormatLimit("emission offset exceeds u32"))?;
        for entry in source_emissions.chunks_exact(EMISSION_RECORD_BYTES) {
            let token = read_u32(entry, 0)?;
            if token > i32::MAX as u32 {
                return Err(R4G1ExportError::TokenOutOfRange { token });
            }
            max_token = Some(max_token.map_or(token, |current: u32| current.max(token)));
            region_emit.extend_from_slice(entry);
            let score = read_i32(entry, 4)?;
            let aggregate = root_scores.entry(token).or_insert((0, 0));
            aggregate.0 = aggregate.0.saturating_add(i64::from(score));
            aggregate.1 = aggregate.1.saturating_add(1);
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
            child_start: 0,
            child_len: 0,
            forward_start: 0,
            forward_len: 0,
            emission_start: exported_emission_start,
            emission_len: u16::try_from(emission_len)
                .map_err(|_| R4G1ExportError::FormatLimit("emission count exceeds u16"))?,
            prototype_word_start,
            mask_word_start,
            radius: read_u16(source, 10)?,
            depth: source[8],
            path: read_path(source),
            path_len: source[9],
        });
    }

    let mut emit = vec![2, 0, 0, 0];
    let root_prefix = 16usize
        .checked_add(
            root_scores
                .len()
                .checked_mul(EMISSION_RECORD_BYTES)
                .ok_or(R4G1ExportError::FormatLimit("root prior size overflow"))?,
        )
        .ok_or(R4G1ExportError::FormatLimit("root prior size overflow"))?;
    let root_count = u32::try_from(root_scores.len())
        .map_err(|_| R4G1ExportError::FormatLimit("root prior count exceeds u32"))?;
    let total_count = root_scores
        .values()
        .fold(0u32, |total, (_, count)| total.saturating_add(*count));
    let root_floor = root_scores
        .values()
        .map(|(sum, count)| average_score(*sum, *count))
        .min()
        .unwrap_or(0);
    emit.extend_from_slice(&root_count.to_le_bytes());
    emit.extend_from_slice(&total_count.to_le_bytes());
    emit.extend_from_slice(&root_floor.to_le_bytes());
    emit.extend_from_slice(&0u32.to_le_bytes());
    for (token, (sum, count)) in &root_scores {
        emit.extend_from_slice(&token.to_le_bytes());
        emit.extend_from_slice(&average_score(*sum, *count).to_le_bytes());
    }
    debug_assert_eq!(emit.len(), 4 + root_prefix);
    for node in nodes.iter_mut().skip(1) {
        node.emission_start = node
            .emission_start
            .checked_add(
                u32::try_from(root_prefix)
                    .map_err(|_| R4G1ExportError::FormatLimit("root prior offset exceeds u32"))?,
            )
            .ok_or(R4G1ExportError::FormatLimit("emission offset overflow"))?;
    }
    emit.extend_from_slice(&region_emit);

    let mut edges = Vec::with_capacity(region_count);
    for node_index in 1..node_count {
        let node = nodes
            .get(node_index)
            .ok_or(R4G1ExportError::FormatLimit("node index is out of bounds"))?;
        let parent = if node.path_len <= 1 {
            0
        } else {
            let parent_len = node.path_len - 1;
            let mut found = None;
            let mut candidate = 1usize;
            while candidate < node_index {
                let possible = nodes
                    .get(candidate)
                    .ok_or(R4G1ExportError::FormatLimit("parent node is out of bounds"))?;
                if possible.path_len == parent_len
                    && same_path_prefix(node, possible, usize::from(parent_len))
                {
                    found = Some(candidate);
                    break;
                }
                candidate += 1;
            }
            found.ok_or(R4G1ExportError::FormatLimit(
                "region path has no deterministic parent",
            ))?
        };
        edges.push(Edge {
            src: u32::try_from(parent)
                .map_err(|_| R4G1ExportError::FormatLimit("edge source exceeds u32"))?,
            dst: u32::try_from(node_index)
                .map_err(|_| R4G1ExportError::FormatLimit("edge destination exceeds u32"))?,
            kind: 0,
            score_q: 0,
            reserved: 0,
        });
    }
    edges.extend(build_predictive_edges(
        &artifact.bytes,
        &view,
        region_offset,
        emission_offset,
        &root_scores,
    )?);
    edges.sort_by_key(|edge| (edge.src, edge.kind, edge.dst));
    let edge_count = u32::try_from(edges.len())
        .map_err(|_| R4G1ExportError::FormatLimit("edge count exceeds u32"))?;
    for (edge_index, edge) in edges.iter().enumerate() {
        if edge.kind != 0 {
            continue;
        }
        let source = nodes
            .get_mut(edge.src as usize)
            .ok_or(R4G1ExportError::FormatLimit(
                "edge source node is out of bounds",
            ))?;
        if source.child_len == 0 {
            source.child_start = u32::try_from(edge_index)
                .map_err(|_| R4G1ExportError::FormatLimit("child edge index exceeds u32"))?;
        }
        source.child_len = source
            .child_len
            .checked_add(1)
            .ok_or(R4G1ExportError::FormatLimit("child edge count exceeds u16"))?;
    }
    let mut reverse: Vec<u32> = (0..edge_count).collect();
    reverse.sort_by_key(|edge_id| {
        let edge = edges[*edge_id as usize];
        (edge.dst, edge.src, edge.kind, *edge_id)
    });
    for (reverse_index, edge_id) in reverse.iter().copied().enumerate() {
        let destination = edges[edge_id as usize].dst as usize;
        let node = nodes
            .get_mut(destination)
            .ok_or(R4G1ExportError::FormatLimit(
                "edge destination node is out of bounds",
            ))?;
        if node.forward_len == 0 {
            node.forward_start = u32::try_from(reverse_index)
                .map_err(|_| R4G1ExportError::FormatLimit("reverse index exceeds u32"))?;
        }
        node.forward_len = node
            .forward_len
            .checked_add(1)
            .ok_or(R4G1ExportError::FormatLimit(
                "forward edge count exceeds u16",
            ))?;
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
        node_section.extend_from_slice(&node.child_start.to_le_bytes());
        node_section.extend_from_slice(&node.child_len.to_le_bytes());
        node_section.extend_from_slice(&node.forward_start.to_le_bytes());
        node_section.extend_from_slice(&node.forward_len.to_le_bytes());
        node_section.extend_from_slice(&node.emission_start.to_le_bytes());
        node_section.extend_from_slice(&node.emission_len.to_le_bytes());
        node_section.extend_from_slice(&node.prototype_word_start.to_le_bytes());
        node_section.extend_from_slice(&node.mask_word_start.to_le_bytes());
        node_section.extend_from_slice(&node.radius.to_le_bytes());
        node_section.push(node.depth);
        node_section.push(0);
    }

    let mut edge_section = Vec::with_capacity(
        edges
            .len()
            .checked_mul(EDGE_RECORD_BYTES + REVERSE_INDEX_ENTRY_BYTES)
            .ok_or(R4G1ExportError::FormatLimit("EDGE section size overflow"))?,
    );
    for edge in &edges {
        edge_section.extend_from_slice(&edge.src.to_le_bytes());
        edge_section.extend_from_slice(&edge.dst.to_le_bytes());
        edge_section.extend_from_slice(&edge.score_q.to_le_bytes());
        edge_section.push(edge.kind);
        edge_section.push(0);
        edge_section.extend_from_slice(&edge.reserved.to_le_bytes());
    }
    for edge_id in reverse {
        edge_section.extend_from_slice(&edge_id.to_le_bytes());
    }

    let exct = build_exct(&artifact.bytes, &view, &root_scores, emission_offset)?;

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
    put_u32(&mut head, 200, edge_count);
    head[204] = depth_count;
    put_u16(&mut head, 212, SIGNATURE_BYTES);
    put_u32(&mut head, 220, vocab_size);

    let prov = b"uor-semantic structural-r4g1-v1\n";
    let sections = [
        (1u32, head.as_slice()),
        (2u32, &[0u8][..]),
        (3u32, node_section.as_slice()),
        (4u32, edge_section.as_slice()),
        (5u32, rout.as_slice()),
        (6u32, emit.as_slice()),
        (7u32, exct.as_slice()),
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
        edge_count,
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
    child_start: u32,
    child_len: u16,
    forward_start: u32,
    forward_len: u16,
    emission_start: u32,
    emission_len: u16,
    prototype_word_start: u32,
    mask_word_start: u32,
    radius: u16,
    depth: u8,
    path: [u16; 8],
    path_len: u8,
}

#[derive(Clone, Copy)]
struct Edge {
    src: u32,
    dst: u32,
    kind: u8,
    score_q: i32,
    reserved: u16,
}

fn read_path(bytes: &[u8]) -> [u16; 8] {
    let mut path = [0u16; 8];
    let mut index = 0usize;
    while index < path.len() {
        let offset = 84 + index * 2;
        path[index] = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        index += 1;
    }
    path
}

fn same_path_prefix(left: &Node, right: &Node, length: usize) -> bool {
    left.path[..length] == right.path[..length]
}

fn build_predictive_edges(
    artifact: &[u8],
    view: &ArtifactView<'_>,
    region_offset: usize,
    emission_offset: usize,
    root_scores: &BTreeMap<u32, (i64, u32)>,
) -> Result<Vec<Edge>, R4G1ExportError> {
    if view.exact_count() == 0 || view.region_count() == 0 {
        return Ok(Vec::new());
    }
    let exact_offset = usize::try_from(read_u64(artifact, EXACT_OFFSET_FIELD)?)
        .map_err(|_| R4G1ExportError::FormatLimit("exact offset exceeds usize"))?;
    let mut evidence = Vec::with_capacity(view.exact_count());
    let mut exact_index = 0usize;
    while exact_index < view.exact_count() {
        let record_start = exact_offset
            .checked_add(
                exact_index
                    .checked_mul(EXACT_RECORD_BYTES)
                    .ok_or(R4G1ExportError::FormatLimit("exact offset overflow"))?,
            )
            .ok_or(R4G1ExportError::FormatLimit("exact offset overflow"))?;
        let record_end = record_start
            .checked_add(EXACT_RECORD_BYTES)
            .ok_or(R4G1ExportError::FormatLimit("exact record end overflow"))?;
        let record = artifact
            .get(record_start..record_end)
            .ok_or(R4G1ExportError::FormatLimit(
                "exact record is out of bounds",
            ))?;
        let context_len = usize::from(read_u16(record, 8)?);
        let mut context = Vec::with_capacity(context_len);
        let mut token_index = 0usize;
        while token_index < context_len {
            context.push(read_u32(record, 48 + token_index * 4)?);
            token_index += 1;
        }
        let emission_len = usize::from(read_u16(record, 10)?);
        if emission_len != 0 {
            let emission_start = usize::try_from(read_u32(record, 12)?)
                .map_err(|_| R4G1ExportError::FormatLimit("exact emission index exceeds usize"))?;
            let emission_offset = emission_offset
                .checked_add(emission_start.checked_mul(EMISSION_RECORD_BYTES).ok_or(
                    R4G1ExportError::FormatLimit("exact emission offset overflow"),
                )?)
                .ok_or(R4G1ExportError::FormatLimit(
                    "exact emission offset overflow",
                ))?;
            let emission_end = emission_offset
                .checked_add(EMISSION_RECORD_BYTES)
                .ok_or(R4G1ExportError::FormatLimit("exact emission end overflow"))?;
            let emission =
                artifact
                    .get(emission_offset..emission_end)
                    .ok_or(R4G1ExportError::FormatLimit(
                        "exact emission record is out of bounds",
                    ))?;
            evidence.push(ExactEvidence {
                context: context.clone(),
                target: read_u32(emission, 0)?,
                score: read_i32(emission, 4)?,
                id: u32::try_from(exact_index + 1)
                    .map_err(|_| R4G1ExportError::FormatLimit("exact evidence ID exceeds u32"))?,
                node: nearest_region(artifact, region_offset, view.region_count(), &context)?,
            });
        }
        exact_index += 1;
    }

    let mut transitions: BTreeMap<(u32, u32), (ScoreAccumulator<MAX_PREDICTIVE_EVIDENCE>, u32)> =
        BTreeMap::new();
    for source in &evidence {
        let Some(source_node) = source.node else {
            continue;
        };
        let mut successor_index = 0usize;
        while successor_index < evidence.len() {
            let successor = &evidence[successor_index];
            if successor.context.len() == source.context.len() + 1
                && successor.context[..source.context.len()] == source.context[..]
                && successor.context[source.context.len()] == source.target
            {
                if let Some(destination_node) = successor.node {
                    let root_score = root_scores
                        .get(&source.target)
                        .map_or(0, |(sum, count)| average_score(*sum, *count));
                    let residual = source.score.saturating_sub(root_score);
                    let transition = transitions
                        .entry((source_node, destination_node))
                        .or_insert((ScoreAccumulator::new(), 0));
                    transition
                        .0
                        .accumulate(ResidualContribution {
                            kind: ResidualContributionKind::TokenEmission,
                            contribution_id: source.id,
                            raw_value: residual,
                        })
                        .map_err(|_| {
                            R4G1ExportError::FormatLimit(
                                "predictive edge evidence exceeds bounded capacity",
                            )
                        })?;
                    transition.1 = transition.1.saturating_add(1);
                }
                break;
            }
            successor_index += 1;
        }
    }

    let mut by_source: BTreeMap<u32, Vec<Edge>> = BTreeMap::new();
    for ((src, dst), (score, count)) in transitions {
        let average = average_score(i64::from(score.score()), count);
        by_source.entry(src).or_default().push(Edge {
            src,
            dst,
            kind: 2,
            score_q: average,
            reserved: 0,
        });
    }
    let mut output = Vec::new();
    for edges in by_source.values_mut() {
        edges.sort_by(|left, right| {
            right
                .score_q
                .cmp(&left.score_q)
                .then_with(|| left.dst.cmp(&right.dst))
        });
        edges.truncate(MAX_PREDICTIVE_EDGES_PER_SOURCE);
        output.extend(edges.iter().copied());
    }
    Ok(output)
}

struct ExactEvidence {
    context: Vec<u32>,
    target: u32,
    score: i32,
    id: u32,
    node: Option<u32>,
}

fn nearest_region(
    artifact: &[u8],
    region_offset: usize,
    region_count: usize,
    context: &[u32],
) -> Result<Option<u32>, R4G1ExportError> {
    let signature = uor_semantic::context_signature(context);
    let mut eligible = None;
    let mut fallback = None;
    for region_index in 0..region_count {
        let start = region_offset
            .checked_add(
                region_index
                    .checked_mul(REGION_RECORD_BYTES)
                    .ok_or(R4G1ExportError::FormatLimit("region offset overflow"))?,
            )
            .ok_or(R4G1ExportError::FormatLimit("region offset overflow"))?;
        let source = artifact.get(start..start + REGION_RECORD_BYTES).ok_or(
            R4G1ExportError::FormatLimit("region record is out of bounds"),
        )?;
        let prototype = read_words(source, 20)?;
        let mask = read_words(source, 52)?;
        let distance = uor_semantic::masked_hamming(&signature, &prototype, &mask);
        let key = (distance, region_index);
        if fallback.is_none_or(|current| key < current) {
            fallback = Some(key);
        }
        if distance <= u64::from(read_u16(source, 10)?)
            && eligible.is_none_or(|current| key < current)
        {
            eligible = Some(key);
        }
    }
    let selected = eligible.or(fallback).map(|(_, index)| index + 1);
    selected
        .map(|index| {
            u32::try_from(index)
                .map_err(|_| R4G1ExportError::FormatLimit("region node ID exceeds u32"))
        })
        .transpose()
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

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, R4G1ExportError> {
    Ok(i32::from_le_bytes(read_u32(bytes, offset)?.to_le_bytes()))
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

fn read_words(bytes: &[u8], offset: usize) -> Result<[u64; SIGNATURE_WORDS], R4G1ExportError> {
    Ok([
        read_u64(bytes, offset)?,
        read_u64(bytes, offset + 8)?,
        read_u64(bytes, offset + 16)?,
        read_u64(bytes, offset + 24)?,
    ])
}

fn average_score(sum: i64, count: u32) -> i32 {
    if count == 0 {
        return 0;
    }
    let average = sum / i64::from(count);
    i32::try_from(average).unwrap_or(if average.is_negative() {
        i32::MIN
    } else {
        i32::MAX
    })
}

fn build_exct(
    artifact: &[u8],
    view: &ArtifactView<'_>,
    root_scores: &BTreeMap<u32, (i64, u32)>,
    emission_offset: usize,
) -> Result<Vec<u8>, R4G1ExportError> {
    let exact_offset = usize::try_from(read_u64(artifact, EXACT_OFFSET_FIELD)?)
        .map_err(|_| R4G1ExportError::FormatLimit("exact offset exceeds usize"))?;
    let mut tables: BTreeMap<Vec<u8>, BTreeMap<u32, (i64, u32)>> = BTreeMap::new();
    let mut exact_index = 0usize;
    while exact_index < view.exact_count() {
        let record_offset = exact_offset
            .checked_add(
                exact_index
                    .checked_mul(EXACT_RECORD_BYTES)
                    .ok_or(R4G1ExportError::FormatLimit("exact offset overflow"))?,
            )
            .ok_or(R4G1ExportError::FormatLimit("exact offset overflow"))?;
        let record_end = record_offset
            .checked_add(EXACT_RECORD_BYTES)
            .ok_or(R4G1ExportError::FormatLimit("exact record end overflow"))?;
        let record =
            artifact
                .get(record_offset..record_end)
                .ok_or(R4G1ExportError::FormatLimit(
                    "exact record is out of bounds",
                ))?;
        let context_len = usize::from(read_u16(record, 8)?);
        let level = context_len.min(EXCT_LEVELS - 1);
        let key = record[16..16 + level].to_vec();
        let table = tables.entry(key).or_default();
        let emission_start = usize::try_from(read_u32(record, 12)?)
            .map_err(|_| R4G1ExportError::FormatLimit("exact emission index exceeds usize"))?;
        let emission_len = usize::from(read_u16(record, 10)?);
        let mut emission_index = 0usize;
        while emission_index < emission_len {
            let global_index =
                emission_start
                    .checked_add(emission_index)
                    .ok_or(R4G1ExportError::FormatLimit(
                        "exact emission index overflow",
                    ))?;
            let entry_offset = emission_offset
                .checked_add(global_index.checked_mul(EMISSION_RECORD_BYTES).ok_or(
                    R4G1ExportError::FormatLimit("exact emission offset overflow"),
                )?)
                .ok_or(R4G1ExportError::FormatLimit(
                    "exact emission offset overflow",
                ))?;
            let entry_end = entry_offset
                .checked_add(EMISSION_RECORD_BYTES)
                .ok_or(R4G1ExportError::FormatLimit("exact emission end overflow"))?;
            let entry =
                artifact
                    .get(entry_offset..entry_end)
                    .ok_or(R4G1ExportError::FormatLimit(
                        "exact emission record is out of bounds",
                    ))?;
            let token = read_u32(entry, 0)?;
            let score = i64::from(read_i32(entry, 4)?);
            let aggregate = table.entry(token).or_insert((0, 0));
            aggregate.0 = aggregate.0.saturating_add(score);
            aggregate.1 = aggregate.1.saturating_add(1);
            emission_index += 1;
        }
        exact_index += 1;
    }

    let mut output = vec![2, 0, 0, 0];
    output.extend_from_slice(b"RX1\0");
    output.push(EXCT_LEVELS as u8);
    output.extend_from_slice(&[0, 0, 0]);
    let mut level = 0usize;
    while level < EXCT_LEVELS {
        let key_count = tables.keys().filter(|key| key.len() == level).count();
        let key_count = u32::try_from(key_count)
            .map_err(|_| R4G1ExportError::FormatLimit("EXCT key count exceeds u32"))?;
        output.extend_from_slice(&key_count.to_le_bytes());
        for (key, table) in tables.iter().filter(|(key, _)| key.len() == level) {
            output.push(
                u8::try_from(key.len()).map_err(|_| {
                    R4G1ExportError::FormatLimit("EXCT route-code length exceeds u8")
                })?,
            );
            output.extend_from_slice(key);
            let total = table
                .values()
                .fold(0u32, |total, (_, count)| total.saturating_add(*count));
            let entry_count = u32::try_from(table.len())
                .map_err(|_| R4G1ExportError::FormatLimit("EXCT entry count exceeds u32"))?;
            output.extend_from_slice(&total.max(1).to_le_bytes());
            output.extend_from_slice(&entry_count.to_le_bytes());
            for (token, (sum, count)) in table {
                let root = root_scores.get(token).map_or(0, |(root_sum, root_count)| {
                    average_score(*root_sum, *root_count)
                });
                let residual = i64::from(average_score(*sum, *count)) - i64::from(root);
                let residual = i32::try_from(residual).unwrap_or(if residual.is_negative() {
                    i32::MIN
                } else {
                    i32::MAX
                });
                output.extend_from_slice(&token.to_le_bytes());
                output.extend_from_slice(&residual.to_le_bytes());
            }
        }
        level += 1;
    }
    Ok(output)
}

use crate::CompiledArtifact;

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

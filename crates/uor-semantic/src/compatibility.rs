//! Typed compatibility records for the R⁴ R4G1/TLA5 boundary.
//!
//! This module deliberately defines an adapter contract instead of importing
//! the target repository's wire-format or UOR dependencies. R4G1 and TLA5
//! identities are represented by the same opaque 256-bit values used by the
//! semantic artifact, while status, witness, and ranked-prediction records
//! remain fixed-capacity and allocation-free.

use core::fmt;

use crate::{
    ArtifactView, CodebookId, Depth, Prediction, PredictionSource, RegionId, ScoreQ, TokenScore,
};

/// Target artifact family represented by a compatibility manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityFormat {
    /// The scored graph container used by the target's deployed path.
    R4G1,
    /// The target's transformerless teacher artifact family.
    Tla5,
}

/// Target status carried by an R4G1/TLA5 prediction or policy decision.
///
/// `ExactContext`, `Graph`, and `Novel` correspond to the target scorer's
/// status space. `Contradictory` is retained as a reserved policy outcome and
/// is intentionally not convertible into a served semantic prediction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum R4Status {
    /// Exact-context evidence resolved the prediction.
    ExactContext,
    /// A semantic graph region resolved the prediction.
    Graph,
    /// No calibrated region resolved the prediction.
    Novel,
    /// Active evidence is contradictory and must not be silently served.
    Contradictory,
}

impl R4Status {
    /// Maps the semantic runtime's evidence source into the target status.
    pub const fn from_source(source: PredictionSource) -> Self {
        match source {
            PredictionSource::Exact => Self::ExactContext,
            PredictionSource::Graph => Self::Graph,
            PredictionSource::Novel => Self::Novel,
        }
    }

    /// Returns the served semantic evidence source, if the status is served.
    pub const fn source(self) -> Option<PredictionSource> {
        match self {
            Self::ExactContext => Some(PredictionSource::Exact),
            Self::Graph => Some(PredictionSource::Graph),
            Self::Novel | Self::Contradictory => None,
        }
    }
}

/// Identity field that failed compatibility validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityField {
    /// The compiled artifact or graph identity.
    Artifact,
    /// The source or teacher identity.
    Source,
    /// The tokenizer identity used to derive input signatures.
    Tokenizer,
}

/// Witness field whose value failed replay validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WitnessField {
    /// The artifact identity bound to the witness.
    ArtifactIdentity,
    /// The target resolution status.
    Status,
    /// The selected token.
    Token,
}

/// Failure while adapting or validating target-compatible records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityError {
    /// A manifest identity differs from the loaded semantic artifact.
    ManifestIdentityMismatch {
        /// Identity field that differed.
        field: IdentityField,
    },
    /// A witness does not bind to the manifest artifact identity.
    WitnessIdentityMismatch,
    /// A witness status does not match the prediction evidence source.
    WitnessStatusMismatch,
    /// A witness selected token differs from the prediction.
    WitnessTokenMismatch,
    /// A graph witness omitted the region that supplied the prediction.
    MissingRegionWitness,
    /// An exact or novel witness incorrectly claimed a graph region.
    UnexpectedRegionWitness,
    /// Contradictory status cannot be represented as a served prediction.
    ContradictoryPrediction,
    /// A fixed-capacity compatibility candidate list is full.
    PredictionCapacityExceeded,
    /// Target and semantic prediction statuses differ.
    PredictionStatusMismatch,
    /// Target and semantic widening state differ.
    PredictionWidenedMismatch,
    /// Target and semantic ranked candidates differ.
    PredictionCandidateMismatch,
    /// A witness was supplied for a prediction with no selected token.
    MissingPrediction,
}

impl fmt::Display for CompatibilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ManifestIdentityMismatch { field } => {
                let label = match field {
                    IdentityField::Artifact => "artifact",
                    IdentityField::Source => "source",
                    IdentityField::Tokenizer => "tokenizer",
                };
                write!(formatter, "compatibility {label} identity mismatch")
            }
            Self::WitnessIdentityMismatch => {
                formatter.write_str("compatibility witness artifact identity mismatch")
            }
            Self::WitnessStatusMismatch => {
                formatter.write_str("compatibility witness status mismatch")
            }
            Self::WitnessTokenMismatch => {
                formatter.write_str("compatibility witness selected token mismatch")
            }
            Self::MissingRegionWitness => {
                formatter.write_str("graph compatibility witness is missing its region")
            }
            Self::UnexpectedRegionWitness => {
                formatter.write_str("non-graph compatibility witness carries a region")
            }
            Self::ContradictoryPrediction => {
                formatter.write_str("contradictory compatibility status cannot be served")
            }
            Self::PredictionCapacityExceeded => {
                formatter.write_str("compatibility prediction capacity exceeded")
            }
            Self::PredictionStatusMismatch => {
                formatter.write_str("compatibility prediction status mismatch")
            }
            Self::PredictionWidenedMismatch => {
                formatter.write_str("compatibility prediction widening mismatch")
            }
            Self::PredictionCandidateMismatch => {
                formatter.write_str("compatibility prediction candidate mismatch")
            }
            Self::MissingPrediction => {
                formatter.write_str("compatibility witness has no selected prediction")
            }
        }
    }
}

impl core::error::Error for CompatibilityError {}

/// Typed identity mapping from an R4G1/TLA5 bundle to a semantic artifact.
///
/// `artifact_id` maps the target graph/artifact CID to the semantic
/// `CodebookId`; `source_id` maps the target source or teacher identity; and
/// `tokenizer_id` maps the tokenizer bytes used to derive signatures. The
/// optional store and certificate identities are retained for a later
/// resolver/certification adapter and are not fabricated by this crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibilityManifest {
    format: CompatibilityFormat,
    artifact_id: CodebookId,
    source_id: CodebookId,
    tokenizer_id: CodebookId,
    store_id: Option<CodebookId>,
    certificate_id: Option<CodebookId>,
}

impl CompatibilityManifest {
    /// Creates a manifest from the target identity fields.
    pub const fn new(
        format: CompatibilityFormat,
        artifact_id: CodebookId,
        source_id: CodebookId,
        tokenizer_id: CodebookId,
        store_id: Option<CodebookId>,
        certificate_id: Option<CodebookId>,
    ) -> Self {
        Self {
            format,
            artifact_id,
            source_id,
            tokenizer_id,
            store_id,
            certificate_id,
        }
    }

    /// Creates a manifest whose required identities are copied from an
    /// already validated semantic artifact.
    pub const fn from_artifact(format: CompatibilityFormat, artifact: &ArtifactView<'_>) -> Self {
        Self::new(
            format,
            artifact.codebook_id(),
            artifact.source_id(),
            artifact.tokenizer_id(),
            None,
            None,
        )
    }

    /// Returns the mapped target artifact family.
    pub const fn format(self) -> CompatibilityFormat {
        self.format
    }

    /// Returns the mapped graph or artifact identity.
    pub const fn artifact_id(self) -> CodebookId {
        self.artifact_id
    }

    /// Returns the mapped source or teacher identity.
    pub const fn source_id(self) -> CodebookId {
        self.source_id
    }

    /// Returns the mapped tokenizer identity.
    pub const fn tokenizer_id(self) -> CodebookId {
        self.tokenizer_id
    }

    /// Returns the optional resolver-backed store identity.
    pub const fn store_id(self) -> Option<CodebookId> {
        self.store_id
    }

    /// Returns the optional quality-certificate identity.
    pub const fn certificate_id(self) -> Option<CodebookId> {
        self.certificate_id
    }

    /// Validates all identities that the semantic artifact can expose.
    pub fn validate_artifact(self, artifact: &ArtifactView<'_>) -> Result<(), CompatibilityError> {
        if self.artifact_id != artifact.codebook_id() {
            return Err(CompatibilityError::ManifestIdentityMismatch {
                field: IdentityField::Artifact,
            });
        }
        if self.source_id != artifact.source_id() {
            return Err(CompatibilityError::ManifestIdentityMismatch {
                field: IdentityField::Source,
            });
        }
        if self.tokenizer_id != artifact.tokenizer_id() {
            return Err(CompatibilityError::ManifestIdentityMismatch {
                field: IdentityField::Tokenizer,
            });
        }
        Ok(())
    }
}

/// Fixed-capacity target-compatible ranked prediction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibilityPrediction<const CAPACITY: usize> {
    entries: [TokenScore; CAPACITY],
    len: usize,
    status: R4Status,
    widened: bool,
}

impl<const CAPACITY: usize> CompatibilityPrediction<CAPACITY> {
    /// Creates an empty target-compatible prediction for a semantic source.
    pub const fn new(source: PredictionSource, widened: bool) -> Self {
        Self::new_status(R4Status::from_source(source), widened)
    }

    /// Creates an empty prediction with an explicit target status.
    pub const fn new_status(status: R4Status, widened: bool) -> Self {
        Self {
            entries: [TokenScore::new(0, ScoreQ::from_raw(i32::MIN)); CAPACITY],
            len: 0,
            status,
            widened,
        }
    }

    /// Appends one candidate in canonical target order.
    pub fn push(&mut self, entry: TokenScore) -> Result<(), CompatibilityError> {
        if self.len == CAPACITY {
            return Err(CompatibilityError::PredictionCapacityExceeded);
        }
        self.entries[self.len] = entry;
        self.len += 1;
        Ok(())
    }

    /// Returns the initialized ranked candidates.
    pub fn as_slice(&self) -> &[TokenScore] {
        &self.entries[..self.len]
    }

    /// Returns the target status.
    pub const fn status(self) -> R4Status {
        self.status
    }

    /// Returns whether the target policy widened its probe.
    pub const fn widened(self) -> bool {
        self.widened
    }

    /// Compares target-compatible output with the semantic runtime output.
    pub fn matches_runtime<const SOURCE_CAPACITY: usize>(
        &self,
        prediction: &Prediction<SOURCE_CAPACITY>,
        source: PredictionSource,
        widened: bool,
    ) -> bool {
        if self.status != R4Status::from_source(source) {
            return false;
        }
        if self.widened != widened || self.as_slice().len() != prediction.as_slice().len() {
            return false;
        }
        self.as_slice() == prediction.as_slice()
    }
}

/// Replay-critical proof claim for one target-compatible prediction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibilityWitness {
    artifact_id: CodebookId,
    status: R4Status,
    region_id: Option<RegionId>,
    depth: Depth,
    token: u32,
    widened: bool,
}

impl CompatibilityWitness {
    /// Creates and structurally validates one compatibility witness.
    pub const fn new(
        artifact_id: CodebookId,
        status: R4Status,
        region_id: Option<RegionId>,
        depth: Depth,
        token: u32,
        widened: bool,
    ) -> Result<Self, CompatibilityError> {
        match status {
            R4Status::Graph if region_id.is_none() => {
                return Err(CompatibilityError::MissingRegionWitness);
            }
            R4Status::ExactContext | R4Status::Novel if region_id.is_some() => {
                return Err(CompatibilityError::UnexpectedRegionWitness);
            }
            R4Status::Contradictory => return Err(CompatibilityError::ContradictoryPrediction),
            R4Status::Graph | R4Status::ExactContext | R4Status::Novel => {}
        }
        Ok(Self {
            artifact_id,
            status,
            region_id,
            depth,
            token,
            widened,
        })
    }

    /// Returns the artifact identity bound to the witness.
    pub const fn artifact_id(self) -> CodebookId {
        self.artifact_id
    }

    /// Returns the target status claimed by the witness.
    pub const fn status(self) -> R4Status {
        self.status
    }

    /// Returns the selected graph region, when graph evidence supplied it.
    pub const fn region_id(self) -> Option<RegionId> {
        self.region_id
    }

    /// Returns the selected graph depth.
    pub const fn depth(self) -> Depth {
        self.depth
    }

    /// Returns the selected token.
    pub const fn token(self) -> u32 {
        self.token
    }

    /// Returns whether the target policy widened its probe.
    pub const fn widened(self) -> bool {
        self.widened
    }

    /// Verifies the witness against a manifest and semantic prediction.
    pub fn verify<const CAPACITY: usize>(
        self,
        manifest: &CompatibilityManifest,
        prediction: &Prediction<CAPACITY>,
        source: PredictionSource,
    ) -> Result<(), CompatibilityError> {
        if self.artifact_id != manifest.artifact_id {
            return Err(CompatibilityError::WitnessIdentityMismatch);
        }
        if self.status != R4Status::from_source(source) {
            return Err(CompatibilityError::WitnessStatusMismatch);
        }
        let Some(selected) = prediction.first() else {
            return Err(CompatibilityError::MissingPrediction);
        };
        if self.token != selected.token() {
            return Err(CompatibilityError::WitnessTokenMismatch);
        }
        Ok(())
    }
}

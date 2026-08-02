//! Teacher-parity measurement and threshold enforcement.

use core::fmt;

use uor_semantic::{
    ArtifactPredictScratch, ArtifactView, ExactPolicy, GenerationState, Prediction,
    PredictionSource, generate_greedy_into,
};

use crate::observation::{ObservationCorpus, ObservedEmission};
use crate::rollout::RolloutCorpus;

/// Minimum measured parity required by a requested certification profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParityThresholds {
    /// Required exact-context top-1 parity, in basis points.
    pub exact_top1_basis_points: u16,
    /// Required graph-only top-1 parity, in basis points.
    pub graph_top1_basis_points: u16,
    /// Required graph-only coverage, in basis points.
    pub graph_coverage_basis_points: u16,
    /// Required overall graph top-K recall, in basis points.
    pub graph_top_k_recall_basis_points: u16,
}

impl ParityThresholds {
    /// Strict exact parity with a caller-selected graph floor.
    pub const fn exact_with_graph_floor(graph_top1_basis_points: u16) -> Self {
        Self {
            exact_top1_basis_points: 10_000,
            graph_top1_basis_points,
            graph_coverage_basis_points: 0,
            graph_top_k_recall_basis_points: 0,
        }
    }
}

/// Measured source-model parity on one pinned corpus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParityReport {
    /// Evaluated observations.
    pub samples: usize,
    /// Exact-context top-1 matches.
    pub exact_top1_matches: usize,
    /// Graph-only top-1 matches.
    pub graph_top1_matches: usize,
    /// Exact-context coverage count.
    pub exact_covered: usize,
    /// Graph coverage count.
    pub graph_covered: usize,
    /// Graph top-K recall hits among covered samples.
    pub graph_top_k_recall_covered_hits: usize,
    /// Total graph top-K tokens among covered samples.
    pub graph_top_k_recall_covered_total: usize,
    /// Exact samples whose complete captured top-K order was reproduced.
    pub exact_top_k_order_matches: usize,
    /// Exact samples with an exact-context record available for top-K comparison.
    pub exact_top_k_order_samples: usize,
    /// Graph predictions containing captured top-K tokens.
    pub graph_top_k_recall_hits: usize,
    /// Total captured top-K tokens evaluated in the graph lane.
    pub graph_top_k_recall_total: usize,
    /// Total indexed candidate regions scanned by graph predictions.
    pub graph_regions_scanned: usize,
}

/// Measured autoregressive rollout parity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RolloutParityReport {
    /// Evaluated rollouts.
    pub samples: usize,
    /// Rollouts whose generated token sequence matched exactly.
    pub sequence_exact_matches: usize,
    /// Rollouts whose EOS position, including no-EOS, matched.
    pub eos_position_matches: usize,
    /// Rollouts whose teacher sequence included EOS.
    pub eos_samples: usize,
}

impl RolloutParityReport {
    /// Exact sequence agreement in basis points.
    pub fn sequence_exact_basis_points(self) -> u16 {
        basis_points(self.sequence_exact_matches, self.samples)
    }

    /// EOS-position agreement in basis points over all rollouts.
    pub fn eos_position_basis_points(self) -> u16 {
        basis_points(self.eos_position_matches, self.samples)
    }

    /// Renders a stable JSON report.
    pub fn to_json(self) -> String {
        format!(
            concat!(
                "{{\n",
                "  \"samples\": {},\n",
                "  \"sequence_exact_matches\": {},\n",
                "  \"eos_position_matches\": {},\n",
                "  \"eos_samples\": {},\n",
                "  \"sequence_exact_basis_points\": {},\n",
                "  \"eos_position_basis_points\": {}\n",
                "}}"
            ),
            self.samples,
            self.sequence_exact_matches,
            self.eos_position_matches,
            self.eos_samples,
            self.sequence_exact_basis_points(),
            self.eos_position_basis_points(),
        )
    }
}

impl ParityReport {
    /// Exact-context top-1 parity in basis points.
    pub fn exact_top1_basis_points(self) -> u16 {
        basis_points(self.exact_top1_matches, self.samples)
    }

    /// Graph-only top-1 parity in basis points.
    pub fn graph_top1_basis_points(self) -> u16 {
        basis_points(self.graph_top1_matches, self.samples)
    }

    /// Graph coverage in basis points.
    pub fn graph_coverage_basis_points(self) -> u16 {
        basis_points(self.graph_covered, self.samples)
    }

    /// Exact top-K token-order agreement in basis points over covered samples.
    pub fn exact_top_k_order_basis_points(self) -> u16 {
        basis_points(
            self.exact_top_k_order_matches,
            self.exact_top_k_order_samples,
        )
    }

    /// Graph top-K token recall in basis points.
    pub fn graph_top_k_recall_basis_points(self) -> u16 {
        basis_points(self.graph_top_k_recall_hits, self.graph_top_k_recall_total)
    }

    /// Graph top-K recall in basis points among covered samples only.
    pub fn graph_top_k_recall_on_covered_basis_points(self) -> u16 {
        basis_points(
            self.graph_top_k_recall_covered_hits,
            self.graph_top_k_recall_covered_total,
        )
    }

    /// Returns whether both requested thresholds pass.
    pub fn passes(self, thresholds: ParityThresholds) -> bool {
        self.exact_top1_basis_points() >= thresholds.exact_top1_basis_points
            && self.graph_top1_basis_points() >= thresholds.graph_top1_basis_points
            && self.graph_coverage_basis_points() >= thresholds.graph_coverage_basis_points
            && self.graph_top_k_recall_basis_points() >= thresholds.graph_top_k_recall_basis_points
    }

    /// Renders a stable JSON report.
    pub fn to_json(self) -> String {
        format!(
            concat!(
                "{{\n",
                "  \"samples\": {},\n",
                "  \"exact_top1_matches\": {},\n",
                "  \"graph_top1_matches\": {},\n",
                "  \"exact_covered\": {},\n",
                "  \"graph_covered\": {},\n",
                "  \"graph_top_k_recall_covered_hits\": {},\n",
                "  \"graph_top_k_recall_covered_total\": {},\n",
                "  \"exact_top_k_order_matches\": {},\n",
                "  \"exact_top_k_order_samples\": {},\n",
                "  \"graph_top_k_recall_hits\": {},\n",
                "  \"graph_top_k_recall_total\": {},\n",
                "  \"graph_regions_scanned\": {},\n",
                "  \"exact_top1_basis_points\": {},\n",
                "  \"exact_top_k_order_basis_points\": {},\n",
                "  \"graph_coverage_basis_points\": {},\n",
                "  \"graph_top1_basis_points\": {},\n",
                "  \"graph_top_k_recall_basis_points\": {},\n",
                "  \"graph_top_k_recall_on_covered_basis_points\": {}\n",
                "}}"
            ),
            self.samples,
            self.exact_top1_matches,
            self.graph_top1_matches,
            self.exact_covered,
            self.graph_covered,
            self.graph_top_k_recall_covered_hits,
            self.graph_top_k_recall_covered_total,
            self.exact_top_k_order_matches,
            self.exact_top_k_order_samples,
            self.graph_top_k_recall_hits,
            self.graph_top_k_recall_total,
            self.graph_regions_scanned,
            self.exact_top1_basis_points(),
            self.exact_top_k_order_basis_points(),
            self.graph_coverage_basis_points(),
            self.graph_top1_basis_points(),
            self.graph_top_k_recall_basis_points(),
            self.graph_top_k_recall_on_covered_basis_points(),
        )
    }
}

/// Failure to evaluate parity.
#[derive(Debug)]
pub enum ParityError {
    /// Artifact validation or prediction failed.
    Artifact(uor_semantic::ArtifactError),
    /// The evaluation corpus contains no observations.
    EmptyCorpus,
    /// Bounded generation failed during rollout evaluation.
    Generation(uor_semantic::GenerationError),
    /// The capture identity does not match the compiled artifact identity.
    IdentityMismatch(&'static str),
}

impl fmt::Display for ParityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Artifact(error) => {
                write!(formatter, "artifact parity evaluation failed: {error}")
            }
            Self::EmptyCorpus => formatter.write_str("parity corpus is empty"),
            Self::Generation(error) => write!(formatter, "rollout generation failed: {error}"),
            Self::IdentityMismatch(field) => {
                write!(formatter, "capture identity mismatch: {field}")
            }
        }
    }
}

impl std::error::Error for ParityError {}

impl From<uor_semantic::ArtifactError> for ParityError {
    fn from(error: uor_semantic::ArtifactError) -> Self {
        Self::Artifact(error)
    }
}

impl From<uor_semantic::GenerationError> for ParityError {
    fn from(error: uor_semantic::GenerationError) -> Self {
        Self::Generation(error)
    }
}

/// Evaluates exact and graph-only top-1 parity against captured teacher argmax.
pub fn evaluate(
    artifact_bytes: &[u8],
    corpus: &ObservationCorpus,
) -> Result<ParityReport, ParityError> {
    if corpus.observations.is_empty() {
        return Err(ParityError::EmptyCorpus);
    }
    let artifact = ArtifactView::parse(artifact_bytes)?;
    validate_observation_identity(&artifact, &corpus.metadata)?;
    let mut scratch = ArtifactPredictScratch::<32>::new();
    let mut prediction = Prediction::<64>::new();
    let mut report = ParityReport {
        samples: corpus.observations.len(),
        exact_top1_matches: 0,
        graph_top1_matches: 0,
        exact_covered: 0,
        graph_covered: 0,
        graph_top_k_recall_covered_hits: 0,
        graph_top_k_recall_covered_total: 0,
        exact_top_k_order_matches: 0,
        exact_top_k_order_samples: 0,
        graph_top_k_recall_hits: 0,
        graph_top_k_recall_total: 0,
        graph_regions_scanned: 0,
    };

    for observation in &corpus.observations {
        let exact = artifact.predict(
            &observation.context,
            ExactPolicy::PreferExact,
            &mut scratch,
            &mut prediction,
        )?;
        if exact.source() == PredictionSource::Exact {
            report.exact_covered = report.exact_covered.saturating_add(1);
            report.exact_top_k_order_samples = report.exact_top_k_order_samples.saturating_add(1);
            if top_k_order_matches(&prediction, &observation.emissions) {
                report.exact_top_k_order_matches =
                    report.exact_top_k_order_matches.saturating_add(1);
            }
        }
        if prediction.first().map(|entry| entry.token()) == Some(observation.target) {
            report.exact_top1_matches = report.exact_top1_matches.saturating_add(1);
        }

        let graph = artifact.predict(
            &observation.context,
            ExactPolicy::GraphOnly,
            &mut scratch,
            &mut prediction,
        )?;
        if graph.source() == PredictionSource::Graph {
            report.graph_covered = report.graph_covered.saturating_add(1);
        }
        let (hits, total) = top_k_recall(&prediction, &observation.emissions);
        report.graph_top_k_recall_hits = report.graph_top_k_recall_hits.saturating_add(hits);
        report.graph_top_k_recall_total = report.graph_top_k_recall_total.saturating_add(total);
        if graph.source() == PredictionSource::Graph {
            report.graph_top_k_recall_covered_hits =
                report.graph_top_k_recall_covered_hits.saturating_add(hits);
            report.graph_top_k_recall_covered_total = report
                .graph_top_k_recall_covered_total
                .saturating_add(total);
        }
        report.graph_regions_scanned = report
            .graph_regions_scanned
            .saturating_add(graph.regions_scanned());
        if prediction.first().map(|entry| entry.token()) == Some(observation.target) {
            report.graph_top1_matches = report.graph_top1_matches.saturating_add(1);
        }
    }
    Ok(report)
}

/// Evaluates only the graph lane, forcibly bypassing exact-context evidence.
///
/// The returned exact counters remain zero by construction. This is intended
/// for held-out observations, where exact lookup must not improve the measured
/// result.
pub fn evaluate_graph_only(
    artifact_bytes: &[u8],
    corpus: &ObservationCorpus,
) -> Result<ParityReport, ParityError> {
    if corpus.observations.is_empty() {
        return Err(ParityError::EmptyCorpus);
    }
    let artifact = ArtifactView::parse(artifact_bytes)?;
    validate_observation_identity(&artifact, &corpus.metadata)?;
    let mut scratch = ArtifactPredictScratch::<32>::new();
    let mut prediction = Prediction::<64>::new();
    let mut report = ParityReport {
        samples: corpus.observations.len(),
        exact_top1_matches: 0,
        graph_top1_matches: 0,
        exact_covered: 0,
        graph_covered: 0,
        graph_top_k_recall_covered_hits: 0,
        graph_top_k_recall_covered_total: 0,
        exact_top_k_order_matches: 0,
        exact_top_k_order_samples: 0,
        graph_top_k_recall_hits: 0,
        graph_top_k_recall_total: 0,
        graph_regions_scanned: 0,
    };

    for observation in &corpus.observations {
        let graph = artifact.predict(
            &observation.context,
            ExactPolicy::GraphOnly,
            &mut scratch,
            &mut prediction,
        )?;
        if graph.source() == PredictionSource::Graph {
            report.graph_covered = report.graph_covered.saturating_add(1);
        }
        let (hits, total) = top_k_recall(&prediction, &observation.emissions);
        report.graph_top_k_recall_hits = report.graph_top_k_recall_hits.saturating_add(hits);
        report.graph_top_k_recall_total = report.graph_top_k_recall_total.saturating_add(total);
        if graph.source() == PredictionSource::Graph {
            report.graph_top_k_recall_covered_hits =
                report.graph_top_k_recall_covered_hits.saturating_add(hits);
            report.graph_top_k_recall_covered_total = report
                .graph_top_k_recall_covered_total
                .saturating_add(total);
        }
        report.graph_regions_scanned = report
            .graph_regions_scanned
            .saturating_add(graph.regions_scanned());
        if prediction.first().map(|entry| entry.token()) == Some(observation.target) {
            report.graph_top1_matches = report.graph_top1_matches.saturating_add(1);
        }
    }
    Ok(report)
}

/// Evaluates bounded greedy generation against autoregressive teacher rollouts.
pub fn evaluate_rollouts(
    artifact_bytes: &[u8],
    corpus: &RolloutCorpus,
) -> Result<RolloutParityReport, ParityError> {
    if corpus.rollouts.is_empty() {
        return Err(ParityError::EmptyCorpus);
    }
    let artifact = ArtifactView::parse(artifact_bytes)?;
    if artifact.tokenizer_id().as_bytes() != &corpus.metadata.tokenizer_sha256 {
        return Err(ParityError::IdentityMismatch("tokenizer_sha256"));
    }
    if artifact.chat_template_id().as_bytes() != &corpus.metadata.chat_template_sha256 {
        return Err(ParityError::IdentityMismatch("chat_template_sha256"));
    }
    if artifact.special_tokens_id().as_bytes() != &corpus.metadata.special_tokens_sha256 {
        return Err(ParityError::IdentityMismatch("special_tokens_sha256"));
    }
    if artifact.eos_token() != corpus.metadata.eos_token {
        return Err(ParityError::IdentityMismatch("eos_token"));
    }
    let mut scratch = ArtifactPredictScratch::<32>::new();
    let mut prediction = Prediction::<64>::new();
    let mut report = RolloutParityReport {
        samples: corpus.rollouts.len(),
        sequence_exact_matches: 0,
        eos_position_matches: 0,
        eos_samples: 0,
    };

    for rollout in &corpus.rollouts {
        let mut state = GenerationState::<32>::new()?;
        state.seed(&rollout.prompt);
        let mut output = vec![0u32; rollout.generated.len()];
        let summary = generate_greedy_into(
            &artifact,
            &mut state,
            &mut output,
            &mut scratch,
            &mut prediction,
        )?;
        output.truncate(summary.written());
        if output == rollout.generated {
            report.sequence_exact_matches = report.sequence_exact_matches.saturating_add(1);
        }
        if rollout.eos_position.is_some() {
            report.eos_samples = report.eos_samples.saturating_add(1);
        }
        if eos_position(&output, corpus.metadata.eos_token) == rollout.eos_position {
            report.eos_position_matches = report.eos_position_matches.saturating_add(1);
        }
    }
    Ok(report)
}

fn eos_position(tokens: &[u32], eos_token: u32) -> Option<usize> {
    tokens.iter().position(|token| *token == eos_token)
}

fn top_k_order_matches<const CAPACITY: usize>(
    prediction: &Prediction<CAPACITY>,
    expected: &[ObservedEmission],
) -> bool {
    expected.iter().enumerate().all(|(index, emission)| {
        prediction
            .as_slice()
            .get(index)
            .is_some_and(|actual| actual.token() == emission.token)
    })
}

fn top_k_recall<const CAPACITY: usize>(
    prediction: &Prediction<CAPACITY>,
    expected: &[ObservedEmission],
) -> (usize, usize) {
    let predicted = prediction.as_slice();
    let mut hits = 0usize;
    for emission in expected {
        if predicted
            .iter()
            .take(expected.len())
            .any(|actual| actual.token() == emission.token)
        {
            hits = hits.saturating_add(1);
        }
    }
    (hits, expected.len())
}

fn validate_observation_identity(
    artifact: &ArtifactView<'_>,
    metadata: &crate::observation::ObservationMetadata,
) -> Result<(), ParityError> {
    if artifact.tokenizer_id().as_bytes() != &metadata.tokenizer_sha256 {
        return Err(ParityError::IdentityMismatch("tokenizer_sha256"));
    }
    if artifact.chat_template_id().as_bytes() != &metadata.chat_template_sha256 {
        return Err(ParityError::IdentityMismatch("chat_template_sha256"));
    }
    if artifact.special_tokens_id().as_bytes() != &metadata.special_tokens_sha256 {
        return Err(ParityError::IdentityMismatch("special_tokens_sha256"));
    }
    if artifact.eos_token() != metadata.eos_token {
        return Err(ParityError::IdentityMismatch("eos_token"));
    }
    Ok(())
}

fn basis_points(matches: usize, samples: usize) -> u16 {
    if samples == 0 {
        return 0;
    }
    let value = matches.saturating_mul(10_000) / samples;
    u16::try_from(value.min(10_000)).unwrap_or(10_000)
}

#[cfg(test)]
mod tests {
    use super::{ParityThresholds, evaluate, evaluate_graph_only, evaluate_rollouts};
    use crate::compiler::{CompilerConfig, compile};
    use crate::observation::ObservationCorpus;

    const CORPUS: &str = concat!(
        "UOROBS1\n",
        "model=fixture/model\n",
        "revision=0123456789abcdef\n",
        "source_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "max_context=4\n",
        "top_k=2\n",
        "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000002\n",
        "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000003\n",
        "eos_token=2\n",
        "--\n",
        "O|1,2|3|3:100,4:90\n",
        "O|1,2,3|4|4:110,3:80\n",
    );

    #[test]
    fn exact_lane_reaches_full_fixture_parity() {
        let corpus = ObservationCorpus::parse(CORPUS).expect("fixture parses");
        let artifact = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
        let report = evaluate(&artifact.bytes, &corpus).expect("parity evaluates");
        assert_eq!(report.exact_top1_basis_points(), 10_000);
        assert!(report.passes(ParityThresholds::exact_with_graph_floor(0)));
        assert!(!report.passes(ParityThresholds::exact_with_graph_floor(10_001)));
    }

    #[test]
    fn graph_only_lane_forces_exact_coverage_to_zero() {
        let corpus = ObservationCorpus::parse(CORPUS).expect("fixture parses");
        let artifact = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
        let report = evaluate_graph_only(&artifact.bytes, &corpus).expect("parity evaluates");
        assert_eq!(report.exact_covered, 0);
        assert_eq!(report.exact_top1_matches, 0);
    }

    #[test]
    fn rollout_lane_reports_sequence_and_eos_agreement() {
        let observations = ObservationCorpus::parse(concat!(
            "UOROBS1\n",
            "model=fixture/model\n",
            "revision=0123456789abcdef\n",
            "source_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
            "max_context=4\n",
            "top_k=2\n",
            "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
            "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000002\n",
            "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000003\n",
            "eos_token=2\n",
            "--\n",
            "O|1,2|3|3:100,4:90\n",
            "O|2,3|2|2:100,4:90\n",
        ))
        .expect("observations parse");
        let artifact = compile(&observations, CompilerConfig::accuracy()).expect("compile");
        let rollouts = crate::RolloutCorpus::parse(concat!(
            "UORROL1\n",
            "model=fixture/model\n",
            "revision=0123456789abcdef0123456789abcdef01234567\n",
            "source_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
            "max_context=4\n",
            "max_tokens=2\n",
            "eos_token=2\n",
            "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
            "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000002\n",
            "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000003\n",
            "--\n",
            "R|1,2|3,2|1\n",
        ))
        .expect("rollouts parse");
        let report = evaluate_rollouts(&artifact.bytes, &rollouts).expect("rollouts evaluate");
        assert_eq!(report.sequence_exact_basis_points(), 10_000);
        assert_eq!(report.eos_position_basis_points(), 10_000);
    }
}

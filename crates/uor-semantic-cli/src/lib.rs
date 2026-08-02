//! Command-line model lifecycle and runtime test surface.

#![deny(missing_docs)]

use core::fmt;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use uor_semantic::{
    ArtifactPredictScratch, ArtifactView, ExactPolicy, GenerationState, Prediction, R4G1Graph,
    R4G1Predictions, R4G1RouteCandidates, context_signature, generate_greedy_into,
};
use uor_semantic_compiler::{
    CompiledArtifact, CompilerConfig, MAX_ROLLOUT_TOKENS, ObservationCorpus, ParityReport,
    ParityThresholds, R4G1Export, R4G1ExportError, R4G1ReplayError, RolloutCorpus,
    compile as compile_observations, evaluate, evaluate_graph_only, evaluate_rollouts,
    export_r4g1 as export_compiled_r4g1, replay_r4g1,
};

const CAPTURE_SCRIPT: &str = include_str!("../../../scripts/capture_hf.py");
const TOKENIZER_SCRIPT: &str = include_str!("../../../scripts/tokenizer_bridge.py");

/// CLI failure.
#[derive(Debug)]
pub enum CliError {
    /// Required command or option is missing.
    Usage(String),
    /// A numeric or token-list argument is invalid.
    InvalidValue(String),
    /// Filesystem operation failed.
    Io(std::io::Error),
    /// Observation parsing failed.
    Observation(uor_semantic_compiler::ObservationError),
    /// Compilation failed.
    Compile(uor_semantic_compiler::CompileError),
    /// Parity evaluation failed.
    Parity(uor_semantic_compiler::ParityError),
    /// Rollout parsing failed.
    Rollout(uor_semantic_compiler::RolloutError),
    /// Artifact access failed.
    Artifact(uor_semantic::ArtifactError),
    /// Generation state or execution failed.
    Generation(uor_semantic::GenerationError),
    /// Structural R4G1 export failed.
    R4G1Export(R4G1ExportError),
    /// R4G1 replay verification failed.
    R4G1Replay(R4G1ReplayError),
    /// R4G1 graph prediction failed.
    R4G1Prediction(uor_semantic::R4G1Error),
    /// An external command exited unsuccessfully.
    ProcessFailed {
        /// Program that failed.
        program: String,
        /// Process exit code when available.
        code: Option<i32>,
        /// Relevant stderr emitted by the process.
        detail: Option<String>,
    },
    /// Local model-build inputs failed preflight.
    Preflight(PreflightError),
    /// Measured parity did not reach requested thresholds.
    ParityThresholdFailed {
        /// Measured exact top-1 basis points.
        exact: u16,
        /// Required exact top-1 basis points.
        required_exact: u16,
        /// Measured graph top-1 basis points.
        graph: u16,
        /// Required graph top-1 basis points.
        required_graph: u16,
        /// Measured graph coverage basis points.
        graph_coverage: u16,
        /// Required graph coverage basis points.
        required_graph_coverage: u16,
        /// Measured graph top-K recall basis points.
        graph_top_k: u16,
        /// Required graph top-K recall basis points.
        required_graph_top_k: u16,
    },
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) | Self::InvalidValue(message) => formatter.write_str(message),
            Self::Io(error) => write!(formatter, "filesystem operation failed: {error}"),
            Self::Observation(error) => write!(formatter, "observation error: {error}"),
            Self::Compile(error) => write!(formatter, "compile error: {error}"),
            Self::Parity(error) => write!(formatter, "parity error: {error}"),
            Self::Rollout(error) => write!(formatter, "rollout error: {error}"),
            Self::Artifact(error) => write!(formatter, "artifact error: {error}"),
            Self::Generation(error) => write!(formatter, "generation error: {error}"),
            Self::R4G1Export(error) => write!(formatter, "R4G1 export error: {error}"),
            Self::R4G1Replay(error) => write!(formatter, "R4G1 replay error: {error}"),
            Self::R4G1Prediction(error) => write!(formatter, "R4G1 prediction error: {error}"),
            Self::ProcessFailed {
                program,
                code,
                detail,
            } => {
                write!(
                    formatter,
                    "{program} exited unsuccessfully with code {code:?}"
                )?;
                if let Some(detail) = detail {
                    write!(formatter, ": {detail}")?;
                }
                Ok(())
            }
            Self::Preflight(error) => write!(formatter, "preflight failed: {error}"),
            Self::ParityThresholdFailed {
                exact,
                required_exact,
                graph,
                required_graph,
                graph_coverage,
                required_graph_coverage,
                graph_top_k,
                required_graph_top_k,
            } => write!(
                formatter,
                "parity thresholds failed: exact {exact}/{required_exact} bps, graph {graph}/{required_graph} bps, coverage {graph_coverage}/{required_graph_coverage} bps, graph_top_k {graph_top_k}/{required_graph_top_k} bps"
            ),
        }
    }
}

impl std::error::Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<uor_semantic_compiler::ObservationError> for CliError {
    fn from(error: uor_semantic_compiler::ObservationError) -> Self {
        Self::Observation(error)
    }
}

impl From<uor_semantic_compiler::CompileError> for CliError {
    fn from(error: uor_semantic_compiler::CompileError) -> Self {
        Self::Compile(error)
    }
}

impl From<uor_semantic_compiler::ParityError> for CliError {
    fn from(error: uor_semantic_compiler::ParityError) -> Self {
        Self::Parity(error)
    }
}

impl From<uor_semantic_compiler::RolloutError> for CliError {
    fn from(error: uor_semantic_compiler::RolloutError) -> Self {
        Self::Rollout(error)
    }
}

impl From<uor_semantic::ArtifactError> for CliError {
    fn from(error: uor_semantic::ArtifactError) -> Self {
        Self::Artifact(error)
    }
}

impl From<uor_semantic::GenerationError> for CliError {
    fn from(error: uor_semantic::GenerationError) -> Self {
        Self::Generation(error)
    }
}

impl From<R4G1ExportError> for CliError {
    fn from(error: R4G1ExportError) -> Self {
        Self::R4G1Export(error)
    }
}

impl From<R4G1ReplayError> for CliError {
    fn from(error: R4G1ReplayError) -> Self {
        Self::R4G1Replay(error)
    }
}

impl From<uor_semantic::R4G1Error> for CliError {
    fn from(error: uor_semantic::R4G1Error) -> Self {
        Self::R4G1Prediction(error)
    }
}

/// Actionable local failure found before a model download or capture.
#[derive(Debug)]
pub enum PreflightError {
    /// The requested corpus path does not exist.
    MissingCorpus {
        /// Absolute path that the child process would have consumed.
        path: PathBuf,
    },
    /// The corpus path exists but is not a regular file.
    CorpusNotRegular {
        /// Resolved corpus path.
        path: PathBuf,
    },
    /// The corpus could not be read.
    CorpusUnreadable {
        /// Resolved corpus path.
        path: PathBuf,
        /// Operating-system reason.
        reason: String,
    },
    /// The corpus is not valid UTF-8.
    CorpusInvalidUtf8 {
        /// Resolved corpus path.
        path: PathBuf,
    },
    /// The corpus has no non-blank sample after the documented filtering policy.
    CorpusEmpty {
        /// Resolved corpus path.
        path: PathBuf,
    },
    /// The local model source is not a complete Hugging Face-style snapshot.
    SourceInvalid {
        /// Resolved source directory.
        path: PathBuf,
        /// Missing or malformed source requirement.
        reason: String,
    },
    /// The work directory is not a usable writable directory.
    WorkDirectory {
        /// Resolved work directory path.
        path: PathBuf,
        /// Operating-system reason.
        reason: String,
    },
    /// Existing work would be overwritten implicitly.
    ExistingWork {
        /// Existing path that blocks a safe build.
        path: PathBuf,
    },
    /// A required executable or Python package is unavailable.
    ExternalRequirement {
        /// Executable being checked.
        program: String,
        /// Human-readable corrective action.
        requirement: String,
        /// Optional command detail.
        detail: Option<String>,
    },
}

/// Typed request for downloading an immutable Hugging Face source snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadRequest {
    /// Hugging Face owner/repository.
    pub repository: String,
    /// Immutable 40-character commit revision.
    pub revision: String,
    /// Destination directory for the source snapshot.
    pub output: PathBuf,
}

/// Typed request for compiling captured observations into a runtime artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileRequest {
    /// Line-oriented `.uorobs` observation corpus.
    pub observations: PathBuf,
    /// Destination `.uors` artifact.
    pub output: PathBuf,
    /// Deterministic compiler configuration.
    pub config: CompilerConfig,
}

/// Capture settings for a typed source compilation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureOptions {
    /// Python executable hosting the Hugging Face bridge.
    pub python: String,
    /// Number of teacher candidates retained per observation.
    pub top_k: usize,
    /// Maximum captured context length.
    pub max_context: usize,
    /// Optional observation sample bound.
    pub max_samples: usize,
    /// Optional bounded rollout length; zero disables rollout capture.
    pub rollout_tokens: usize,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            python: "python3".to_owned(),
            top_k: 64,
            max_context: 32,
            max_samples: usize::MAX,
            rollout_tokens: 0,
        }
    }
}

/// Typed request for compiling a verified local teacher source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceCompileRequest {
    /// Verified local Hugging Face-style source directory.
    pub source_dir: PathBuf,
    /// Model identifier recorded in observation provenance.
    pub model_id: String,
    /// Immutable source revision recorded in observation provenance.
    pub revision: String,
    /// UTF-8 construction corpus used by the teacher bridge.
    pub corpus: PathBuf,
    /// Output directory for observations, artifact, and parity.
    pub work_dir: PathBuf,
    /// Teacher capture settings.
    pub capture: CaptureOptions,
    /// Deterministic artifact compiler settings.
    pub compiler: CompilerConfig,
    /// Required parity floors for the construction corpus.
    pub parity: ParityThresholds,
}

/// Files and measurements produced by [`compile_source`].
#[derive(Clone, Debug)]
pub struct SourceCompileResult {
    /// Resolved resumable output directory.
    pub work_dir: PathBuf,
    /// Captured observation file.
    pub observations: PathBuf,
    /// Compiled runtime artifact.
    pub artifact: PathBuf,
    /// JSON parity report.
    pub parity_report: PathBuf,
    /// In-memory validated artifact summary.
    pub compiled: CompiledArtifact,
    /// Measured construction-corpus parity.
    pub parity: ParityReport,
    /// Optional bounded autoregressive rollout capture.
    pub rollouts: Option<PathBuf>,
}

impl fmt::Display for PreflightError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCorpus { path } => write!(
                formatter,
                "corpus file does not exist at {}; create it with cp data/construction.example.txt {} or pass a different --corpus path",
                path.display(),
                path.display()
            ),
            Self::CorpusNotRegular { path } => write!(
                formatter,
                "corpus path {} is not a regular file; pass --corpus a readable UTF-8 text file",
                path.display()
            ),
            Self::CorpusUnreadable { path, reason } => write!(
                formatter,
                "corpus file {} is not readable ({reason}); check permissions or pass another --corpus path",
                path.display()
            ),
            Self::CorpusInvalidUtf8 { path } => write!(
                formatter,
                "corpus file {} is not valid UTF-8; rewrite it as UTF-8 text with one sample per line",
                path.display()
            ),
            Self::CorpusEmpty { path } => write!(
                formatter,
                "corpus file {} has no non-blank samples; add one non-empty line per sample or copy data/construction.example.txt",
                path.display()
            ),
            Self::SourceInvalid { path, reason } => write!(
                formatter,
                "model source {} is invalid ({reason}); provide config.json, tokenizer.json, and at least one .safetensors file",
                path.display()
            ),
            Self::WorkDirectory { path, reason } => write!(
                formatter,
                "work directory {} is not safely writable ({reason}); choose another --work-dir",
                path.display()
            ),
            Self::ExistingWork { path } => write!(
                formatter,
                "existing work at {} is not a verified pinned snapshot or output; choose a new --work-dir so existing files are not overwritten",
                path.display()
            ),
            Self::ExternalRequirement {
                program,
                requirement,
                detail,
            } => {
                write!(formatter, "{requirement} (checked {program})")?;
                if let Some(detail) = detail {
                    write!(formatter, ": {detail}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for PreflightError {}

/// Runs the CLI from the process argument vector.
pub fn run_env() -> Result<(), CliError> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    run(&arguments)
}

/// Runs one parsed command.
pub fn run(arguments: &[String]) -> Result<(), CliError> {
    let Some(command) = arguments.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };
    match command {
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        "self-test" => command_self_test(),
        "artifact" => command_artifact(&arguments[1..]),
        "model" => command_model(&arguments[1..]),
        "download" => command_download(&arguments[1..]),
        "compile" => command_compile(&arguments[1..]),
        _ => Err(CliError::Usage(format!(
            "unknown command `{command}`; run `uor-semantic help`"
        ))),
    }
}

fn print_help() {
    println!(concat!(
        "uor-semantic — compile and test allocation-free semantic artifacts\n\n",
        "Commands:\n",
        "  self-test\n",
        "  artifact inspect <artifact>\n",
        "  artifact replay <artifact> --r4g1 <container>\n",
        "  artifact predict-r4g1 <container> --tokens <csv> [--top-k <n>]\n",
        "  artifact predict <artifact> --tokens <csv> [--graph-only]\n",
        "  artifact generate <artifact> --tokens <csv> --max-tokens <n>\n",
        "  model download <repo> --revision <40-hex> --output <dir>\n",
        "  model capture <source-dir> --corpus <txt> --output <uorobs>\n",
        "      --model-id <repo> --revision <40-hex> [--top-k <n>]\n",
        "      [--max-context <n>] [--max-samples <n>] [--python <path>]\n",
        "      [--rollout-tokens <n>] [--rollout-output <uoroll>]\n",
        "  model compile <uorobs> --output <artifact> [--r4g1-output <container>] [--regions <n>]\n",
        "      [--iterations <n>] [--overlap-margin <n>] [--region-top-k <n>]\n",
        "      [--without-exact]\n",
        "  model parity <artifact> --observations <uorobs>\n",
        "      [--min-exact-bps <0..10000>] [--min-graph-bps <0..10000>]\n",
        "      [--min-graph-coverage-bps <0..10000>] [--min-graph-top-k-bps <0..10000>]\n",
        "      [--report <json>] [--graph-only]\n",
        "  model parity <artifact> --rollouts <uoroll> [--report <json>]\n",
        "  model build <repo> --revision <40-hex> --corpus <txt> --work-dir <dir>\n",
        "      [--held-out-corpus <txt>] [capture and compiler options]\n",
        "  model generate <artifact> --source <model-dir> --prompt <text>\n",
        "      --max-tokens <n> [--python <path>]\n",
        "\nCompatibility aliases matching the R4 lifecycle surface:\n",
        "  download --repository <repo> --revision <40-hex> --name <name> [--output <dir>]\n",
        "  compile <uorobs> --output <artifact> [--r4g1-output <container>] [compiler options]\n"
    ));
}

fn command_download(arguments: &[String]) -> Result<(), CliError> {
    let repository = required_option(arguments, "--repository")?;
    let revision = required_option(arguments, "--revision")?;
    let name = required_option(arguments, "--name")?;
    let output = option_value(arguments, "--output")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".uor-models/sources").join(safe_name(name)));
    let path = download(&DownloadRequest {
        repository: repository.to_owned(),
        revision: revision.to_owned(),
        output,
    })?;
    println!("{}", path.display());
    Ok(())
}

fn command_compile(arguments: &[String]) -> Result<(), CliError> {
    let observations = PathBuf::from(required_positional(arguments, 0, "observation file")?);
    let output = PathBuf::from(required_option(arguments, "--output")?);
    let r4g1_output = option_value(arguments, "--r4g1-output").map(PathBuf::from);
    let request = CompileRequest {
        observations,
        output,
        config: compiler_config(arguments)?,
    };
    let artifact = compile(&request)?;
    if let Some(r4g1_output) = r4g1_output.as_deref() {
        let exported = export_r4g1(&artifact, r4g1_output)?;
        println!("r4g1: {}", r4g1_output.display());
        println!("r4g1_artifact_cid: {}", hex(&exported.artifact_cid));
    }
    println!("codebook_id: {}", hex(artifact.codebook_id.as_bytes()));
    println!("observations: {}", artifact.observations);
    println!("bytes: {}", artifact.bytes.len());
    Ok(())
}

fn safe_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn command_artifact(arguments: &[String]) -> Result<(), CliError> {
    let Some(command) = arguments.first().map(String::as_str) else {
        return Err(CliError::Usage(
            "artifact subcommand is required".to_owned(),
        ));
    };
    match command {
        "inspect" => {
            let path = required_positional(arguments, 1, "artifact path")?;
            artifact_inspect(Path::new(path))
        }
        "replay" => {
            let artifact = required_positional(arguments, 1, "artifact path")?;
            let r4g1 = required_option(arguments, "--r4g1")?;
            artifact_replay(Path::new(artifact), Path::new(r4g1))
        }
        "predict-r4g1" => {
            let path = required_positional(arguments, 1, "R4G1 container path")?;
            let tokens = parse_tokens(required_option(arguments, "--tokens")?)?;
            let top_k = parse_optional_usize(arguments, "--top-k", 8)?;
            if top_k == 0 || top_k > 64 {
                return Err(CliError::InvalidValue(
                    "top-k must be between 1 and 64".to_owned(),
                ));
            }
            artifact_predict_r4g1(Path::new(path), &tokens, top_k)
        }
        "predict" => {
            let path = required_positional(arguments, 1, "artifact path")?;
            let tokens = parse_tokens(required_option(arguments, "--tokens")?)?;
            let policy = if flag_present(arguments, "--graph-only") {
                ExactPolicy::GraphOnly
            } else {
                ExactPolicy::PreferExact
            };
            artifact_predict(Path::new(path), &tokens, policy)
        }
        "generate" => {
            let path = required_positional(arguments, 1, "artifact path")?;
            let tokens = parse_tokens(required_option(arguments, "--tokens")?)?;
            let max_tokens =
                parse_usize(required_option(arguments, "--max-tokens")?, "max-tokens")?;
            artifact_generate(Path::new(path), &tokens, max_tokens).map(|_| ())
        }
        _ => Err(CliError::Usage(format!(
            "unknown artifact subcommand `{command}`"
        ))),
    }
}

fn command_model(arguments: &[String]) -> Result<(), CliError> {
    let Some(command) = arguments.first().map(String::as_str) else {
        return Err(CliError::Usage("model subcommand is required".to_owned()));
    };
    match command {
        "download" => {
            let repository = required_positional(arguments, 1, "Hugging Face repository")?;
            let revision = required_option(arguments, "--revision")?;
            let output = PathBuf::from(required_option(arguments, "--output")?);
            model_download(repository, revision, &output)
        }
        "capture" => {
            let source = PathBuf::from(required_positional(arguments, 1, "source directory")?);
            let corpus = PathBuf::from(required_option(arguments, "--corpus")?);
            let output = PathBuf::from(required_option(arguments, "--output")?);
            let model_id = required_option(arguments, "--model-id")?;
            let revision = required_option(arguments, "--revision")?;
            let options = CaptureOptions::from_arguments(arguments)?;
            let rollout_output = option_value(arguments, "--rollout-output").map(PathBuf::from);
            model_capture(
                &source,
                &corpus,
                &output,
                model_id,
                revision,
                options,
                rollout_output.as_deref(),
            )
        }
        "compile" => {
            let observations =
                PathBuf::from(required_positional(arguments, 1, "observation file")?);
            let output = PathBuf::from(required_option(arguments, "--output")?);
            let r4g1_output = option_value(arguments, "--r4g1-output").map(PathBuf::from);
            let config = compiler_config(arguments)?;
            model_compile(&observations, &output, r4g1_output.as_deref(), config)
        }
        "parity" => {
            let artifact = PathBuf::from(required_positional(arguments, 1, "artifact file")?);
            let report = option_value(arguments, "--report").map(PathBuf::from);
            if let Some(rollouts) = option_value(arguments, "--rollouts") {
                if option_value(arguments, "--observations").is_some() {
                    return Err(CliError::Usage(
                        "--rollouts and --observations are mutually exclusive".to_owned(),
                    ));
                }
                model_rollout_parity(&artifact, Path::new(rollouts), report.as_deref())
            } else {
                let observations = PathBuf::from(required_option(arguments, "--observations")?);
                let thresholds = parity_thresholds(arguments)?;
                model_parity(
                    &artifact,
                    &observations,
                    thresholds,
                    report.as_deref(),
                    flag_present(arguments, "--graph-only"),
                )
            }
        }
        "build" => model_build(arguments),
        "generate" => model_generate(arguments),
        _ => Err(CliError::Usage(format!(
            "unknown model subcommand `{command}`"
        ))),
    }
}

fn artifact_inspect(path: &Path) -> Result<(), CliError> {
    let bytes = std::fs::read(path)?;
    let artifact = ArtifactView::parse(&bytes)?;
    println!("artifact: {}", path.display());
    println!("codebook_id: {}", hex(artifact.codebook_id().as_bytes()));
    println!("source_id: {}", hex(artifact.source_id().as_bytes()));
    println!("tokenizer_id: {}", hex(artifact.tokenizer_id().as_bytes()));
    println!(
        "chat_template_id: {}",
        hex(artifact.chat_template_id().as_bytes())
    );
    println!(
        "special_tokens_id: {}",
        hex(artifact.special_tokens_id().as_bytes())
    );
    println!("eos_token: {}", artifact.eos_token());
    println!("exact_records: {}", artifact.exact_count());
    println!("regions: {}", artifact.region_count());
    println!("emissions: {}", artifact.emission_count());
    println!("bytes: {}", bytes.len());
    Ok(())
}

fn artifact_replay(artifact_path: &Path, r4g1_path: &Path) -> Result<(), CliError> {
    let artifact = std::fs::read(artifact_path)?;
    let r4g1 = std::fs::read(r4g1_path)?;
    let report = replay_r4g1(&artifact, &r4g1)?;
    println!("artifact: {}", artifact_path.display());
    println!("r4g1: {}", r4g1_path.display());
    println!("expected_transitions: {}", report.expected_transitions);
    println!(
        "emitted_predictive_edges: {}",
        report.emitted_predictive_edges
    );
    println!("matched_transitions: {}", report.matched_transitions);
    println!("score_matches: {}", report.score_matches);
    println!(
        "score_agreement_basis_points: {}",
        report.score_agreement_basis_points()
    );
    println!("complete: {}", report.is_complete());
    Ok(())
}

fn artifact_predict_r4g1(path: &Path, tokens: &[u32], top_k: usize) -> Result<(), CliError> {
    let bytes = std::fs::read(path)?;
    let graph = R4G1Graph::parse(&bytes)?;
    let signature = context_signature(tokens);
    let mut candidates = R4G1RouteCandidates::<64>::new();
    let mut predictions = R4G1Predictions::<64>::new();
    graph.predict_top_k(&signature, &mut candidates, &mut predictions)?;
    println!("routed_candidates: {}", candidates.as_slice().len());
    println!("truncated_predictions: {}", predictions.truncated());
    for entry in predictions.as_slice().iter().take(top_k) {
        println!("{}\t{}", entry.token, entry.score_q);
    }
    Ok(())
}

fn artifact_predict(path: &Path, tokens: &[u32], policy: ExactPolicy) -> Result<(), CliError> {
    let bytes = std::fs::read(path)?;
    let artifact = ArtifactView::parse(&bytes)?;
    let mut scratch = ArtifactPredictScratch::<32>::new();
    let mut prediction = Prediction::<64>::new();
    let summary = artifact.predict(tokens, policy, &mut scratch, &mut prediction)?;
    println!("source: {:?}", summary.source());
    println!("exact_context_len: {}", summary.exact_context_len());
    println!("regions_matched: {}", summary.regions_matched());
    println!("regions_retained: {}", summary.regions_retained());
    println!("regions_scanned: {}", summary.regions_scanned());
    for entry in prediction.as_slice() {
        println!("{}\t{}", entry.token(), entry.score().raw());
    }
    Ok(())
}

fn artifact_generate(path: &Path, prompt: &[u32], max_tokens: usize) -> Result<Vec<u32>, CliError> {
    let bytes = std::fs::read(path)?;
    let artifact = ArtifactView::parse(&bytes)?;
    let mut state = GenerationState::<32>::new()?;
    state.seed(prompt);
    let mut output = vec![0u32; max_tokens];
    let mut scratch = ArtifactPredictScratch::<32>::new();
    let mut prediction = Prediction::<64>::new();
    let summary = generate_greedy_into(
        &artifact,
        &mut state,
        &mut output,
        &mut scratch,
        &mut prediction,
    )?;
    output.truncate(summary.written());
    println!("stop: {:?}", summary.stop());
    println!("exact_steps: {}", summary.exact_steps());
    println!("graph_steps: {}", summary.graph_steps());
    println!(
        "tokens: {}",
        output
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    Ok(output)
}

/// Downloads an immutable source snapshot using the official `hf` CLI.
pub fn download(request: &DownloadRequest) -> Result<PathBuf, CliError> {
    validate_revision(&request.revision)?;
    if verified_pinned_snapshot(&request.output, &request.revision) {
        println!(
            "reusing verified pinned source snapshot: {}",
            request.output.display()
        );
        return Ok(request.output.clone());
    }
    std::fs::create_dir_all(&request.output)?;
    let arguments = download_arguments(&request.repository, &request.revision, &request.output);
    run_process("hf", &arguments)?;
    Ok(request.output.clone())
}

/// Compatibility name matching the R4 model-source API.
pub fn download_source(request: &DownloadRequest) -> Result<PathBuf, CliError> {
    download(request)
}

fn model_download(repository: &str, revision: &str, output: &Path) -> Result<(), CliError> {
    download(&DownloadRequest {
        repository: repository.to_owned(),
        revision: revision.to_owned(),
        output: output.to_owned(),
    })
    .map(|_| ())
}

/// Constructs the exact official `hf download` argument vector.
pub fn download_arguments(repository: &str, revision: &str, output: &Path) -> Vec<String> {
    vec![
        "download".to_owned(),
        repository.to_owned(),
        "--revision".to_owned(),
        revision.to_owned(),
        "--local-dir".to_owned(),
        output.display().to_string(),
    ]
}

impl CaptureOptions {
    fn from_arguments(arguments: &[String]) -> Result<Self, CliError> {
        let rollout_tokens = parse_optional_usize(arguments, "--rollout-tokens", 0)?;
        if rollout_tokens > MAX_ROLLOUT_TOKENS {
            return Err(CliError::InvalidValue(format!(
                "rollout-tokens must be between 1 and {MAX_ROLLOUT_TOKENS}"
            )));
        }
        Ok(Self {
            python: option_value(arguments, "--python")
                .unwrap_or("python3")
                .to_owned(),
            top_k: parse_optional_usize(arguments, "--top-k", 64)?,
            max_context: parse_optional_usize(arguments, "--max-context", 32)?,
            max_samples: parse_optional_usize(arguments, "--max-samples", usize::MAX)?,
            rollout_tokens,
        })
    }
}

fn validate_capture_options(
    options: &CaptureOptions,
    has_rollout_output: bool,
) -> Result<(), CliError> {
    if options.top_k == 0 {
        return Err(CliError::InvalidValue("top-k must be non-zero".to_owned()));
    }
    if options.max_context == 0 || options.max_context > uor_semantic::MAX_CONTEXT_TOKENS {
        return Err(CliError::InvalidValue(format!(
            "max-context must be between 1 and {}",
            uor_semantic::MAX_CONTEXT_TOKENS
        )));
    }
    if options.max_samples == 0 {
        return Err(CliError::InvalidValue(
            "max-samples must be non-zero".to_owned(),
        ));
    }
    if options.rollout_tokens > MAX_ROLLOUT_TOKENS {
        return Err(CliError::InvalidValue(format!(
            "rollout-tokens must be between 1 and {MAX_ROLLOUT_TOKENS}"
        )));
    }
    if options.rollout_tokens > 0 && !has_rollout_output {
        return Err(CliError::InvalidValue(
            "rollout-output is required when rollout-tokens is non-zero".to_owned(),
        ));
    }
    if options.rollout_tokens == 0 && has_rollout_output {
        return Err(CliError::InvalidValue(
            "rollout-tokens is required when rollout-output is provided".to_owned(),
        ));
    }
    Ok(())
}

fn model_capture(
    source: &Path,
    corpus: &Path,
    output: &Path,
    model_id: &str,
    revision: &str,
    options: CaptureOptions,
    rollout_output: Option<&Path>,
) -> Result<(), CliError> {
    validate_revision(revision)?;
    validate_capture_options(&options, rollout_output.is_some())?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(rollout_output) = rollout_output
        && let Some(parent) = rollout_output.parent()
    {
        std::fs::create_dir_all(parent)?;
    }
    let script = materialize_script("capture_hf.py", CAPTURE_SCRIPT)?;
    let mut arguments = vec![
        script.display().to_string(),
        "--source".to_owned(),
        source.display().to_string(),
        "--corpus".to_owned(),
        corpus.display().to_string(),
        "--output".to_owned(),
        output.display().to_string(),
        "--model-id".to_owned(),
        model_id.to_owned(),
        "--revision".to_owned(),
        revision.to_owned(),
        "--top-k".to_owned(),
        options.top_k.to_string(),
        "--max-context".to_owned(),
        options.max_context.to_string(),
    ];
    if options.max_samples != usize::MAX {
        arguments.push("--max-samples".to_owned());
        arguments.push(options.max_samples.to_string());
    }
    if let Some(rollout_output) = rollout_output {
        arguments.push("--rollout-output".to_owned());
        arguments.push(rollout_output.display().to_string());
        arguments.push("--rollout-tokens".to_owned());
        arguments.push(options.rollout_tokens.to_string());
    }
    run_process_capture(&options.python, &arguments)
}

fn model_compile(
    observations: &Path,
    output: &Path,
    r4g1_output: Option<&Path>,
    config: CompilerConfig,
) -> Result<(), CliError> {
    let artifact = compile(&CompileRequest {
        observations: observations.to_owned(),
        output: output.to_owned(),
        config,
    })?;
    let exported = r4g1_output
        .map(|path| export_r4g1(&artifact, path))
        .transpose()?;
    println!("artifact: {}", output.display());
    println!("codebook_id: {}", hex(artifact.codebook_id.as_bytes()));
    println!("observations: {}", artifact.observations);
    println!("exact_records: {}", artifact.exact_records);
    println!("regions: {}", artifact.regions);
    println!("region_memberships: {}", artifact.memberships);
    println!("emissions: {}", artifact.emissions);
    println!("bytes: {}", artifact.bytes.len());
    if let (Some(path), Some(exported)) = (r4g1_output, exported) {
        println!("r4g1: {}", path.display());
        println!("r4g1_artifact_cid: {}", hex(&exported.artifact_cid));
    }
    Ok(())
}

/// Compiles a typed observation-corpus request and writes its validated artifact.
pub fn compile(request: &CompileRequest) -> Result<CompiledArtifact, CliError> {
    let corpus = ObservationCorpus::read(&request.observations)?;
    let artifact = compile_observations(&corpus, request.config)?;
    if let Some(parent) = request.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&request.output, &artifact.bytes)?;
    Ok(artifact)
}

/// Exports a validated compiled artifact to a structural R4G1 container.
pub fn export_r4g1(artifact: &CompiledArtifact, output: &Path) -> Result<R4G1Export, CliError> {
    let exported = export_compiled_r4g1(artifact)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, &exported.bytes)?;
    Ok(exported)
}

/// Captures a verified local teacher source, compiles its observations, and
/// writes a parity report in one typed operation.
///
/// This is the typed source-side lifecycle bridge corresponding to the R4
/// compile entry point. It currently emits this repository's validated `.uors`
/// artifact; scored R4G1 emission remains a separate compatibility stage.
pub fn compile_source(request: &SourceCompileRequest) -> Result<SourceCompileResult, CliError> {
    validate_revision(&request.revision)?;
    validate_capture_options(&request.capture, request.capture.rollout_tokens > 0)?;
    validate_compiler_config(request.compiler)?;

    let source = preflight_source_directory(&request.source_dir)?;
    let corpus = preflight_corpus(&request.corpus)?;
    let work_dir = preflight_work_directory(&request.work_dir)?;
    let observations = work_dir.join("observations.uorobs");
    let artifact = work_dir.join("model.uors");
    let parity_report = work_dir.join("parity.json");
    let rollouts = (request.capture.rollout_tokens > 0).then(|| work_dir.join("rollouts.uorroll"));

    for path in [&observations, &artifact, &parity_report] {
        if path.exists() {
            return Err(CliError::Preflight(PreflightError::ExistingWork {
                path: path.clone(),
            }));
        }
    }
    if let Some(path) = &rollouts
        && path.exists()
    {
        return Err(CliError::Preflight(PreflightError::ExistingWork {
            path: path.clone(),
        }));
    }

    check_python_bridge(&request.capture.python)?;
    model_capture(
        &source,
        &corpus,
        &observations,
        &request.model_id,
        &request.revision,
        request.capture.clone(),
        rollouts.as_deref(),
    )?;
    let compiled = compile(&CompileRequest {
        observations: observations.clone(),
        output: artifact.clone(),
        config: request.compiler,
    })?;
    let captured = ObservationCorpus::read(&observations)?;
    let parity = evaluate(&compiled.bytes, &captured)?;
    std::fs::write(&parity_report, parity.to_json())?;
    if !parity.passes(request.parity) {
        return Err(CliError::ParityThresholdFailed {
            exact: parity.exact_top1_basis_points(),
            required_exact: request.parity.exact_top1_basis_points,
            graph: parity.graph_top1_basis_points(),
            required_graph: request.parity.graph_top1_basis_points,
            graph_coverage: parity.graph_coverage_basis_points(),
            required_graph_coverage: request.parity.graph_coverage_basis_points,
            graph_top_k: parity.graph_top_k_recall_basis_points(),
            required_graph_top_k: request.parity.graph_top_k_recall_basis_points,
        });
    }

    Ok(SourceCompileResult {
        work_dir,
        observations,
        artifact,
        parity_report,
        compiled,
        parity,
        rollouts,
    })
}

fn model_parity(
    artifact: &Path,
    observations: &Path,
    thresholds: ParityThresholds,
    report_path: Option<&Path>,
    graph_only: bool,
) -> Result<(), CliError> {
    let bytes = std::fs::read(artifact)?;
    let corpus = ObservationCorpus::read(observations)?;
    let report = if graph_only {
        evaluate_graph_only(&bytes, &corpus)?
    } else {
        evaluate(&bytes, &corpus)?
    };
    let json = report.to_json();
    if let Some(path) = report_path {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &json)?;
    }
    println!("{json}");
    if !report.passes(thresholds) {
        return Err(CliError::ParityThresholdFailed {
            exact: report.exact_top1_basis_points(),
            required_exact: thresholds.exact_top1_basis_points,
            graph: report.graph_top1_basis_points(),
            required_graph: thresholds.graph_top1_basis_points,
            graph_coverage: report.graph_coverage_basis_points(),
            required_graph_coverage: thresholds.graph_coverage_basis_points,
            graph_top_k: report.graph_top_k_recall_basis_points(),
            required_graph_top_k: thresholds.graph_top_k_recall_basis_points,
        });
    }
    Ok(())
}

fn model_rollout_parity(
    artifact: &Path,
    rollouts: &Path,
    report_path: Option<&Path>,
) -> Result<(), CliError> {
    let bytes = std::fs::read(artifact)?;
    let corpus = RolloutCorpus::read(rollouts)?;
    let report = evaluate_rollouts(&bytes, &corpus)?;
    let json = report.to_json();
    if let Some(path) = report_path {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &json)?;
    }
    println!("{json}");
    Ok(())
}

struct ModelBuildPlan {
    repository: String,
    revision: String,
    corpus: PathBuf,
    work_dir: PathBuf,
    source: PathBuf,
    observations: PathBuf,
    artifact: PathBuf,
    parity: PathBuf,
    held_out_corpus: Option<PathBuf>,
    held_out_observations: Option<PathBuf>,
    held_out_parity: Option<PathBuf>,
    rollouts: Option<PathBuf>,
    held_out_rollouts: Option<PathBuf>,
    rollout_parity: Option<PathBuf>,
    capture: CaptureOptions,
    config: CompilerConfig,
    thresholds: ParityThresholds,
}

fn model_build(arguments: &[String]) -> Result<(), CliError> {
    model_build_with_downloader(arguments, true, |repository, revision, source| {
        model_download(repository, revision, source)
    })
}

fn model_build_with_downloader<F>(
    arguments: &[String],
    check_external: bool,
    mut downloader: F,
) -> Result<(), CliError>
where
    F: FnMut(&str, &str, &Path) -> Result<(), CliError>,
{
    let plan = preflight_model_build(arguments, check_external)?;
    downloader(&plan.repository, &plan.revision, &plan.source)?;
    model_capture(
        &plan.source,
        &plan.corpus,
        &plan.observations,
        &plan.repository,
        &plan.revision,
        plan.capture.clone(),
        plan.rollouts.as_deref(),
    )?;
    if let (Some(corpus), Some(observations)) = (&plan.held_out_corpus, &plan.held_out_observations)
    {
        model_capture(
            &plan.source,
            corpus,
            observations,
            &plan.repository,
            &plan.revision,
            plan.capture,
            plan.held_out_rollouts.as_deref(),
        )?;
    }
    model_compile(&plan.observations, &plan.artifact, None, plan.config)?;
    model_parity(
        &plan.artifact,
        &plan.observations,
        plan.thresholds,
        Some(&plan.parity),
        false,
    )?;
    if let (Some(observations), Some(report)) = (&plan.held_out_observations, &plan.held_out_parity)
    {
        model_parity(
            &plan.artifact,
            observations,
            ParityThresholds {
                exact_top1_basis_points: 0,
                graph_top1_basis_points: plan.thresholds.graph_top1_basis_points,
                graph_coverage_basis_points: plan.thresholds.graph_coverage_basis_points,
                graph_top_k_recall_basis_points: plan.thresholds.graph_top_k_recall_basis_points,
            },
            Some(report),
            true,
        )?;
        println!("held_out_observations: {}", observations.display());
        println!("held_out_parity: {}", report.display());
    }
    let rollout_input = plan.held_out_rollouts.as_ref().or(plan.rollouts.as_ref());
    if let (Some(rollouts), Some(report)) = (rollout_input, &plan.rollout_parity) {
        model_rollout_parity(&plan.artifact, rollouts, Some(report))?;
        println!("rollout_parity: {}", report.display());
    }
    println!("build_dir: {}", plan.work_dir.display());
    Ok(())
}

fn preflight_model_build(
    arguments: &[String],
    check_external: bool,
) -> Result<ModelBuildPlan, CliError> {
    let repository = required_positional(arguments, 1, "Hugging Face repository")?;
    let revision = required_option(arguments, "--revision")?;
    let corpus_arg = PathBuf::from(required_option(arguments, "--corpus")?);
    let work_dir_arg = PathBuf::from(required_option(arguments, "--work-dir")?);
    validate_revision(revision)?;

    let capture = CaptureOptions::from_arguments(arguments)?;
    let config = compiler_config(arguments)?;
    validate_compiler_config(config)?;
    let thresholds = parity_thresholds(arguments)?;
    let corpus = preflight_corpus(&corpus_arg)?;
    let held_out_corpus = option_value(arguments, "--held-out-corpus")
        .map(PathBuf::from)
        .map(|path| preflight_corpus(&path))
        .transpose()?;
    if held_out_corpus.as_ref() == Some(&corpus) {
        return Err(CliError::InvalidValue(
            "held-out-corpus must resolve to a different file than --corpus".to_owned(),
        ));
    }
    let work_dir = preflight_work_directory(&work_dir_arg)?;
    let source = work_dir.join("source");
    let observations = work_dir.join("observations.uorobs");
    let artifact = work_dir.join("model.uors");
    let parity = work_dir.join("parity.json");
    let held_out_observations = held_out_corpus
        .as_ref()
        .map(|_| work_dir.join("held-out-observations.uorobs"));
    let held_out_parity = held_out_corpus
        .as_ref()
        .map(|_| work_dir.join("held-out-parity.json"));
    let rollouts = (capture.rollout_tokens > 0).then(|| work_dir.join("rollouts.uorroll"));
    let held_out_rollouts = held_out_corpus
        .as_ref()
        .filter(|_| capture.rollout_tokens > 0)
        .map(|_| work_dir.join("held-out-rollouts.uorroll"));
    let rollout_parity = rollouts
        .as_ref()
        .map(|_| work_dir.join("rollout-parity.json"));

    if source.exists() && !verified_pinned_snapshot(&source, revision) {
        return Err(CliError::Preflight(PreflightError::ExistingWork {
            path: source,
        }));
    }
    for path in [&observations, &artifact, &parity] {
        if path.exists() {
            return Err(CliError::Preflight(PreflightError::ExistingWork {
                path: path.clone(),
            }));
        }
    }
    for path in held_out_observations.iter().chain(held_out_parity.iter()) {
        if path.exists() {
            return Err(CliError::Preflight(PreflightError::ExistingWork {
                path: path.clone(),
            }));
        }
    }
    for path in rollouts
        .iter()
        .chain(held_out_rollouts.iter())
        .chain(rollout_parity.iter())
    {
        if path.exists() {
            return Err(CliError::Preflight(PreflightError::ExistingWork {
                path: path.clone(),
            }));
        }
    }

    if check_external {
        if !verified_pinned_snapshot(&source, revision) {
            check_executable(
                "hf",
                "Hugging Face CLI is required; install huggingface_hub[cli] before model build",
            )?;
        }
        check_python_bridge(&capture.python)?;
    }

    Ok(ModelBuildPlan {
        repository: repository.to_owned(),
        revision: revision.to_owned(),
        corpus,
        work_dir,
        source,
        observations,
        artifact,
        parity,
        held_out_corpus,
        held_out_observations,
        held_out_parity,
        rollouts,
        held_out_rollouts,
        rollout_parity,
        capture,
        config,
        thresholds,
    })
}

fn preflight_corpus(path: &Path) -> Result<PathBuf, CliError> {
    let resolved = resolve_path(path)?;
    let metadata = match std::fs::metadata(&resolved) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(CliError::Preflight(PreflightError::MissingCorpus {
                path: resolved,
            }));
        }
        Err(error) => {
            return Err(CliError::Preflight(PreflightError::CorpusUnreadable {
                path: resolved,
                reason: error.to_string(),
            }));
        }
    };
    if !metadata.is_file() {
        return Err(CliError::Preflight(PreflightError::CorpusNotRegular {
            path: resolved,
        }));
    }
    let bytes = std::fs::read(&resolved).map_err(|error| {
        CliError::Preflight(PreflightError::CorpusUnreadable {
            path: resolved.clone(),
            reason: error.to_string(),
        })
    })?;
    let text = String::from_utf8(bytes).map_err(|_| {
        CliError::Preflight(PreflightError::CorpusInvalidUtf8 {
            path: resolved.clone(),
        })
    })?;
    if !text.lines().any(|line| !line.trim().is_empty()) {
        return Err(CliError::Preflight(PreflightError::CorpusEmpty {
            path: resolved,
        }));
    }
    Ok(resolved)
}

fn preflight_source_directory(path: &Path) -> Result<PathBuf, CliError> {
    let resolved = resolve_path(path)?;
    let metadata = std::fs::metadata(&resolved).map_err(|error| {
        CliError::Preflight(PreflightError::SourceInvalid {
            path: resolved.clone(),
            reason: error.to_string(),
        })
    })?;
    if !metadata.is_dir() {
        return Err(CliError::Preflight(PreflightError::SourceInvalid {
            path: resolved,
            reason: "path is not a directory".to_owned(),
        }));
    }
    for required in ["config.json", "tokenizer.json"] {
        if !resolved.join(required).is_file() {
            return Err(CliError::Preflight(PreflightError::SourceInvalid {
                path: resolved,
                reason: format!("missing {required}"),
            }));
        }
    }
    let has_weights = std::fs::read_dir(&resolved)
        .map_err(|error| {
            CliError::Preflight(PreflightError::SourceInvalid {
                path: resolved.clone(),
                reason: error.to_string(),
            })
        })?
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "safetensors")
        });
    if !has_weights {
        return Err(CliError::Preflight(PreflightError::SourceInvalid {
            path: resolved,
            reason: "no .safetensors weights found".to_owned(),
        }));
    }
    Ok(resolved)
}

fn preflight_work_directory(path: &Path) -> Result<PathBuf, CliError> {
    let requested = resolve_path(path)?;
    std::fs::create_dir_all(&requested).map_err(|error| {
        CliError::Preflight(PreflightError::WorkDirectory {
            path: requested.clone(),
            reason: error.to_string(),
        })
    })?;
    let metadata = std::fs::metadata(&requested).map_err(|error| {
        CliError::Preflight(PreflightError::WorkDirectory {
            path: requested.clone(),
            reason: error.to_string(),
        })
    })?;
    if !metadata.is_dir() {
        return Err(CliError::Preflight(PreflightError::WorkDirectory {
            path: requested,
            reason: "path is not a directory".to_owned(),
        }));
    }
    let probe = requested.join(format!(".uor-semantic-preflight-{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|error| {
            CliError::Preflight(PreflightError::WorkDirectory {
                path: requested.clone(),
                reason: error.to_string(),
            })
        })?;
    file.write_all(b"preflight").map_err(|error| {
        CliError::Preflight(PreflightError::WorkDirectory {
            path: requested.clone(),
            reason: error.to_string(),
        })
    })?;
    drop(file);
    std::fs::remove_file(&probe).map_err(|error| {
        CliError::Preflight(PreflightError::WorkDirectory {
            path: requested.clone(),
            reason: error.to_string(),
        })
    })?;
    std::fs::canonicalize(&requested).map_err(|error| {
        CliError::Preflight(PreflightError::WorkDirectory {
            path: requested,
            reason: error.to_string(),
        })
    })
}

fn resolve_path(path: &Path) -> Result<PathBuf, CliError> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    match std::fs::canonicalize(&absolute) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(absolute),
        Err(error) => Err(CliError::Preflight(PreflightError::CorpusUnreadable {
            path: absolute,
            reason: error.to_string(),
        })),
    }
}

fn validate_compiler_config(config: CompilerConfig) -> Result<(), CliError> {
    if config.max_regions == 0 || config.max_regions > u32::MAX as usize {
        return Err(CliError::InvalidValue(
            "regions must be between 1 and 4294967295".to_owned(),
        ));
    }
    if config.iterations == 0 {
        return Err(CliError::InvalidValue(
            "iterations must be non-zero".to_owned(),
        ));
    }
    if config.max_region_emissions == 0 || config.max_region_emissions > usize::from(u16::MAX) {
        return Err(CliError::InvalidValue(
            "region-top-k must be between 1 and 65535".to_owned(),
        ));
    }
    Ok(())
}

fn check_executable(program: &str, requirement: &str) -> Result<(), CliError> {
    let result = Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match result {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(CliError::Preflight(PreflightError::ExternalRequirement {
            program: program.to_owned(),
            requirement: requirement.to_owned(),
            detail: Some(format!("exited with code {:?}", status.code())),
        })),
        Err(error) => Err(CliError::Preflight(PreflightError::ExternalRequirement {
            program: program.to_owned(),
            requirement: requirement.to_owned(),
            detail: Some(error.to_string()),
        })),
    }
}

fn check_python_bridge(python: &str) -> Result<(), CliError> {
    let output = Command::new(python)
        .args(["-c", "import torch, transformers"])
        .output();
    match output {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(CliError::Preflight(PreflightError::ExternalRequirement {
            program: python.to_owned(),
            requirement: "Python bridge requires torch and transformers; install them with python3 -m pip install -r scripts/requirements-hf.txt".to_owned(),
            detail: stderr_detail(&output.stderr),
        })),
        Err(error) => Err(CliError::Preflight(PreflightError::ExternalRequirement {
            program: python.to_owned(),
            requirement: "Python bridge requires an executable Python with torch and transformers".to_owned(),
            detail: Some(error.to_string()),
        })),
    }
}

fn stderr_detail(bytes: &[u8]) -> Option<String> {
    let detail = String::from_utf8_lossy(bytes).trim().to_owned();
    (!detail.is_empty()).then_some(detail)
}

fn verified_pinned_snapshot(source: &Path, revision: &str) -> bool {
    let required = ["config.json", "tokenizer.json", "model.safetensors"];
    if !required.iter().all(|name| source.join(name).is_file()) {
        return false;
    }
    let metadata = source.join(".cache/huggingface/download/config.json.metadata");
    let Ok(text) = std::fs::read_to_string(metadata) else {
        return false;
    };
    text.lines().next() == Some(revision)
}

fn model_generate(arguments: &[String]) -> Result<(), CliError> {
    let artifact = PathBuf::from(required_positional(arguments, 1, "artifact file")?);
    let source = PathBuf::from(required_option(arguments, "--source")?);
    let prompt = required_option(arguments, "--prompt")?;
    let max_tokens = parse_usize(required_option(arguments, "--max-tokens")?, "max-tokens")?;
    let python = option_value(arguments, "--python").unwrap_or("python3");
    let prompt_tokens = tokenizer_call(python, &source, "encode", prompt)?;
    let generated = artifact_generate(&artifact, &prompt_tokens, max_tokens)?;
    let token_csv = generated
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let decoded = tokenizer_call_text(python, &source, "decode", &token_csv)?;
    println!("text: {decoded}");
    Ok(())
}

fn tokenizer_call(
    python: &str,
    source: &Path,
    operation: &str,
    value: &str,
) -> Result<Vec<u32>, CliError> {
    let output = tokenizer_call_text(python, source, operation, value)?;
    parse_tokens(output.trim())
}

fn tokenizer_call_text(
    python: &str,
    source: &Path,
    operation: &str,
    value: &str,
) -> Result<String, CliError> {
    let script = materialize_script("tokenizer_bridge.py", TOKENIZER_SCRIPT)?;
    let output = Command::new(python)
        .arg(script)
        .arg("--source")
        .arg(source)
        .arg(operation)
        .arg(value)
        .output()?;
    if !output.status.success() {
        return Err(CliError::ProcessFailed {
            program: python.to_owned(),
            code: output.status.code(),
            detail: stderr_detail(&output.stderr),
        });
    }
    String::from_utf8(output.stdout)
        .map(|text| text.trim_end().to_owned())
        .map_err(|_| CliError::InvalidValue("tokenizer emitted non-UTF-8 output".to_owned()))
}

fn command_self_test() -> Result<(), CliError> {
    let corpus = ObservationCorpus::parse(concat!(
        "UOROBS1\n",
        "model=fixture/model\n",
        "revision=0123456789abcdef0123456789abcdef01234567\n",
        "source_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "max_context=4\n",
        "top_k=3\n",
        "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000002\n",
        "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000003\n",
        "eos_token=2\n",
        "--\n",
        "O|1,2|3|3:0,4:-10,5:-20\n",
        "O|1,2,3|4|4:0,5:-10,6:-20\n",
    ))?;
    let artifact = compile_observations(&corpus, CompilerConfig::accuracy())?;
    let report = evaluate(&artifact.bytes, &corpus)?;
    if report.exact_top1_basis_points() != 10_000 {
        return Err(CliError::ParityThresholdFailed {
            exact: report.exact_top1_basis_points(),
            required_exact: 10_000,
            graph: report.graph_top1_basis_points(),
            required_graph: 0,
            graph_coverage: report.graph_coverage_basis_points(),
            required_graph_coverage: 0,
            graph_top_k: report.graph_top_k_recall_basis_points(),
            required_graph_top_k: 0,
        });
    }
    let view = ArtifactView::parse(&artifact.bytes)?;
    let mut state = GenerationState::<4>::new()?;
    state.seed(&[1, 2]);
    let mut output = [0u32; 2];
    let mut scratch = ArtifactPredictScratch::<8>::new();
    let mut prediction = Prediction::<8>::new();
    let summary = generate_greedy_into(
        &view,
        &mut state,
        &mut output,
        &mut scratch,
        &mut prediction,
    )?;
    if summary.written() != 2 || output != [3, 4] {
        return Err(CliError::InvalidValue(
            "self-test generation did not follow exact teacher records".to_owned(),
        ));
    }
    println!("self-test: PASS");
    println!("exact_top1_basis_points: 10000");
    println!("generated_tokens: 3,4");
    Ok(())
}

fn compiler_config(arguments: &[String]) -> Result<CompilerConfig, CliError> {
    let mut config = CompilerConfig::accuracy();
    config.max_regions = parse_optional_usize(arguments, "--regions", config.max_regions)?;
    config.iterations = parse_optional_usize(arguments, "--iterations", config.iterations)?;
    config.overlap_margin = u16::try_from(parse_optional_usize(
        arguments,
        "--overlap-margin",
        usize::from(config.overlap_margin),
    )?)
    .map_err(|_| CliError::InvalidValue("overlap-margin exceeds u16".to_owned()))?;
    config.max_region_emissions =
        parse_optional_usize(arguments, "--region-top-k", config.max_region_emissions)?;
    config.include_exact = !flag_present(arguments, "--without-exact");
    Ok(config)
}

fn parity_thresholds(arguments: &[String]) -> Result<ParityThresholds, CliError> {
    let exact = parse_optional_usize(arguments, "--min-exact-bps", 10_000)?;
    let graph = parse_optional_usize(arguments, "--min-graph-bps", 0)?;
    let graph_coverage = parse_optional_usize(arguments, "--min-graph-coverage-bps", 0)?;
    let graph_top_k = parse_optional_usize(arguments, "--min-graph-top-k-bps", 0)?;
    if exact > 10_000 || graph > 10_000 || graph_coverage > 10_000 || graph_top_k > 10_000 {
        return Err(CliError::InvalidValue(
            "parity basis-point thresholds must be between 0 and 10000".to_owned(),
        ));
    }
    Ok(ParityThresholds {
        exact_top1_basis_points: u16::try_from(exact)
            .map_err(|_| CliError::InvalidValue("exact threshold is invalid".to_owned()))?,
        graph_top1_basis_points: u16::try_from(graph)
            .map_err(|_| CliError::InvalidValue("graph threshold is invalid".to_owned()))?,
        graph_coverage_basis_points: u16::try_from(graph_coverage).map_err(|_| {
            CliError::InvalidValue("graph coverage threshold is invalid".to_owned())
        })?,
        graph_top_k_recall_basis_points: u16::try_from(graph_top_k)
            .map_err(|_| CliError::InvalidValue("graph top-k threshold is invalid".to_owned()))?,
    })
}

fn validate_revision(revision: &str) -> Result<(), CliError> {
    if revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(CliError::InvalidValue(
            "revision must be an immutable 40-character hexadecimal commit".to_owned(),
        ))
    }
}

fn run_process<S: AsRef<OsStr>>(program: &str, arguments: &[S]) -> Result<(), CliError> {
    let status = Command::new(program).args(arguments).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::ProcessFailed {
            program: program.to_owned(),
            code: status.code(),
            detail: None,
        })
    }
}

fn run_process_capture<S: AsRef<OsStr>>(program: &str, arguments: &[S]) -> Result<(), CliError> {
    let output = Command::new(program).args(arguments).output()?;
    if output.status.success() {
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        Ok(())
    } else {
        Err(CliError::ProcessFailed {
            program: program.to_owned(),
            code: output.status.code(),
            detail: stderr_detail(&output.stderr),
        })
    }
}

fn materialize_script(name: &str, content: &str) -> Result<PathBuf, CliError> {
    let directory = std::env::temp_dir().join(format!(
        "uor-semantic-{}-{}",
        std::process::id(),
        name.replace('.', "-")
    ));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(name);
    std::fs::write(&path, content)?;
    Ok(path)
}

fn required_positional<'a>(
    arguments: &'a [String],
    index: usize,
    label: &str,
) -> Result<&'a str, CliError> {
    arguments
        .get(index)
        .map(String::as_str)
        .filter(|value| !value.starts_with('-'))
        .ok_or_else(|| CliError::Usage(format!("{label} is required")))
}

fn required_option<'a>(arguments: &'a [String], name: &str) -> Result<&'a str, CliError> {
    option_value(arguments, name)
        .ok_or_else(|| CliError::Usage(format!("required option `{name}` is missing")))
}

fn option_value<'a>(arguments: &'a [String], name: &str) -> Option<&'a str> {
    arguments
        .windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].as_str())
}

fn flag_present(arguments: &[String], name: &str) -> bool {
    arguments.iter().any(|argument| argument == name)
}

fn parse_optional_usize(
    arguments: &[String],
    name: &str,
    default: usize,
) -> Result<usize, CliError> {
    match option_value(arguments, name) {
        Some(value) => parse_usize(value, name),
        None => Ok(default),
    }
}

fn parse_usize(value: &str, label: &str) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .map_err(|_| CliError::InvalidValue(format!("{label} must be a non-negative integer")))
}

fn parse_tokens(value: &str) -> Result<Vec<u32>, CliError> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|token| {
            token
                .trim()
                .parse::<u32>()
                .map_err(|_| CliError::InvalidValue(format!("token `{token}` is not a u32")))
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{
        CliError, command_self_test, download_arguments, model_build_with_downloader,
        preflight_model_build, validate_revision,
    };

    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

    fn test_directory(label: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("uor-semantic-cli-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("test directory creates");
        path
    }

    fn build_arguments(corpus: &Path, work_dir: &Path) -> Vec<String> {
        vec![
            "build".to_owned(),
            "org/model".to_owned(),
            "--revision".to_owned(),
            REVISION.to_owned(),
            "--corpus".to_owned(),
            corpus.display().to_string(),
            "--work-dir".to_owned(),
            work_dir.display().to_string(),
        ]
    }

    fn assert_preflight_failure(result: Result<(), CliError>, needle: &str) {
        let error = result.expect_err("preflight must fail");
        let text = error.to_string();
        assert!(
            text.contains(needle),
            "{text:?} does not contain {needle:?}"
        );
    }

    #[test]
    fn download_command_is_pinned_and_uses_local_dir() {
        let arguments = download_arguments(
            "org/model",
            "0123456789abcdef0123456789abcdef01234567",
            Path::new("models/model"),
        );
        assert_eq!(arguments[0], "download");
        assert!(arguments.windows(2).any(|pair| {
            pair[0] == "--revision" && pair[1] == "0123456789abcdef0123456789abcdef01234567"
        }));
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair[0] == "--local-dir" && pair[1] == "models/model")
        );
    }

    #[test]
    fn moving_revision_is_rejected() {
        assert!(validate_revision("main").is_err());
    }

    #[test]
    fn missing_corpus_fails_before_downloader_hf_04() {
        let root = test_directory("missing-corpus");
        let corpus = root.join("missing.txt");
        let work = root.join("work");
        let arguments = build_arguments(&corpus, &work);
        let mut invoked = false;
        let result = model_build_with_downloader(&arguments, false, |_, _, _| {
            invoked = true;
            Ok(())
        });
        assert_preflight_failure(result, &corpus.display().to_string());
        assert!(!invoked);
        fs::remove_dir_all(root).expect("test directory removes");
    }

    #[test]
    fn empty_corpus_fails_before_downloader_hf_04() {
        let root = test_directory("empty-corpus");
        let corpus = root.join("empty.txt");
        fs::write(&corpus, " \n\n\t\n").expect("empty corpus writes");
        let arguments = build_arguments(&corpus, &root.join("work"));
        let mut invoked = false;
        let result = model_build_with_downloader(&arguments, false, |_, _, _| {
            invoked = true;
            Ok(())
        });
        assert_preflight_failure(result, "no non-blank samples");
        assert!(!invoked);
        fs::remove_dir_all(root).expect("test directory removes");
    }

    #[test]
    fn unreadable_corpus_fails_before_downloader_hf_04() {
        let root = test_directory("unreadable-corpus");
        let corpus = root.join("unreadable.txt");
        fs::write(&corpus, "sample\n").expect("corpus writes");
        let arguments = build_arguments(&corpus, &root.join("work"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&corpus, fs::Permissions::from_mode(0o000))
                .expect("corpus permissions change");
            let mut invoked = false;
            let result = model_build_with_downloader(&arguments, false, |_, _, _| {
                invoked = true;
                Ok(())
            });
            assert_preflight_failure(result, "not readable");
            assert!(!invoked);
            fs::set_permissions(&corpus, fs::Permissions::from_mode(0o600))
                .expect("corpus permissions restore");
        }

        #[cfg(not(unix))]
        {
            let result = model_build_with_downloader(&arguments, false, |_, _, _| Ok(()));
            assert_preflight_failure(result, "not");
        }
        fs::remove_dir_all(root).expect("test directory removes");
    }

    #[test]
    fn invalid_numeric_bound_fails_before_downloader_hf_04() {
        let root = test_directory("invalid-number");
        let corpus = root.join("corpus.txt");
        fs::write(&corpus, "sample\n").expect("corpus writes");
        let mut arguments = build_arguments(&corpus, &root.join("work"));
        arguments.extend(["--iterations".to_owned(), "0".to_owned()]);
        let mut invoked = false;
        let result = model_build_with_downloader(&arguments, false, |_, _, _| {
            invoked = true;
            Ok(())
        });
        assert_preflight_failure(result, "iterations must be non-zero");
        assert!(!invoked);
        fs::remove_dir_all(root).expect("test directory removes");
    }

    #[test]
    fn relative_corpus_is_resolved_before_child_hf_05() {
        let relative = PathBuf::from(format!(
            ".uor-semantic-relative-corpus-{}.txt",
            std::process::id()
        ));
        fs::write(&relative, "sample\n").expect("relative corpus writes");
        let root = test_directory("absolute-corpus");
        let arguments = build_arguments(&relative, &root.join("work"));
        let plan = preflight_model_build(&arguments, false).expect("preflight succeeds");
        assert!(plan.corpus.is_absolute());
        assert_eq!(
            plan.corpus,
            fs::canonicalize(&relative).expect("corpus canonicalizes")
        );
        fs::remove_file(relative).expect("relative corpus removes");
        fs::remove_dir_all(root).expect("test directory removes");
    }

    #[test]
    fn existing_unverified_work_is_not_overwritten_cp_02() {
        let root = test_directory("existing-work");
        let corpus = root.join("corpus.txt");
        let work = root.join("work");
        let source = work.join("source");
        fs::create_dir_all(&source).expect("source directory creates");
        fs::write(source.join("sentinel"), "keep me").expect("sentinel writes");
        fs::write(&corpus, "sample\n").expect("corpus writes");
        let arguments = build_arguments(&corpus, &work);
        let mut invoked = false;
        let result = model_build_with_downloader(&arguments, false, |_, _, _| {
            invoked = true;
            Ok(())
        });
        assert_preflight_failure(result, "not a verified pinned snapshot");
        assert!(!invoked);
        assert_eq!(
            fs::read_to_string(source.join("sentinel")).expect("sentinel reads"),
            "keep me"
        );
        fs::remove_dir_all(root).expect("test directory removes");
    }

    #[test]
    fn valid_corpus_reaches_downloader_hf_04() {
        let root = test_directory("valid-corpus");
        let corpus = root.join("corpus.txt");
        let work = root.join("work");
        fs::write(&corpus, "sample\n").expect("corpus writes");
        let arguments = build_arguments(&corpus, &work);
        let mut invoked = false;
        let result = model_build_with_downloader(&arguments, false, |_, _, source| {
            invoked = true;
            assert!(source.is_absolute());
            Err(CliError::InvalidValue(
                "controlled downloader stop".to_owned(),
            ))
        });
        assert_preflight_failure(result, "controlled downloader stop");
        assert!(invoked);
        fs::remove_dir_all(root).expect("test directory removes");
    }

    #[test]
    fn separate_construction_and_held_out_corpora_are_planned_ac_03() {
        let root = test_directory("held-out-corpus");
        let construction = root.join("construction.txt");
        let held_out = root.join("held-out.txt");
        let work = root.join("work");
        fs::write(&construction, "construction sample\n").expect("construction writes");
        fs::write(&held_out, "held-out sample\n").expect("held-out writes");
        let mut arguments = build_arguments(&construction, &work);
        arguments.extend([
            "--held-out-corpus".to_owned(),
            held_out.display().to_string(),
        ]);
        let plan = preflight_model_build(&arguments, false).expect("preflight succeeds");
        assert_eq!(
            plan.held_out_corpus,
            Some(fs::canonicalize(&held_out).expect("held-out canonicalizes"))
        );
        assert!(
            plan.held_out_observations
                .as_ref()
                .is_some_and(|path| path.ends_with("held-out-observations.uorobs"))
        );
        assert!(
            plan.held_out_parity
                .as_ref()
                .is_some_and(|path| path.ends_with("held-out-parity.json"))
        );
        fs::remove_dir_all(root).expect("test directory removes");
    }

    #[test]
    fn self_test_executes_compiler_runtime_and_generation() {
        command_self_test().expect("self-test passes");
    }
}

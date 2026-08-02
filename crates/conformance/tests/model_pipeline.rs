//! Artifact, compiler, Hugging Face command, CLI, and parity conformance.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use uor_semantic::{
    ArtifactError, ArtifactPredictScratch, ArtifactView, CodebookId, CompatibilityError,
    CompatibilityFormat, CompatibilityManifest, CompatibilityPrediction, CompatibilityWitness,
    Depth, ExactPolicy, GenerationState, MAX_EMISSION_RECORDS, MAX_EXACT_RECORDS,
    MAX_REGION_RECORDS, Prediction, PredictionSource, R4G1Error, R4G1Graph, R4G1Identity,
    R4G1RangeField, R4G1Section, R4G1Structure, R4Status, ResidualContribution,
    ResidualContributionKind, ScoreAccumulator, ScoreQ, ScoringError, TokenScore,
    context_signature, generate_greedy_into,
};
use uor_semantic_cli::{
    CaptureOptions, CompileRequest as CliCompileRequest, DownloadRequest, SourceCompileRequest,
    compile as cli_compile, compile_source, download as cli_download, download_arguments,
    export_r4g1 as cli_export_r4g1, run as cli_run,
};
use uor_semantic_compiler::{
    CompilerConfig, Observation, ObservationCorpus, ObservationMetadata, ObservedEmission,
    ParityThresholds, RolloutCorpus, compile, evaluate, evaluate_graph_only, evaluate_rollouts,
    export_r4g1, replay_r4g1, verify_r4g1_cids,
};

const TRAIN: &str = concat!(
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
    "O|7,8|3|3:0,4:-15,5:-25\n",
);

const HELD_OUT: &str = concat!(
    "UOROBS1\n",
    "model=fixture/model\n",
    "revision=0123456789abcdef0123456789abcdef01234567\n",
    "source_sha256=0000000000000000000000000000000000000000000000000000000000000002\n",
    "max_context=4\n",
    "top_k=3\n",
    "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
    "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000002\n",
    "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000003\n",
    "eos_token=2\n",
    "--\n",
    "O|100,200|3|3:0,4:-10,5:-20\n",
);

const OVERLAPPING_HELD_OUT: &str = concat!(
    "UOROBS1\n",
    "model=fixture/model\n",
    "revision=0123456789abcdef0123456789abcdef01234567\n",
    "source_sha256=0000000000000000000000000000000000000000000000000000000000000003\n",
    "max_context=4\n",
    "top_k=3\n",
    "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
    "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000002\n",
    "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000003\n",
    "eos_token=2\n",
    "--\n",
    "O|1,2|3|3:0,4:-10,5:-20\n",
);

fn train() -> ObservationCorpus {
    ObservationCorpus::parse(TRAIN).expect("training fixture parses")
}

fn r4g1_identity_fixture(
    artifact_id: CodebookId,
    teacher_id: CodebookId,
    tokenizer_id: CodebookId,
) -> Vec<u8> {
    let header_len = 88usize;
    let entry_len = 16usize;
    let head_offset = header_len + entry_len;
    let head_len = 224usize;
    let total_len = head_offset + head_len;
    let mut bytes = vec![0u8; total_len];
    bytes[0..4].copy_from_slice(b"R4G1");
    bytes[4] = 0;
    bytes[5] = 0;
    bytes[6] = 1;
    bytes[7] = 3;
    bytes[8..16].copy_from_slice(&(total_len as u64).to_le_bytes());
    bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
    bytes[20..24].copy_from_slice(&0u32.to_le_bytes());
    bytes[24..56].copy_from_slice(artifact_id.as_bytes());
    bytes[56..88].copy_from_slice(&[0u8; 32]);
    bytes[88..92].copy_from_slice(&1u32.to_le_bytes());
    bytes[92..96].copy_from_slice(&0u32.to_le_bytes());
    bytes[96..100].copy_from_slice(&(head_offset as u32).to_le_bytes());
    bytes[100..104].copy_from_slice(&(head_len as u32).to_le_bytes());
    bytes[head_offset..head_offset + 32].copy_from_slice(teacher_id.as_bytes());
    bytes[head_offset + 32..head_offset + 64].copy_from_slice(tokenizer_id.as_bytes());
    bytes
}

fn r4g1_structure_fixture() -> Vec<u8> {
    let ids = [1u32, 2, 3, 4, 5, 6, 8];
    let table_end = 88usize + ids.len() * 16;
    let head_offset = table_end;
    let head_len = 224usize;
    let body_len = 8usize;
    let total_len = head_offset + head_len + body_len * 6;
    let mut bytes = vec![0u8; total_len];
    bytes[0..4].copy_from_slice(b"R4G1");
    bytes[4] = 0;
    bytes[5] = 0;
    bytes[6] = 1;
    bytes[7] = 3;
    bytes[8..16].copy_from_slice(&(total_len as u64).to_le_bytes());
    bytes[16..20].copy_from_slice(&(ids.len() as u32).to_le_bytes());
    bytes[20..24].copy_from_slice(&0u32.to_le_bytes());
    bytes[24..56].copy_from_slice(&[1u8; 32]);
    bytes[56..88].copy_from_slice(&[0u8; 32]);

    let mut cursor = 88usize;
    let mut offset = head_offset;
    let mut index = 0usize;
    while index < ids.len() {
        let length = if ids[index] == 1 { head_len } else { body_len };
        bytes[cursor..cursor + 4].copy_from_slice(&ids[index].to_le_bytes());
        bytes[cursor + 4..cursor + 8].copy_from_slice(&0u32.to_le_bytes());
        bytes[cursor + 8..cursor + 12].copy_from_slice(&(offset as u32).to_le_bytes());
        bytes[cursor + 12..cursor + 16].copy_from_slice(&(length as u32).to_le_bytes());
        cursor += 16;
        offset += length;
        index += 1;
    }
    bytes[head_offset..head_offset + 32].copy_from_slice(&[2u8; 32]);
    bytes[head_offset + 32..head_offset + 64].copy_from_slice(&[3u8; 32]);
    bytes
}

fn r4g1_graph_fixture() -> Vec<u8> {
    let ids = [1u32, 2, 3, 4, 5, 6, 8];
    let head_offset = 88usize + ids.len() * 16;
    let code_offset = head_offset + 224;
    let node_offset = code_offset + 8;
    let edge_offset = node_offset + 32;
    let rout_offset = edge_offset + 24;
    let emit_offset = rout_offset + 64;
    let prov_offset = emit_offset + 8;
    let total_len = prov_offset + 8;
    let mut bytes = vec![0u8; total_len];
    bytes[0..4].copy_from_slice(b"R4G1");
    bytes[4] = 0;
    bytes[5] = 0;
    bytes[6] = 1;
    bytes[7] = 3;
    bytes[8..16].copy_from_slice(&(total_len as u64).to_le_bytes());
    bytes[16..20].copy_from_slice(&(ids.len() as u32).to_le_bytes());
    bytes[24..56].copy_from_slice(&[1u8; 32]);

    let offsets = [
        head_offset,
        code_offset,
        node_offset,
        edge_offset,
        rout_offset,
        emit_offset,
        prov_offset,
    ];
    let lengths = [224usize, 8, 30, 20, 64, 8, 8];
    let mut cursor = 88usize;
    let mut index = 0usize;
    while index < ids.len() {
        bytes[cursor..cursor + 4].copy_from_slice(&ids[index].to_le_bytes());
        bytes[cursor + 8..cursor + 12].copy_from_slice(&(offsets[index] as u32).to_le_bytes());
        bytes[cursor + 12..cursor + 16].copy_from_slice(&(lengths[index] as u32).to_le_bytes());
        cursor += 16;
        index += 1;
    }

    let head = head_offset;
    bytes[head + 180..head + 182].copy_from_slice(&4u16.to_le_bytes());
    bytes[head + 182..head + 184].copy_from_slice(&4u16.to_le_bytes());
    bytes[head + 184..head + 186].copy_from_slice(&1u16.to_le_bytes());
    bytes[head + 186..head + 188].copy_from_slice(&1u16.to_le_bytes());
    bytes[head + 188..head + 192].copy_from_slice(&4u32.to_le_bytes());
    bytes[head + 192..head + 196].copy_from_slice(&4u32.to_le_bytes());
    bytes[head + 196..head + 200].copy_from_slice(&1u32.to_le_bytes());
    bytes[head + 200..head + 204].copy_from_slice(&1u32.to_le_bytes());
    bytes[head + 204] = 1;
    bytes[head + 212..head + 214].copy_from_slice(&8u16.to_le_bytes());
    bytes[head + 220..head + 224].copy_from_slice(&16u32.to_le_bytes());

    let node = node_offset;
    bytes[node + 4..node + 6].copy_from_slice(&1u16.to_le_bytes());
    bytes[node + 10..node + 12].copy_from_slice(&1u16.to_le_bytes());
    bytes[node + 22..node + 26].copy_from_slice(&1u32.to_le_bytes());

    let edge = edge_offset;
    bytes[edge + 16..edge + 20].copy_from_slice(&0u32.to_le_bytes());
    bytes[emit_offset] = 0;
    bytes
}

#[test]
fn default_accuracy_profile_uses_cross_validated_graph_settings_gp_05() {
    let config = CompilerConfig::accuracy();

    assert_eq!(config.max_regions, 48);
    assert_eq!(config.iterations, 16);
    assert_eq!(config.overlap_margin, 16);
    assert_eq!(config.max_region_emissions, 1);
    assert!(config.include_exact);
}

#[test]
fn accuracy_profile_preserves_bounded_graph_work_gp_06() {
    let config = CompilerConfig::accuracy();
    let compiled = compile(&train(), config).expect("fixture compiles");
    let report = evaluate_graph_only(
        &compiled.bytes,
        &ObservationCorpus::parse(HELD_OUT).expect("held-out parses"),
    )
    .expect("graph parity evaluates");

    assert!(compiled.regions <= config.max_regions);
    assert!(compiled.emissions <= config.max_regions);
    assert!(report.graph_regions_scanned <= compiled.regions);
}

#[test]
fn artifact_round_trip_preserves_exact_prediction_ar_01() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let artifact = ArtifactView::parse(&compiled.bytes).expect("artifact validates");
    let mut scratch = ArtifactPredictScratch::<8>::new();
    let mut prediction = Prediction::<8>::new();
    let summary = artifact
        .predict(
            &[1, 2],
            ExactPolicy::PreferExact,
            &mut scratch,
            &mut prediction,
        )
        .expect("prediction succeeds");

    assert_eq!(summary.source(), PredictionSource::Exact);
    assert_eq!(prediction.first().map(|entry| entry.token()), Some(3));
}

#[test]
fn greedy_generation_follows_compiled_teacher_chain_gn_01() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let artifact = ArtifactView::parse(&compiled.bytes).expect("artifact validates");
    let mut state = GenerationState::<4>::new().expect("state capacity is valid");
    state.seed(&[1, 2]);
    let mut output = [0u32; 2];
    let mut scratch = ArtifactPredictScratch::<8>::new();
    let mut prediction = Prediction::<8>::new();
    let summary = generate_greedy_into(
        &artifact,
        &mut state,
        &mut output,
        &mut scratch,
        &mut prediction,
    )
    .expect("generation succeeds");

    assert_eq!(summary.written(), 2);
    assert_eq!(output, [3, 4]);
    assert_eq!(summary.exact_steps(), 2);
}

#[test]
fn hugging_face_download_command_requires_immutable_revision_hf_01() {
    let arguments = download_arguments(
        "org/model",
        "0123456789abcdef0123456789abcdef01234567",
        Path::new("models/model"),
    );
    assert_eq!(arguments[0], "download");
    assert_eq!(arguments[2], "--revision");
    assert_eq!(arguments[3], "0123456789abcdef0123456789abcdef01234567");
    assert_eq!(arguments[4], "--local-dir");
}

#[test]
fn captured_observations_compile_to_runtime_artifact_hf_02() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    assert_eq!(compiled.observations, 3);
    assert_eq!(compiled.exact_records, 3);
    assert!(!compiled.bytes.is_empty());
    let artifact = ArtifactView::parse(&compiled.bytes).expect("runtime accepts compiler output");
    assert_eq!(artifact.exact_count(), compiled.exact_records);
    assert_eq!(artifact.region_count(), compiled.regions);
    assert_eq!(artifact.emission_count(), compiled.emissions);
}

#[test]
fn artifact_identity_rejects_tampering_ar_02() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let mut tampered = compiled.bytes.clone();
    tampered[64] ^= 1;
    assert_eq!(
        ArtifactView::parse(&tampered),
        Err(ArtifactError::IdentityMismatch)
    );
}

#[test]
fn artifact_parser_rejects_resource_limits_ar_03() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    for (offset, maximum, section) in [
        (20usize, MAX_EXACT_RECORDS, "exact records"),
        (24usize, MAX_REGION_RECORDS, "region records"),
        (28usize, MAX_EMISSION_RECORDS, "emission records"),
    ] {
        let mut oversized = compiled.bytes.clone();
        oversized[offset..offset + 4].copy_from_slice(
            &u32::try_from(maximum + 1)
                .expect("fixture limit fits")
                .to_le_bytes(),
        );
        let error = ArtifactView::parse(&oversized).expect_err("resource limit rejects");
        assert!(error.to_string().contains(section));
    }
}

#[test]
fn indexed_candidate_routing_avoids_full_region_scan_ar_04() {
    let mut contexts = Vec::new();
    let mut buckets = BTreeSet::new();
    let mut token = 0u32;
    while contexts.len() < 8 {
        let context = vec![token];
        let bucket = (context_signature(&context)[0] & 0xff) as u8;
        if buckets.insert(bucket) {
            contexts.push(context);
        }
        token = token.saturating_add(1);
    }
    let observations = contexts
        .iter()
        .enumerate()
        .map(|(index, context)| Observation {
            context: context.clone(),
            target: u32::try_from(index + 1).expect("fixture token fits"),
            emissions: vec![ObservedEmission {
                token: u32::try_from(index + 1).expect("fixture token fits"),
                score: 0,
            }],
        })
        .collect();
    let corpus = ObservationCorpus {
        metadata: ObservationMetadata {
            model: "fixture/model".to_owned(),
            revision: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            source_sha256: [1; 32],
            max_context: 4,
            top_k: 1,
            tokenizer_sha256: [1; 32],
            chat_template_sha256: [2; 32],
            special_tokens_sha256: [3; 32],
            eos_token: 2,
        },
        observations,
    };
    let compiled = compile(
        &corpus,
        CompilerConfig {
            max_regions: contexts.len(),
            iterations: 1,
            overlap_margin: 0,
            max_region_emissions: 1,
            include_exact: false,
        },
    )
    .expect("indexed fixture compiles");
    let artifact = ArtifactView::parse(&compiled.bytes).expect("indexed artifact parses");
    let mut scratch = ArtifactPredictScratch::<8>::new();
    let mut prediction = Prediction::<8>::new();
    let summary = artifact
        .predict(
            &contexts[0],
            ExactPolicy::GraphOnly,
            &mut scratch,
            &mut prediction,
        )
        .expect("indexed prediction succeeds");
    assert_eq!(summary.regions_scanned(), 1);
    assert!(summary.regions_scanned() < artifact.region_count());
}

#[test]
fn canonical_compiler_output_is_byte_identical_cp_01() {
    let corpus = train();
    let first = compile(&corpus, CompilerConfig::accuracy()).expect("first compile");
    let second = compile(&corpus, CompilerConfig::accuracy()).expect("second compile");
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(first.codebook_id, second.codebook_id);
}

#[test]
fn exact_context_lane_matches_teacher_argmax_gp_01() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let report = evaluate(&compiled.bytes, &corpus).expect("parity evaluates");
    assert_eq!(report.exact_top1_basis_points(), 10_000);
    assert_eq!(report.exact_covered, report.samples);
}

#[test]
fn parity_certificate_rejects_unmet_threshold_gp_02() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let held_out = ObservationCorpus::parse(HELD_OUT).expect("held-out fixture parses");
    let report = evaluate(&compiled.bytes, &held_out).expect("parity evaluates");
    let exact_required = ParityThresholds {
        exact_top1_basis_points: 10_000,
        graph_top1_basis_points: 0,
        graph_coverage_basis_points: 0,
        graph_top_k_recall_basis_points: 0,
    };

    assert_eq!(report.exact_covered, 0);
    assert_eq!(report.exact_top1_basis_points(), 0);
    assert!(!report.passes(exact_required));
}

#[test]
fn parity_certificate_requires_coverage_and_top_k_floors_gp_04() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let held_out = ObservationCorpus::parse(HELD_OUT).expect("held-out fixture parses");
    let report = evaluate_graph_only(&compiled.bytes, &held_out).expect("parity evaluates");
    let thresholds = ParityThresholds {
        exact_top1_basis_points: 0,
        graph_top1_basis_points: 0,
        graph_coverage_basis_points: 10_001,
        graph_top_k_recall_basis_points: 10_001,
    };

    assert!(!report.passes(thresholds));
}

#[test]
fn graph_only_parity_reports_top_k_recall_gp_03() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let report = evaluate_graph_only(&compiled.bytes, &corpus).expect("parity evaluates");

    assert!(report.graph_top_k_recall_total >= report.samples);
    assert!(report.graph_top_k_recall_hits <= report.graph_top_k_recall_total);
    assert!(report.graph_coverage_basis_points() <= 10_000);
    assert!(report.graph_top_k_recall_covered_hits <= report.graph_top_k_recall_covered_total);
    assert!(report.to_json().contains("graph_top_k_recall_basis_points"));
}

#[test]
fn graph_only_held_out_fixture_is_measured_without_exact_lookup_ac_02() {
    let corpus = train();
    let config = CompilerConfig {
        max_regions: 1,
        iterations: 4,
        overlap_margin: 256,
        max_region_emissions: 8,
        include_exact: true,
    };
    let compiled = compile(&corpus, config).expect("fixture compiles");
    let held_out = ObservationCorpus::parse(HELD_OUT).expect("held-out fixture parses");
    let report = evaluate(&compiled.bytes, &held_out).expect("held-out parity evaluates");
    assert_eq!(report.exact_covered, 0);
    assert_eq!(report.graph_covered, 1);
    assert_eq!(report.graph_top1_basis_points(), 10_000);
}

#[test]
fn separate_held_out_input_forces_exact_lookup_off_ac_03() {
    let construction = train();
    let compiled = compile(
        &construction,
        CompilerConfig {
            max_regions: 1,
            iterations: 4,
            overlap_margin: 256,
            max_region_emissions: 8,
            include_exact: true,
        },
    )
    .expect("construction compiles");
    let held_out = ObservationCorpus::parse(OVERLAPPING_HELD_OUT).expect("held-out fixture parses");
    let report = evaluate_graph_only(&compiled.bytes, &held_out).expect("graph-only evaluates");
    assert_eq!(report.exact_covered, 0);
    assert_eq!(report.exact_top1_matches, 0);
    assert_eq!(report.graph_covered, 1);
    assert_eq!(report.graph_top1_basis_points(), 10_000);
}

#[test]
fn graph_only_parity_reports_indexed_candidate_work_ac_04() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let report = evaluate_graph_only(&compiled.bytes, &corpus).expect("parity evaluates");

    assert!(report.graph_regions_scanned >= report.graph_covered);
    assert!(report.to_json().contains("graph_regions_scanned"));
}

#[test]
fn symmetric_graph_cross_validation_keeps_exact_lookup_off_ac_05() {
    let first = train();
    let second = ObservationCorpus::parse(HELD_OUT).expect("held-out fixture parses");
    let first_artifact = compile(&first, CompilerConfig::accuracy()).expect("first compiles");
    let second_artifact = compile(&second, CompilerConfig::accuracy()).expect("second compiles");
    let first_report =
        evaluate_graph_only(&first_artifact.bytes, &second).expect("first direction evaluates");
    let second_report =
        evaluate_graph_only(&second_artifact.bytes, &first).expect("second direction evaluates");

    assert_eq!(first_report.exact_covered, 0);
    assert_eq!(second_report.exact_covered, 0);
    assert!(first_report.graph_top_k_recall_total > 0);
    assert!(second_report.graph_top_k_recall_total > 0);
}

#[test]
fn autoregressive_rollout_reports_sequence_and_eos_parity_gn_02() {
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
    let rollouts = RolloutCorpus::parse(concat!(
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

#[test]
fn tokenizer_identity_mismatch_fails_before_parity_measurement_hf_06() {
    let corpus = ObservationCorpus::parse(TRAIN).expect("training fixture parses");
    let artifact = compile(&corpus, CompilerConfig::accuracy()).expect("compile");
    for (needle, replacement, field) in [
        (
            "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000001",
            "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000009",
            "tokenizer_sha256",
        ),
        (
            "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000002",
            "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000009",
            "chat_template_sha256",
        ),
        (
            "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000003",
            "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000009",
            "special_tokens_sha256",
        ),
        ("eos_token=2", "eos_token=3", "eos_token"),
    ] {
        let mismatched = TRAIN.replace(needle, replacement);
        let mismatched = ObservationCorpus::parse(&mismatched).expect("mismatched fixture parses");
        let error = evaluate(&artifact.bytes, &mismatched).expect_err("identity mismatch rejects");
        assert!(error.to_string().contains(field));
    }
}

#[test]
fn cli_self_test_runs_end_to_end_cl_01() {
    let arguments = vec!["self-test".to_owned()];
    uor_semantic_cli::run(&arguments).expect("CLI self-test passes");
}

#[test]
fn interactive_artifact_commands_succeed_cl_02() {
    let compiled = compile(&train(), CompilerConfig::accuracy()).expect("fixture compiles");
    let artifact_path = std::env::temp_dir().join(format!(
        "uor-semantic-interactive-{}.uors",
        std::process::id()
    ));
    fs::write(&artifact_path, &compiled.bytes).expect("artifact writes");
    let artifact = artifact_path.display().to_string();

    uor_semantic_cli::run(&[
        "artifact".to_owned(),
        "inspect".to_owned(),
        artifact.clone(),
    ])
    .expect("inspect succeeds");
    uor_semantic_cli::run(&[
        "artifact".to_owned(),
        "predict".to_owned(),
        artifact.clone(),
        "--tokens".to_owned(),
        "1,2".to_owned(),
        "--graph-only".to_owned(),
    ])
    .expect("predict succeeds");
    uor_semantic_cli::run(&[
        "artifact".to_owned(),
        "generate".to_owned(),
        artifact.clone(),
        "--tokens".to_owned(),
        "1,2".to_owned(),
        "--max-tokens".to_owned(),
        "2".to_owned(),
    ])
    .expect("generate succeeds");

    fs::remove_file(artifact_path).expect("artifact removes");
}

#[test]
fn compatibility_manifest_binds_r4g1_and_tla5_identity_cx_01() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let artifact = ArtifactView::parse(&compiled.bytes).expect("artifact validates");

    for format in [CompatibilityFormat::R4G1, CompatibilityFormat::Tla5] {
        let manifest = CompatibilityManifest::from_artifact(format, &artifact);
        manifest
            .validate_artifact(&artifact)
            .expect("compatibility identity validates");
        assert_eq!(manifest.artifact_id(), artifact.codebook_id());
        assert_eq!(manifest.source_id(), artifact.source_id());
        assert_eq!(manifest.tokenizer_id(), artifact.tokenizer_id());
        assert_eq!(manifest.store_id(), None);
        assert_eq!(manifest.certificate_id(), None);
    }

    let mismatched = CompatibilityManifest::new(
        CompatibilityFormat::R4G1,
        CodebookId::from_bytes([9; 32]),
        artifact.source_id(),
        artifact.tokenizer_id(),
        None,
        None,
    );
    assert!(mismatched.validate_artifact(&artifact).is_err());
}

#[test]
fn compatibility_status_mapping_preserves_target_outcomes_cx_02() {
    assert_eq!(
        R4Status::from_source(PredictionSource::Exact),
        R4Status::ExactContext
    );
    assert_eq!(
        R4Status::from_source(PredictionSource::Graph),
        R4Status::Graph
    );
    assert_eq!(
        R4Status::from_source(PredictionSource::Novel),
        R4Status::Novel
    );
    assert_eq!(R4Status::Contradictory.source(), None);
    let contradictory = CompatibilityPrediction::<1>::new_status(R4Status::Contradictory, false);
    assert!(!contradictory.matches_runtime(
        &Prediction::<1>::new(),
        PredictionSource::Novel,
        false
    ));
}

#[test]
fn compatibility_witness_enforces_identity_status_token_and_region_cx_03() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let artifact = ArtifactView::parse(&compiled.bytes).expect("artifact validates");
    let manifest = CompatibilityManifest::from_artifact(CompatibilityFormat::R4G1, &artifact);
    let mut scratch = ArtifactPredictScratch::<8>::new();
    let mut prediction = Prediction::<8>::new();
    let summary = artifact
        .predict(
            &[1, 2],
            ExactPolicy::PreferExact,
            &mut scratch,
            &mut prediction,
        )
        .expect("prediction succeeds");
    let token = prediction.first().expect("fixture predicts").token();
    let witness = CompatibilityWitness::new(
        manifest.artifact_id(),
        R4Status::ExactContext,
        None,
        Depth::new(0),
        token,
        false,
    )
    .expect("exact witness is structurally valid");
    witness
        .verify(&manifest, &prediction, summary.source())
        .expect("exact witness verifies");

    assert_eq!(
        CompatibilityWitness::new(
            manifest.artifact_id(),
            R4Status::Graph,
            None,
            Depth::new(1),
            token,
            false,
        ),
        Err(CompatibilityError::MissingRegionWitness)
    );
    assert_eq!(
        CompatibilityWitness::new(
            CodebookId::from_bytes([9; 32]),
            R4Status::ExactContext,
            None,
            Depth::new(0),
            token,
            false,
        )
        .expect("identity can be constructed")
        .verify(&manifest, &prediction, summary.source()),
        Err(CompatibilityError::WitnessIdentityMismatch)
    );
    assert_eq!(
        CompatibilityWitness::new(
            manifest.artifact_id(),
            R4Status::Contradictory,
            None,
            Depth::new(0),
            token,
            false,
        ),
        Err(CompatibilityError::ContradictoryPrediction)
    );
}

#[test]
fn compatibility_prediction_equivalence_preserves_ranked_output_cx_04() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let artifact = ArtifactView::parse(&compiled.bytes).expect("artifact validates");
    let mut scratch = ArtifactPredictScratch::<8>::new();
    let mut prediction = Prediction::<8>::new();
    let summary = artifact
        .predict(
            &[1, 2],
            ExactPolicy::PreferExact,
            &mut scratch,
            &mut prediction,
        )
        .expect("prediction succeeds");
    let mut target = CompatibilityPrediction::<8>::new(summary.source(), false);
    for entry in prediction.as_slice() {
        target
            .push(TokenScore::new(
                entry.token(),
                ScoreQ::from_raw(entry.score().raw()),
            ))
            .expect("target capacity fits");
    }
    assert!(target.matches_runtime(&prediction, summary.source(), false));

    let mut reordered = CompatibilityPrediction::<8>::new(summary.source(), false);
    reordered
        .push(TokenScore::new(4, ScoreQ::from_raw(-10)))
        .expect("target capacity fits");
    reordered
        .push(TokenScore::new(3, ScoreQ::from_raw(0)))
        .expect("target capacity fits");
    assert!(!reordered.matches_runtime(&prediction, summary.source(), false));
}

#[test]
fn r4g1_identity_header_maps_to_manifest_cx_05() {
    let corpus = train();
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let artifact = ArtifactView::parse(&compiled.bytes).expect("artifact validates");
    let bytes = r4g1_identity_fixture(
        artifact.codebook_id(),
        artifact.source_id(),
        artifact.tokenizer_id(),
    );
    let identity = R4G1Identity::parse(&bytes).expect("R4G1 identity parses");
    let manifest = identity.to_manifest();

    assert_eq!(identity.artifact_id(), artifact.codebook_id());
    assert_eq!(identity.teacher_id(), artifact.source_id());
    assert_eq!(identity.tokenizer_id(), artifact.tokenizer_id());
    manifest
        .validate_artifact(&artifact)
        .expect("adapted identities validate");
}

#[test]
fn r4g1_identity_header_rejects_malformed_containers_cx_06() {
    let valid = r4g1_identity_fixture(
        CodebookId::from_bytes([1; 32]),
        CodebookId::from_bytes([2; 32]),
        CodebookId::from_bytes([3; 32]),
    );
    assert_eq!(
        R4G1Identity::parse(&valid[..87]),
        Err(R4G1Error::HeaderTooShort)
    );

    let mut unsupported = valid.clone();
    unsupported[4] = 1;
    assert_eq!(
        R4G1Identity::parse(&unsupported),
        Err(R4G1Error::UnsupportedMajor { found: 1 })
    );

    let mut mis_sized = valid.clone();
    mis_sized[8..16].copy_from_slice(&((valid.len() as u64) + 1).to_le_bytes());
    assert_eq!(
        R4G1Identity::parse(&mis_sized),
        Err(R4G1Error::LengthMismatch)
    );

    let mut bad_head_size = valid.clone();
    bad_head_size[100..104].copy_from_slice(&223u32.to_le_bytes());
    assert_eq!(
        R4G1Identity::parse(&bad_head_size),
        Err(R4G1Error::HeadLengthMismatch)
    );

    let mut misaligned = valid.clone();
    misaligned[96..100].copy_from_slice(&105u32.to_le_bytes());
    assert_eq!(
        R4G1Identity::parse(&misaligned),
        Err(R4G1Error::UnalignedSection)
    );

    let mut headless = valid;
    headless[88..92].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(R4G1Identity::parse(&headless), Err(R4G1Error::MissingHead));

    let mut oversized_table = r4g1_identity_fixture(
        CodebookId::from_bytes([1; 32]),
        CodebookId::from_bytes([2; 32]),
        CodebookId::from_bytes([3; 32]),
    );
    oversized_table[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        R4G1Identity::parse(&oversized_table),
        Err(R4G1Error::SectionTableOutOfBounds)
    );
}

#[test]
fn r4g1_structure_view_exposes_required_sections_cx_07() {
    let bytes = r4g1_structure_fixture();
    let view = R4G1Structure::parse(&bytes).expect("structure validates");

    assert_eq!(view.identity().section_count(), 7);
    assert_eq!(view.section(R4G1Section::Head).map(<[u8]>::len), Some(224));
    assert_eq!(view.section(R4G1Section::Code).map(<[u8]>::len), Some(8));
    assert_eq!(view.section(R4G1Section::Prov).map(<[u8]>::len), Some(8));
    assert_eq!(view.section(R4G1Section::Exct), None);
}

#[test]
fn r4g1_structure_view_rejects_unsafe_section_topology_cx_08() {
    let valid = r4g1_structure_fixture();

    let mut unknown = valid.clone();
    unknown[88 + 6 * 16..92 + 6 * 16].copy_from_slice(&0x20u32.to_le_bytes());
    assert_eq!(
        R4G1Structure::parse(&unknown),
        Err(R4G1Error::UnknownMandatorySection { id: 0x20 })
    );

    let mut missing = valid.clone();
    missing[88 + 6 * 16..92 + 6 * 16].copy_from_slice(&9u32.to_le_bytes());
    assert_eq!(
        R4G1Structure::parse(&missing),
        Err(R4G1Error::MissingRequiredSection { id: 8 })
    );

    let mut overlap = valid;
    let head_offset = u32::from_le_bytes([overlap[96], overlap[97], overlap[98], overlap[99]]);
    overlap[112..116].copy_from_slice(&head_offset.to_le_bytes());
    assert_eq!(
        R4G1Structure::parse(&overlap),
        Err(R4G1Error::SectionsOverlap)
    );
}

#[test]
fn r4g1_graph_view_validates_and_exposes_records_cx_09() {
    let bytes = r4g1_graph_fixture();
    let graph = R4G1Graph::parse(&bytes).expect("graph validates");
    let node = graph.node(0).expect("node is exposed");
    let edge = graph.edge(0).expect("edge is exposed");

    assert_eq!(node.child_len, 1);
    assert_eq!(node.prototype_word_start, 0);
    assert_eq!(edge.src, 0);
    assert_eq!(edge.dst, 0);
    assert_eq!(graph.node_count(), 1);
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn r4g1_graph_view_rejects_invalid_ranges_and_endpoints_cx_10() {
    let valid = r4g1_graph_fixture();

    let mut bad_node_range = valid.clone();
    bad_node_range[432..436].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        R4G1Graph::parse(&bad_node_range),
        Err(R4G1Error::NodeRangeOutOfBounds {
            node: 0,
            field: R4G1RangeField::Child,
        })
    );

    let mut bad_depth = valid.clone();
    bad_depth[460] = 1;
    assert_eq!(
        R4G1Graph::parse(&bad_depth),
        Err(R4G1Error::NodeDepthOutOfBounds { node: 0 })
    );

    let mut bad_rout = valid.clone();
    bad_rout[450..454].copy_from_slice(&8u32.to_le_bytes());
    assert_eq!(
        R4G1Graph::parse(&bad_rout),
        Err(R4G1Error::NodeRangeOutOfBounds {
            node: 0,
            field: R4G1RangeField::Prototype,
        })
    );

    let mut bad_edge = valid;
    bad_edge[468..472].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        R4G1Graph::parse(&bad_edge),
        Err(R4G1Error::EdgeEndpointOutOfBounds { edge: 0 })
    );
}

#[test]
fn typed_r4_lifecycle_entry_points_pin_and_compile_cx_11() {
    let rejected = cli_download(&DownloadRequest {
        repository: "org/model".to_owned(),
        revision: "main".to_owned(),
        output: std::env::temp_dir().join("uor-semantic-invalid-download"),
    })
    .expect_err("moving revisions are rejected before hf is invoked");
    assert!(
        rejected
            .to_string()
            .contains("immutable 40-character hexadecimal")
    );

    let root =
        std::env::temp_dir().join(format!("uor-semantic-r4-lifecycle-{}", std::process::id()));
    let observations = root.join("observations.uorobs");
    let artifact = root.join("compiled/model.uors");
    fs::create_dir_all(&root).expect("lifecycle fixture directory creates");
    fs::write(&observations, TRAIN).expect("observation fixture writes");
    let compiled = cli_compile(&CliCompileRequest {
        observations,
        output: artifact.clone(),
        config: CompilerConfig::accuracy(),
    })
    .expect("typed compile succeeds");
    assert!(artifact.is_file());
    assert_eq!(
        ArtifactView::parse(&compiled.bytes).unwrap().codebook_id(),
        compiled.codebook_id
    );
}

#[test]
fn typed_source_compile_preflights_corpus_before_teacher_bridge_cx_12() {
    let root = std::env::temp_dir().join(format!(
        "uor-semantic-source-compile-{}",
        std::process::id()
    ));
    let source = root.join("source");
    fs::create_dir_all(&source).expect("source fixture directory creates");
    fs::write(source.join("config.json"), b"{}").expect("config fixture writes");
    fs::write(source.join("tokenizer.json"), b"{}").expect("tokenizer fixture writes");
    fs::write(source.join("model.safetensors"), []).expect("weights fixture writes");

    let result = compile_source(&SourceCompileRequest {
        source_dir: source,
        model_id: "fixture/model".to_owned(),
        revision: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        corpus: root.join("missing.txt"),
        work_dir: root.join("work"),
        capture: CaptureOptions::default(),
        compiler: CompilerConfig::accuracy(),
        parity: ParityThresholds {
            exact_top1_basis_points: 10_000,
            graph_top1_basis_points: 0,
            graph_coverage_basis_points: 0,
            graph_top_k_recall_basis_points: 0,
        },
    });
    assert!(matches!(
        result,
        Err(uor_semantic_cli::CliError::Preflight(
            uor_semantic_cli::PreflightError::MissingCorpus { .. }
        ))
    ));
    assert!(!root.join("work").exists());
}

#[test]
fn r4g1_graph_view_rejects_invalid_rout_bytecode_cx_13() {
    let valid = r4g1_graph_fixture();

    let mut unknown = valid.clone();
    unknown[488] = 0x7f;
    assert_eq!(
        R4G1Graph::parse(&unknown),
        Err(R4G1Error::UnknownRoutingOp {
            offset: 0,
            opcode: 0x7f,
        })
    );

    let mut operand = valid.clone();
    operand[488] = 0x01;
    operand[489] = 1;
    assert_eq!(
        R4G1Graph::parse(&operand),
        Err(R4G1Error::RoutingOperandOutOfBounds { op_index: 0 })
    );

    let mut jump = valid.clone();
    jump[488] = 0x02;
    jump[489..491].copy_from_slice(&1u16.to_le_bytes());
    assert_eq!(
        R4G1Graph::parse(&jump),
        Err(R4G1Error::RoutingJumpOutOfBounds {
            op_index: 0,
            target: 2,
        })
    );

    let mut shortlist = valid;
    shortlist[488] = 0x03;
    shortlist[493..495].copy_from_slice(&u16::MAX.to_le_bytes());
    assert_eq!(
        R4G1Graph::parse(&shortlist),
        Err(R4G1Error::RoutingShortlistOutOfBounds { op_index: 0 })
    );
}

#[test]
fn compiled_artifact_exports_to_structural_r4g1_with_valid_cids_cx_14() {
    let compiled = compile(&train(), CompilerConfig::accuracy()).expect("fixture compiles");
    let exported = export_r4g1(&compiled).expect("structural R4G1 export succeeds");
    let repeated = export_r4g1(&compiled).expect("repeated structural export succeeds");
    assert_eq!(exported.bytes, repeated.bytes);
    let graph = R4G1Graph::parse(&exported.bytes).expect("exported graph validates");

    assert_eq!(graph.node_count(), exported.node_count);
    assert_eq!(graph.edge_count(), exported.edge_count);
    let mut refinement_edges = 0u32;
    let mut predictive_edges = 0u32;
    assert!(graph.node(0).is_some_and(|node| node.child_len > 0));
    for edge_index in 0..graph.edge_count() {
        let edge = graph.edge(edge_index).expect("exported edge exists");
        match edge.kind {
            0 => {
                refinement_edges += 1;
                assert!(edge.src < edge.dst);
            }
            2 => predictive_edges += 1,
            kind => panic!("unexpected exported edge kind {kind}"),
        }
        assert!(graph.reverse_edge_id(edge_index).is_some());
    }
    assert_eq!(refinement_edges, 3);
    assert!(predictive_edges > 0);
    assert_eq!(
        graph.identity().artifact_id().as_bytes(),
        &exported.artifact_cid
    );
    assert_eq!(graph.identity().section_count(), 8);
    verify_r4g1_cids(&exported.bytes).expect("exported CIDs verify");

    let mut tampered = exported.bytes.clone();
    tampered[24] ^= 1;
    assert!(matches!(
        verify_r4g1_cids(&tampered),
        Err(uor_semantic_compiler::R4G1ExportError::InvalidCid(
            "artifact"
        ))
    ));
}

#[test]
fn cli_compile_writes_optional_structural_r4g1_container_cx_15() {
    let root = std::env::temp_dir().join(format!("uor-semantic-cli-r4g1-{}", std::process::id()));
    let observations = root.join("observations.uorobs");
    let artifact = root.join("compiled/model.uors");
    let r4g1 = root.join("compiled/model.r4g1");
    fs::create_dir_all(&root).expect("CLI fixture directory creates");
    fs::write(&observations, TRAIN).expect("observation fixture writes");

    let arguments = [
        "compile".to_owned(),
        observations.display().to_string(),
        "--output".to_owned(),
        artifact.display().to_string(),
        "--r4g1-output".to_owned(),
        r4g1.display().to_string(),
    ];
    cli_run(&arguments).expect("CLI compile with R4G1 output succeeds");

    assert!(artifact.is_file());
    assert!(r4g1.is_file());
    let bytes = fs::read(&r4g1).expect("R4G1 output reads");
    let graph = R4G1Graph::parse(&bytes).expect("CLI R4G1 output validates");
    verify_r4g1_cids(&bytes).expect("CLI R4G1 CIDs verify");
    assert_eq!(graph.node_count(), 4);

    let compiled = cli_compile(&CliCompileRequest {
        observations,
        output: root.join("typed/model.uors"),
        config: CompilerConfig::accuracy(),
    })
    .expect("typed compile succeeds");
    let typed_output = root.join("typed/model.r4g1");
    let typed_export =
        cli_export_r4g1(&compiled, &typed_output).expect("typed R4G1 export succeeds");
    assert_eq!(
        fs::read(typed_output).expect("typed R4G1 output reads"),
        typed_export.bytes
    );
}

#[test]
fn r4g1_graph_view_validates_edge_flags_and_reverse_ids_cx_16() {
    let valid = r4g1_graph_fixture();

    let mut flags = valid.clone();
    flags[477] = 1;
    assert_eq!(
        R4G1Graph::parse(&flags),
        Err(R4G1Error::EdgeFlagsInvalid { edge: 0 })
    );

    let mut reverse = valid;
    reverse[480..484].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        R4G1Graph::parse(&reverse),
        Err(R4G1Error::ReverseIndexOutOfBounds {
            index: 0,
            edge_id: 1,
        })
    );
}

#[test]
fn r4g1_export_emits_root_prior_and_rx1_exct_cx_17() {
    let compiled = compile(&train(), CompilerConfig::accuracy()).expect("fixture compiles");
    let exported = export_r4g1(&compiled).expect("structural R4G1 export succeeds");
    let graph = R4G1Graph::parse(&exported.bytes).expect("exported graph validates");

    let emit = graph.section(R4G1Section::Emit).expect("EMIT exists");
    assert_eq!(&emit[..4], &[2, 0, 0, 0]);
    assert!(u32::from_le_bytes([emit[4], emit[5], emit[6], emit[7]]) > 0);
    assert!(u32::from_le_bytes([emit[8], emit[9], emit[10], emit[11]]) > 0);
    assert_eq!(&emit[16..20], &[0, 0, 0, 0]);

    let exct = graph.section(R4G1Section::Exct).expect("EXCT exists");
    assert_eq!(&exct[..4], &[2, 0, 0, 0]);
    assert_eq!(&exct[4..8], b"RX1\0");
    assert_eq!(exct[8], 5);

    let mut malformed = exported.bytes.clone();
    let section_count =
        u32::from_le_bytes([malformed[16], malformed[17], malformed[18], malformed[19]]);
    let mut cursor = 88usize;
    let mut exct_offset = None;
    let mut section = 0u32;
    while section < section_count {
        let id = u32::from_le_bytes([
            malformed[cursor],
            malformed[cursor + 1],
            malformed[cursor + 2],
            malformed[cursor + 3],
        ]);
        if id == R4G1Section::Exct.raw() {
            exct_offset = Some(u32::from_le_bytes([
                malformed[cursor + 8],
                malformed[cursor + 9],
                malformed[cursor + 10],
                malformed[cursor + 11],
            ]) as usize);
            break;
        }
        cursor += 16;
        section += 1;
    }
    let exct_offset = exct_offset.expect("EXCT table entry exists");
    malformed[exct_offset + 8] = 4;
    assert_eq!(R4G1Graph::parse(&malformed), Err(R4G1Error::InvalidExct));
}

#[test]
fn r4g1_scoring_accumulates_unique_residuals_with_deterministic_ties_cx_18() {
    let mut accumulator = ScoreAccumulator::<2>::new();
    assert!(
        accumulator
            .accumulate(ResidualContribution {
                kind: ResidualContributionKind::RootPrior,
                contribution_id: 10,
                raw_value: i32::MAX - 5,
            })
            .expect("first contribution fits")
    );
    assert!(
        !accumulator
            .accumulate(ResidualContribution {
                kind: ResidualContributionKind::InteractionResidual,
                contribution_id: 10,
                raw_value: 500,
            })
            .expect("duplicate contribution is ignored")
    );
    assert_eq!(accumulator.score(), i32::MAX - 5);

    assert!(
        accumulator
            .accumulate(ResidualContribution {
                kind: ResidualContributionKind::TokenEmission,
                contribution_id: 11,
                raw_value: 10,
            })
            .expect("second contribution fits")
    );
    assert_eq!(accumulator.score(), i32::MAX);
    assert_eq!(accumulator.evidence_count(), 2);
    assert_eq!(
        accumulator.accumulate(ResidualContribution {
            kind: ResidualContributionKind::ConstraintPenalty,
            contribution_id: 12,
            raw_value: -1,
        }),
        Err(ScoringError::EvidenceCapacityExceeded)
    );

    let mut negative = ScoreAccumulator::<2>::new();
    negative
        .accumulate(ResidualContribution {
            kind: ResidualContributionKind::ConstraintPenalty,
            contribution_id: 20,
            raw_value: i32::MIN + 1,
        })
        .expect("negative contribution fits");
    negative
        .accumulate(ResidualContribution {
            kind: ResidualContributionKind::UncertaintyPenalty,
            contribution_id: 21,
            raw_value: -10,
        })
        .expect("negative saturation contribution fits");
    assert_eq!(negative.score(), i32::MIN);

    assert_eq!(
        ScoreAccumulator::<2>::compare_candidates(50, 3, 50, 4),
        core::cmp::Ordering::Less
    );
    assert_eq!(
        ScoreAccumulator::<2>::compare_candidates(51, 9, 50, 1),
        core::cmp::Ordering::Less
    );
}

#[test]
fn r4g1_export_emits_predictive_edges_with_refinement_ranges_cx_19() {
    let compiled = compile(&train(), CompilerConfig::accuracy()).expect("fixture compiles");
    let exported = export_r4g1(&compiled).expect("structural R4G1 export succeeds");
    let graph = R4G1Graph::parse(&exported.bytes).expect("exported graph validates");
    let mut predictive = 0u32;
    for edge_index in 0..graph.edge_count() {
        let edge = graph.edge(edge_index).expect("edge exists");
        if edge.kind == 2 {
            predictive += 1;
            assert!(graph.reverse_edge_id(edge_index).is_some());
        }
    }
    assert!(predictive > 0);

    for node_index in 0..graph.node_count() {
        let node = graph.node(node_index).expect("node exists");
        let end = node.child_start + u32::from(node.child_len);
        let mut edge_index = node.child_start;
        while edge_index < end {
            let edge = graph.edge(edge_index).expect("child edge exists");
            assert_eq!(edge.kind, 0);
            assert_eq!(edge.src, node_index);
            edge_index += 1;
        }
    }
}

#[test]
fn r4g1_replay_certificate_reports_predictive_score_agreement_cx_20() {
    let compiled = compile(&train(), CompilerConfig::accuracy()).expect("fixture compiles");
    let exported = export_r4g1(&compiled).expect("structural R4G1 export succeeds");
    let report = replay_r4g1(&compiled.bytes, &exported.bytes).expect("replay evaluates");

    assert!(report.expected_transitions > 0);
    assert_eq!(report.expected_transitions, report.emitted_predictive_edges);
    assert_eq!(report.expected_transitions, report.matched_transitions);
    assert_eq!(report.expected_transitions, report.score_matches);
    assert_eq!(report.score_agreement_basis_points(), 10_000);
    assert!(report.is_complete());

    let mut tampered = exported.bytes.clone();
    let section_count = u32::from_le_bytes(tampered[16..20].try_into().expect("header field"));
    let mut section_index = 0u32;
    let mut edge_offset = None;
    while section_index < section_count {
        let table = 88 + section_index as usize * 16;
        let id = u32::from_le_bytes(tampered[table..table + 4].try_into().expect("section ID"));
        if id == 4 {
            edge_offset = Some(u32::from_le_bytes(
                tampered[table + 8..table + 12]
                    .try_into()
                    .expect("section offset"),
            ) as usize);
            break;
        }
        section_index += 1;
    }
    let edge_offset = edge_offset.expect("EDGE section exists");
    let mut edge_index = 0usize;
    while tampered[edge_offset + edge_index * 16 + 12] != 2 {
        edge_index += 1;
    }
    let score = edge_offset + edge_index * 16 + 8;
    tampered[score..score + 4].copy_from_slice(&i32::MAX.to_le_bytes());
    let tampered_report = replay_r4g1(&compiled.bytes, &tampered).expect("tampered graph parses");
    assert_eq!(
        tampered_report.expected_transitions,
        report.expected_transitions
    );
    assert_eq!(
        tampered_report.matched_transitions,
        report.matched_transitions
    );
    assert_eq!(tampered_report.score_matches, 0);
    assert!(!tampered_report.is_complete());
}

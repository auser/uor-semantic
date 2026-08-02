//! Allocation census for the strict routing call.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use repo_conformance::{AMBIGUOUS_STOP, REGIONS};
use uor_semantic::{CandidateSet, OperationCensus, ReferenceRouter, RouteCloud};

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static TEST_LOCK: Mutex<()> = Mutex::new(());

struct CountingAllocator;

// SAFETY: every request is forwarded unchanged to `System`; the atomics only
// observe call counts and do not alter pointer or layout semantics.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwarding the caller-provided layout to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: `pointer` and `layout` are forwarded to the allocator that
        // produced the allocation.
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn warmed_routing_performs_zero_heap_operations_rt_02() {
    let _guard = TEST_LOCK.lock().expect("allocator test lock");
    let candidates = CandidateSet::<1, 3>::new(&REGIONS).expect("unique fixture regions");
    let mut cloud = RouteCloud::<3>::new();
    let mut census = OperationCensus::new();

    let _ = ReferenceRouter::route(&AMBIGUOUS_STOP, candidates, &mut cloud, &mut census);

    let allocations_before = ALLOCATIONS.load(Ordering::SeqCst);
    let deallocations_before = DEALLOCATIONS.load(Ordering::SeqCst);
    for _iteration in 0..1_024 {
        let _ = black_box(ReferenceRouter::route(
            black_box(&AMBIGUOUS_STOP),
            candidates,
            &mut cloud,
            &mut census,
        ));
    }
    let allocations_after = ALLOCATIONS.load(Ordering::SeqCst);
    let deallocations_after = DEALLOCATIONS.load(Ordering::SeqCst);

    assert_eq!(allocations_after, allocations_before);
    assert_eq!(deallocations_after, deallocations_before);
}

#[test]
fn artifact_parse_predict_and_generation_perform_zero_heap_operations_rt_03() {
    let _guard = TEST_LOCK.lock().expect("allocator test lock");
    use uor_semantic::{
        ArtifactPredictScratch, ArtifactView, ExactPolicy, GenerationState, Prediction,
        generate_greedy_into,
    };
    use uor_semantic_compiler::{CompilerConfig, ObservationCorpus, compile};

    let corpus = ObservationCorpus::parse(concat!(
        "UOROBS1\n",
        "model=fixture/model\n",
        "revision=0123456789abcdef0123456789abcdef01234567\n",
        "source_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "max_context=4\n",
        "top_k=2\n",
        "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000002\n",
        "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000003\n",
        "eos_token=2\n",
        "--\n",
        "O|1,2|3|3:0,4:-10\n",
        "O|1,2,3|4|4:0,5:-10\n",
    ))
    .expect("fixture parses");
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let artifact = ArtifactView::parse(&compiled.bytes).expect("artifact validates");
    let mut state = GenerationState::<4>::new().expect("state capacity is valid");
    let mut output = [0u32; 2];
    let mut scratch = ArtifactPredictScratch::<8>::new();
    let mut prediction = Prediction::<8>::new();

    state.seed(&[1, 2]);
    let _ = generate_greedy_into(
        &artifact,
        &mut state,
        &mut output,
        &mut scratch,
        &mut prediction,
    )
    .expect("warm generation succeeds");

    let allocations_before = ALLOCATIONS.load(Ordering::SeqCst);
    let deallocations_before = DEALLOCATIONS.load(Ordering::SeqCst);
    for _iteration in 0..1_024 {
        let parsed = ArtifactView::parse(black_box(&compiled.bytes)).expect("parse succeeds");
        let _ = parsed
            .predict(
                black_box(&[1, 2]),
                ExactPolicy::PreferExact,
                &mut scratch,
                &mut prediction,
            )
            .expect("predict succeeds");
        state.seed(&[1, 2]);
        let _ = generate_greedy_into(
            &parsed,
            &mut state,
            &mut output,
            &mut scratch,
            &mut prediction,
        )
        .expect("generation succeeds");
    }
    let allocations_after = ALLOCATIONS.load(Ordering::SeqCst);
    let deallocations_after = DEALLOCATIONS.load(Ordering::SeqCst);

    assert_eq!(allocations_after, allocations_before);
    assert_eq!(deallocations_after, deallocations_before);
}

#[test]
fn r4g1_route_and_emit_perform_zero_heap_operations_cx_21() {
    let _guard = TEST_LOCK.lock().expect("allocator test lock");
    use uor_semantic::{R4G1Emissions, R4G1Graph, R4G1RouteCandidates, context_signature};
    use uor_semantic_compiler::{CompilerConfig, ObservationCorpus, compile, export_r4g1};

    let corpus = ObservationCorpus::parse(concat!(
        "UOROBS1\n",
        "model=fixture/model\n",
        "revision=0123456789abcdef0123456789abcdef01234567\n",
        "source_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "max_context=4\n",
        "top_k=2\n",
        "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000002\n",
        "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000003\n",
        "eos_token=2\n",
        "--\n",
        "O|1,2|3|3:0,4:-10\n",
        "O|1,2,3|4|4:0,5:-10\n",
    ))
    .expect("fixture parses");
    let compiled = compile(&corpus, CompilerConfig::accuracy()).expect("fixture compiles");
    let exported = export_r4g1(&compiled).expect("R4G1 export succeeds");
    let graph = R4G1Graph::parse(&exported.bytes).expect("R4G1 graph validates");
    let signature = context_signature(&[1, 2]);
    let mut candidates = R4G1RouteCandidates::<8>::new();
    let mut emissions = R4G1Emissions::<4>::new();

    graph
        .route(&signature, &mut candidates)
        .expect("route warms");
    let node = *candidates.as_slice().first().expect("route returns a node");
    graph
        .node_emissions(node, &mut emissions)
        .expect("emissions warm");

    let allocations_before = ALLOCATIONS.load(Ordering::SeqCst);
    let deallocations_before = DEALLOCATIONS.load(Ordering::SeqCst);
    for _iteration in 0..1_024 {
        graph
            .route(black_box(&signature), &mut candidates)
            .expect("route succeeds");
        graph
            .node_emissions(black_box(node), &mut emissions)
            .expect("emissions succeed");
        black_box(candidates.as_slice());
        black_box(emissions.as_slice());
    }
    let allocations_after = ALLOCATIONS.load(Ordering::SeqCst);
    let deallocations_after = DEALLOCATIONS.load(Ordering::SeqCst);

    assert_eq!(allocations_after, allocations_before);
    assert_eq!(deallocations_after, deallocations_before);
}

//! Strict-core source and deterministic work conformance.

use std::path::PathBuf;

use repo_conformance::{AMBIGUOUS_STOP, audit_strict_core, route_fixture};
use uor_semantic::{OperationBudget, OperationCensus, RouteCloud};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/conformance is two levels below the repository root")
        .to_path_buf()
}

#[test]
fn strict_core_has_no_dependency_heap_float_or_product_escape_rt_01() {
    let report = audit_strict_core(&root()).expect("the strict-core audit runs");
    assert!(
        report.is_clean(),
        "strict-core violations:\n{}",
        report.violations.join("\n")
    );
    assert!(report.files_scanned >= 8, "the source audit must be armed");
}

#[test]
fn route_work_stays_within_declared_operation_budget_pf_01() {
    let budget = OperationBudget {
        candidate_scans: 3,
        word_steps: 3,
        xors: 3,
        ands: 3,
        popcounts: 3,
        distance_adds: 3,
        comparisons: 6,
        table_reads: 9,
        table_writes: 4,
    };
    let mut expected = None;

    for _run in 0..32 {
        let mut cloud = RouteCloud::<3>::new();
        let mut census = OperationCensus::new();
        let summary = route_fixture(&AMBIGUOUS_STOP, &mut cloud, &mut census)
            .expect("the checked fixture is valid");
        assert_eq!(summary.candidates_scanned(), 3);
        assert_eq!(summary.retained(), cloud.len());
        assert!(!summary.was_truncated());
        assert!(budget.permits(&census));

        if let Some(previous) = expected {
            assert_eq!(census, previous);
        } else {
            expected = Some(census);
        }
    }
}

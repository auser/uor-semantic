//! Empirical report shape and interpretation.

use repo_conformance::{BenchmarkConfig, run_route_benchmark};

#[test]
fn benchmark_reports_speed_without_claiming_a_universal_threshold_pf_02() {
    let config = BenchmarkConfig::new(3, 256).expect("non-zero benchmark dimensions");
    let report = run_route_benchmark(config).expect("the benchmark fixture runs");

    assert_eq!(report.samples, 3);
    assert_eq!(report.iterations_per_sample, 256);
    assert_eq!(report.total_routes, 768);
    assert!(report.min_ns_per_route <= report.median_ns_per_route);
    assert!(report.median_ns_per_route <= report.max_ns_per_route);
    assert_eq!(report.candidates_per_route, 3);
    assert_eq!(report.words_per_route, 3);
    assert!(report.to_json().contains("median_ns_per_route"));
}

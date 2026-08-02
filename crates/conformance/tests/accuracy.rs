//! Deterministic contextual-disambiguation fixture.

use repo_conformance::{
    AMBIGUOUS_STOP, BUS_STOP, STOP_THE_CAR, TRANSIT_STOP_REGION, VEHICLE_STOP_REGION, route_fixture,
};
use uor_semantic::{OperationCensus, RegionId, RouteCloud};

#[test]
fn contextual_stop_fixture_routes_every_declared_case_ac_01() {
    let cases: [([u64; 1], RegionId, usize); 3] = [
        (AMBIGUOUS_STOP, VEHICLE_STOP_REGION, 3),
        (STOP_THE_CAR, VEHICLE_STOP_REGION, 1),
        (BUS_STOP, TRANSIT_STOP_REGION, 1),
    ];

    let mut correct = 0usize;
    for (code, expected_top, expected_active) in cases {
        let mut cloud = RouteCloud::<3>::new();
        let mut census = OperationCensus::new();
        let summary =
            route_fixture(&code, &mut cloud, &mut census).expect("the checked fixture is valid");
        let observed_top = cloud.first().map(|membership| membership.region_id());
        if observed_top == Some(expected_top) && summary.retained() == expected_active {
            correct += 1;
        }
    }

    assert_eq!(correct, cases.len(), "all pinned fixture cases must pass");
}

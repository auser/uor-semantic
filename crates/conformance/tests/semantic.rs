//! Semantic routing, path, and boundary behavior.

use core::convert::TryInto as _;

use repo_conformance::{
    AMBIGUOUS_STOP, REGIONS, TRANSIT_STOP_REGION, VEHICLE_STOP_REGION, route_fixture,
};
use uor_semantic::{
    AddressedPath, CodebookId, Depth, MembershipMargin, OperationCensus, PathId, ReferenceRouter,
    Region, RegionId, ResolutionStatus, RouteCloud, SemanticAddressBundle, SemanticPath,
    SemanticSlot, masked_hamming,
};

#[test]
fn one_context_can_retain_overlapping_meanings_sr_01() {
    let mut cloud = RouteCloud::<3>::new();
    let mut census = OperationCensus::new();
    let summary = route_fixture(&AMBIGUOUS_STOP, &mut cloud, &mut census)
        .expect("the checked fixture is valid");

    assert_eq!(summary.resolution(), ResolutionStatus::Boundary);
    assert_eq!(summary.matched(), 3);
    assert_eq!(summary.retained(), 3);
    assert_eq!(summary.truncated(), 0);

    let ids: Vec<_> = cloud
        .as_slice()
        .iter()
        .map(|membership| membership.region_id())
        .collect();
    assert!(ids.contains(&RegionId::new(1)));
    assert!(ids.contains(&RegionId::new(2)));
    assert!(ids.contains(&RegionId::new(3)));
}

#[test]
fn candidate_order_does_not_change_canonical_output_sr_02() {
    let permutations = [
        [REGIONS[0], REGIONS[1], REGIONS[2]],
        [REGIONS[0], REGIONS[2], REGIONS[1]],
        [REGIONS[1], REGIONS[0], REGIONS[2]],
        [REGIONS[1], REGIONS[2], REGIONS[0]],
        [REGIONS[2], REGIONS[0], REGIONS[1]],
        [REGIONS[2], REGIONS[1], REGIONS[0]],
    ];

    let mut first_cloud = RouteCloud::<3>::new();
    let mut first_census = OperationCensus::new();
    let first_candidates =
        uor_semantic::CandidateSet::<1, 3>::new(&permutations[0]).expect("unique fixture regions");
    let _ = ReferenceRouter::route(
        &AMBIGUOUS_STOP,
        first_candidates,
        &mut first_cloud,
        &mut first_census,
    );
    let expected: [uor_semantic::RegionMembership; 3] = first_cloud
        .as_slice()
        .try_into()
        .expect("three fixture memberships");

    for regions in &permutations[1..] {
        let candidates =
            uor_semantic::CandidateSet::<1, 3>::new(regions).expect("unique fixture regions");
        let mut cloud = RouteCloud::<3>::new();
        let mut census = OperationCensus::new();
        let _ = ReferenceRouter::route(&AMBIGUOUS_STOP, candidates, &mut cloud, &mut census);
        assert_eq!(cloud.as_slice(), &expected[..]);
    }
}

#[test]
fn all_initialized_prefixes_are_resolvable_sp_01() {
    let mut cessation = SemanticPath::<4>::new();
    for slot in [17, 42, 8, 3] {
        cessation
            .push(SemanticSlot::new(slot))
            .expect("four-slot fixture fits");
    }

    for depth in 0..=cessation.len() {
        let prefix = cessation.prefix(depth).expect("initialized prefix");
        assert_eq!(prefix.len(), depth);
        assert_eq!(prefix.slots(), &cessation.as_slice()[..depth]);
    }

    let mut transit = SemanticPath::<4>::new();
    for slot in [9, 31, 6, 2] {
        transit
            .push(SemanticSlot::new(slot))
            .expect("four-slot fixture fits");
    }

    let mut bundle = SemanticAddressBundle::<2, 4>::new(CodebookId::from_bytes([7; 32]));
    bundle
        .insert(AddressedPath::new(cessation, MembershipMargin::new(7)))
        .expect("first overlapping path fits");
    bundle
        .insert(AddressedPath::new(transit, MembershipMargin::new(3)))
        .expect("second overlapping path fits");

    assert_eq!(bundle.len(), 2);
    assert_eq!(bundle.paths()[0].path(), &cessation);
    assert_eq!(bundle.paths()[1].path(), &transit);
}

#[test]
fn masked_hamming_boundaries_are_inclusive_rh_01() {
    assert_eq!(masked_hamming(&[0b1010], &[0b0011], &[0b1111]), 2);
    assert_eq!(masked_hamming(&[0b1010], &[0b0011], &[0b0011]), 1);

    let regions = [Region::new(
        RegionId::new(99),
        PathId::new(199),
        Depth::new(2),
        [0b0011],
        [0b0011],
        1,
    )];
    let candidates = uor_semantic::CandidateSet::<1, 1>::new(&regions).expect("one unique region");
    let mut cloud = RouteCloud::<1>::new();
    let mut census = OperationCensus::new();
    let summary = ReferenceRouter::route(&[0b1010], candidates, &mut cloud, &mut census);

    assert_eq!(summary.resolution(), ResolutionStatus::Supported);
    assert_eq!(cloud.first().map(|entry| entry.distance()), Some(1));
    assert_eq!(
        cloud.first().map(|entry| entry.margin()),
        Some(MembershipMargin::new(0))
    );
}

#[test]
fn bounded_cloud_reports_truncation_rc_01() {
    let mut cloud = RouteCloud::<2>::new();
    let mut census = OperationCensus::new();
    let summary = route_fixture(&AMBIGUOUS_STOP, &mut cloud, &mut census)
        .expect("the checked fixture is valid");

    assert_eq!(summary.matched(), 3);
    assert_eq!(summary.retained(), 2);
    assert_eq!(summary.truncated(), 1);
    assert!(summary.was_truncated());
    assert_eq!(summary.resolution(), ResolutionStatus::Boundary);
    assert_eq!(cloud.as_slice()[0].region_id(), VEHICLE_STOP_REGION);
    assert_eq!(cloud.as_slice()[1].region_id(), TRANSIT_STOP_REGION);
}

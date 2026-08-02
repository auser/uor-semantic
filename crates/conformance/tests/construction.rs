//! Construction failures are typed, bounded, and atomic.

use repo_conformance::{AMBIGUOUS_STOP, REGIONS, route_fixture};
use uor_semantic::{
    AddressInsertError, AddressedPath, CandidateSet, CandidateSetError, CodebookId,
    MembershipMargin, OperationCensus, PathError, RouteCloud, SemanticAddressBundle, SemanticPath,
    SemanticSlot,
};

#[test]
fn path_capacity_and_prefix_failures_preserve_state() {
    let mut path = SemanticPath::<1>::new();
    path.push(SemanticSlot::new(7)).expect("first slot fits");
    let before = path;

    assert_eq!(
        path.push(SemanticSlot::new(8)),
        Err(PathError::CapacityExceeded { capacity: 1 })
    );
    assert_eq!(path, before);
    assert_eq!(
        path.prefix(2),
        Err(PathError::PrefixOutOfBounds {
            requested: 2,
            available: 1,
        })
    );
    assert_eq!(path, before);
}

#[test]
fn address_duplicate_and_capacity_failures_preserve_state() {
    let mut first = SemanticPath::<1>::new();
    first.push(SemanticSlot::new(1)).expect("first path fits");
    let mut second = SemanticPath::<1>::new();
    second.push(SemanticSlot::new(2)).expect("second path fits");

    let mut bundle = SemanticAddressBundle::<1, 1>::new(CodebookId::from_bytes([9; 32]));
    let first_entry = AddressedPath::new(first, MembershipMargin::new(4));
    bundle.insert(first_entry).expect("first entry fits");
    let before = bundle;

    assert_eq!(
        bundle.insert(first_entry),
        Err(AddressInsertError::DuplicatePath)
    );
    assert_eq!(bundle, before);
    assert_eq!(
        bundle.insert(AddressedPath::new(second, MembershipMargin::new(5))),
        Err(AddressInsertError::CapacityExceeded { capacity: 1 })
    );
    assert_eq!(bundle, before);
}

#[test]
fn candidate_set_rejects_excess_and_duplicate_regions() {
    assert_eq!(
        CandidateSet::<1, 1>::new(&REGIONS[..2]).unwrap_err(),
        CandidateSetError::CapacityExceeded {
            provided: 2,
            maximum: 1,
        }
    );

    let duplicate = [REGIONS[0], REGIONS[0]];
    assert_eq!(
        CandidateSet::<1, 2>::new(&duplicate).unwrap_err(),
        CandidateSetError::DuplicateRegion {
            region_id: REGIONS[0].id(),
        }
    );
}

#[test]
fn zero_capacity_cloud_reports_every_match_as_truncated() {
    let mut cloud = RouteCloud::<0>::new();
    let mut census = OperationCensus::new();
    let summary = route_fixture(&AMBIGUOUS_STOP, &mut cloud, &mut census)
        .expect("the checked fixture is valid");

    assert_eq!(summary.matched(), 3);
    assert_eq!(summary.retained(), 0);
    assert_eq!(summary.truncated(), 3);
    assert!(summary.was_truncated());
    assert!(cloud.is_empty());
}

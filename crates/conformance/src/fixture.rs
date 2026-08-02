//! Small deterministic semantic fixture used by conformance tests.

use core::fmt;

use uor_semantic::{
    CandidateSet, CandidateSetError, Depth, OperationCensus, PathId, ReferenceRouter, Region,
    RegionId, RouteCloud, RouteSummary,
};

/// Packed context for the context-free word “stop”.
pub const AMBIGUOUS_STOP: [u64; 1] = [0b0010];
/// Packed context for “stop the car”.
pub const STOP_THE_CAR: [u64; 1] = [0b0000];
/// Packed context for “wait at the bus stop”.
pub const BUS_STOP: [u64; 1] = [0b1111];

/// Region identifier for physical-motion inhibition.
pub const VEHICLE_STOP_REGION: RegionId = RegionId::new(1);
/// Region identifier for an imperative command.
pub const COMMAND_REGION: RegionId = RegionId::new(2);
/// Region identifier for a transportation location.
pub const TRANSIT_STOP_REGION: RegionId = RegionId::new(3);

/// Three deliberately overlapping regions for the word “stop”.
///
/// The values are a conformance fixture, not a learned production codebook.
pub const REGIONS: [Region<1>; 3] = [
    Region::new(
        VEHICLE_STOP_REGION,
        PathId::new(101),
        Depth::new(4),
        [0b0000],
        [0b1111],
        2,
    ),
    Region::new(
        COMMAND_REGION,
        PathId::new(102),
        Depth::new(3),
        [0b0011],
        [0b1111],
        1,
    ),
    Region::new(
        TRANSIT_STOP_REGION,
        PathId::new(103),
        Depth::new(4),
        [0b1111],
        [0b1111],
        3,
    ),
];

/// Failure to run the checked fixture router.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixtureRouteError(CandidateSetError);

impl fmt::Display for FixtureRouteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid conformance fixture: {}", self.0)
    }
}

impl std::error::Error for FixtureRouteError {}

/// Routes one fixture code into caller-owned storage.
pub fn route_fixture<const MAX_ACTIVE: usize>(
    code: &[u64; 1],
    cloud: &mut RouteCloud<MAX_ACTIVE>,
    census: &mut OperationCensus,
) -> Result<RouteSummary, FixtureRouteError> {
    let candidates = CandidateSet::<1, 3>::new(&REGIONS).map_err(FixtureRouteError)?;
    Ok(ReferenceRouter::route(code, candidates, cloud, census))
}

//! Conformance support for `uor-semantic`.
//!
//! This crate is development and CI infrastructure. It deliberately uses
//! `std` and allocation so the shipped `uor-semantic` crate does not have to.

#![deny(missing_docs)]

pub mod benchmark;
pub mod fixture;
pub mod meta;
pub mod runner;
pub mod source_audit;

pub use benchmark::{BenchmarkConfig, BenchmarkError, BenchmarkReport, run_route_benchmark};
pub use fixture::{
    AMBIGUOUS_STOP, BUS_STOP, COMMAND_REGION, FixtureRouteError, REGIONS, STOP_THE_CAR,
    TRANSIT_STOP_REGION, VEHICLE_STOP_REGION, route_fixture,
};
pub use meta::{HonestyReport, check_honesty};
pub use runner::{Scenario, SuiteReport, scenarios_in};
pub use source_audit::{SourceAuditError, SourceAuditReport, audit_strict_core};

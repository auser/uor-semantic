# Security policy

Please report security-sensitive defects privately to the UOR Foundation rather
than opening a public issue containing exploit details.

The shipped crate forbids unsafe Rust, does not import `alloc`, and validates
caller-controlled capacities before repeated routing work. These are scoped
construction properties of this repository, not statements about dependencies
or applications that embed it.

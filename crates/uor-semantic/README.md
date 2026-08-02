# uor-semantic

`uor-semantic` is the strict runtime substrate for divisible semantic addresses
and overlapping semantic-region routing.

The crate is:

- `no_std` and does not import `alloc`;
- dependency-free;
- safe Rust only;
- fixed-capacity and caller-owned;
- deterministic under a documented total order;
- limited to bitwise operations, population count, integer addition and
  subtraction, comparisons, bounded branches, and table access in its routing
  kernel.

The repository-level README describes the architecture, conformance model, and
benchmark workflow.

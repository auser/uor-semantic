# AGENTS

This file is the standing brief for humans and agents changing this repository.

## Rules

| Rule | Requirement | Gate |
| --- | --- | --- |
| R1 | `model/*.toml` is the single source of every registered claim; `CONFORMANCE.md` is generated. | `cargo xtask check-model` |
| R2 | Every claim is `some-true`, `build`, or `open`, and prose respects that level. | BDD honesty meta-gate |
| R3 | A capability begins as a register row, then a Gherkin scenario, then a failing Rust test named for the ID. | `just bdd` |
| R4 | No unfinished capability marker or inert implementation stub ships. | `cargo xtask audit-deferral` |
| R5 | The published core remains safe, dependency-free, no_std, heapless, fixed-point, and bounded. | `cargo xtask audit-core`, Clippy, tests |
| R6 | Shipped dependencies are deliberate, versioned, licensed, and advisory-clean. | `cargo deny` |
| R7 | Portable performance claims use deterministic work counts; elapsed time is reported with its hardware and sample context. | `PF-01`, `PF-02` |

## Adding behavior

Use this order:

1. add a row to `model/ids.toml` with its honesty level;
2. add a tagged scenario under `features/suites/`;
3. add a failing test whose name ends in the lowercased ID, replacing `-` with
   `_`;
4. implement the smallest complete behavior;
5. regenerate `CONFORMANCE.md` with `just model-write`;
6. run `just vv` and `just deny`.

There is no pending scenario state. A behavior that is not part of the current
crate is a non-goal, not an inert switch or an empty API.

## Strict-core boundary

Code under `crates/uor-semantic/src/` owes the following contract:

- no `std` or `alloc` import;
- no runtime dependency;
- no unsafe Rust;
- no heap-backed collection or formatting allocation;
- no floating-point type;
- no product, quotient, or remainder operator in production Rust source;
- no panic, unwrapping, or expectation in callable production paths;
- caller-owned or const-generic capacity for every bounded output;
- typed, documented errors for construction failures;
- canonical deterministic ordering where more than one output is valid;
- `debug_assert!` for internal invariants already made safe by public checks.

The conformance and tooling crates may use `std`, allocation, timing, and the
small amount of documented unsafe code required by the test-only counting
allocator. None may become a dependency of the published crate.

## Performance work

Optimize only against a pinned behavior oracle. Preserve the scalar reference
implementation, compare exact outputs, and record both deterministic work and
hardware-specific measurements. A faster implementation that changes ordering,
boundary inclusion, truncation, or resolution status is a semantic change.

Do not put an absolute nanosecond threshold on an unpinned shared CI runner.
Use operation budgets in ordinary CI and compare empirical reports on named
hardware classes.

## Comments and documentation

Document contracts at the API boundary: allocation behavior, capacity,
complexity, ordering, determinism, errors, and identity assumptions. Comments
inside functions should explain why a choice was made, not translate the code
into prose.

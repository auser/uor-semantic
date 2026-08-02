# Changelog

## 0.1.0

- Adds fixed-capacity divisible semantic paths.
- Adds bounded overlapping route clouds with deterministic ordering.
- Adds masked-Hamming region routing and an operation census.
- Adds a zero-copy `.uors` artifact reader with exact and graph prediction lanes.
- Adds fixed-capacity greedy generation into caller-owned output.
- Adds a deterministic offline compiler and parity evaluator.
- Adds a dependency-free Rust CLI for artifact inspection, prediction,
  generation, Hugging Face download orchestration, observation capture,
  compilation, parity reporting, and end-to-end builds.
- Adds BDD, construction, contextual-accuracy, exact-parity, graph-only,
  deterministic-build, no-allocation, operation-budget, and performance tests.
- Pins Rust 1.97.1 and Rust edition 2024.
- Verifies host and embedded-target `no_std` compilation.

### Parity scope

- Exact top-1 agreement is certified only for captured observations.
- Graph-only measurements disable exact lookup.
- Hugging Face capture is teacher-forced over a supplied corpus with a bounded
  context of at most 32 tokens.

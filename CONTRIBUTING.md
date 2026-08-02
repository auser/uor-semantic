# Contributing

Read `AGENTS.md` before making a change. A capability is added in this order:

1. register its conformance ID in `model/ids.toml`;
2. add its Gherkin scenario under `features/suites/`;
3. add a failing test whose name ends in the lowercased ID;
4. implement the behavior;
5. run `just vv`.

Public runtime APIs must make ownership, capacity, allocation, error, ordering,
and determinism contracts visible. Core code remains `no_std`, no-heap, and
safe Rust.

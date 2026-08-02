# `just vv` is the normative acceptance gate.
default: vv

vv: fmt-check model lint check test doc release-assertions no-alloc bdd cli-self-test
    @echo "vv: the acceptance gate passed"

model:
    cargo run --locked -q -p xtask -- validate

model-write:
    cargo run --locked -q -p xtask -- check-model --write

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

check:
    cargo check -p uor-semantic --no-default-features --locked
    cargo check -p uor-semantic --target thumbv7em-none-eabihf --no-default-features --locked
    cargo check --workspace --all-features --all-targets --locked

test:
    cargo test --workspace --locked -- --test-threads=1

doc:
    cargo test --workspace --doc --locked

release-assertions:
    cargo test --workspace --profile release-assertions --locked -- --test-threads=1

no-alloc:
    cargo test -p repo-conformance --test no_alloc --locked -- --test-threads=1

bdd:
    cargo test -p repo-conformance --test bdd --locked

bench samples="31" iterations="10000":
    cargo run --release --locked -p repo-conformance --example bench_route -- {{samples}} {{iterations}}

deny:
    cargo deny --all-features check

cli-self-test:
    cargo run --locked -p uor-semantic-cli --bin uor-semantic -- self-test

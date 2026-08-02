# UOR Semantic — Code-Aware Session Handoff

You are working inside the actual `uor-semantic` Rust workspace. Treat the repository contents and command output as the source of truth. Do not rely on prior architectural descriptions when they conflict with the code.

## Working rules

1. Read `AGENTS.md`, `README.md`, `ARCHITECTURE.md`, `CONFORMANCE.md`, `VALIDATION.md`, `Cargo.toml`, `rust-toolchain.toml`, and `Justfile` before making changes.
2. Inspect the relevant implementation and tests before proposing a fix.
3. Reproduce every reported failure locally.
4. Never claim that something compiles, passes, is allocation-free, or matches a teacher model unless the corresponding command has completed successfully in this session.
5. Keep the deployed `uor-semantic` runtime:
   - `#![no_std]`
   - free of heap allocation during artifact parsing, routing, prediction, and generation
   - free of third-party Rust dependencies
   - free of floating-point operations and dense matrix multiplication in the runtime path
   - caller-buffered and fixed-capacity
6. The offline compiler and CLI may allocate memory where necessary. Do not weaken the runtime contract to simplify tooling.
7. Minimize dependencies. Do not add a crate when the standard library or a small local implementation is sufficient.
8. Maintain typed errors, deterministic behavior, canonical ordering, content identity, and explicit capacity failures.
9. Every externally meaningful behavior change must have:
   - a registered conformance ID where appropriate,
   - a non-pending Gherkin scenario,
   - a Rust test whose name ends in the same conformance ID,
   - generated conformance documentation synchronized with the model.
10. Use the repository-pinned Rust toolchain. Do not silently lower `rust-version` to make an older compiler work.

## First actions

Run and record:

```bash
pwd
find .. -name AGENTS.md -print
rustc --version --verbose
cargo --version
rustup show active-toolchain || true
git status --short
git branch --show-current
just vv
```

If `just vv` is not green at the start, diagnose and fix the actual failures before beginning feature work. Do not suppress warnings or tests merely to make the gate pass.

## Immediate reproduced failure

The following command downloaded approximately 1.59 GB of Hugging Face model data and only then failed because the corpus file did not exist:

```bash
./target/release/uor-semantic model build \
  HuggingFaceTB/SmolLM2-135M-Instruct \
  --revision 7e27bd9f95328f0f3b08261d1252705110c806f8 \
  --corpus data/construction.txt \
  --work-dir .uor-models/smollm2-baseline \
  --top-k 64 \
  --max-context 32 \
  --regions 256 \
  --iterations 16 \
  --min-exact-bps 10000
```

Observed terminal failure:

```text
FileNotFoundError: [Errno 2] No such file or directory: 'data/construction.txt'
error: python3 exited unsuccessfully with code Some(1)
```

There was also a Transformers warning:

```text
`torch_dtype` is deprecated! Use `dtype` instead!
```

## Required fix: model-build preflight

Implement a robust preflight phase that runs before any network download or expensive model loading.

At minimum, validate before invoking Hugging Face:

- corpus path exists;
- corpus path is a regular readable file;
- corpus is not empty after applying the command's documented blank-line policy;
- revision is an immutable commit identifier under the repository's current policy;
- numeric arguments satisfy all format/runtime bounds;
- work directory can be created or written safely;
- required external executables are present;
- Python bridge requirements are diagnosed clearly when missing.

The CLI must pass an absolute or canonicalized corpus path to the Python bridge so behavior does not depend on the child's working directory.

Failure must be typed and actionable. For a missing corpus, print a concise error before downloading anything, including the resolved path and a copy-paste way to create or specify a corpus.

Do not merely add `data/construction.txt` to hide the path bug.

## Corpus onboarding

Add the smallest maintainable onboarding mechanism after inspecting the current CLI design. Prefer one of these, in order:

1. A `corpus init` or similarly clear CLI command that writes a documented starter corpus only when the target does not exist.
2. A committed `data/construction.example.txt` plus an actionable error telling the user how to copy it.
3. Both, only if the additional command remains simple and dependency-free.

Never overwrite an existing corpus without an explicit flag.

A starter corpus is for pipeline verification, not an accuracy claim. Document that real evaluation requires representative construction, calibration, held-out, and rollout prompt datasets.

## Hugging Face bridge corrections

Inspect `scripts/capture_hf.py` and the CLI process-launch code. Make the smallest compatible corrections:

- replace deprecated `torch_dtype=` usage with the supported `dtype=` form where the installed Transformers API permits it;
- keep `trust_remote_code=False`;
- prefer or require safetensors under the current security policy;
- retain immutable revision pinning;
- preserve tokenizer/model revision alignment;
- surface Python exceptions with the command and relevant path context;
- avoid downloading the model again when a verified pinned source snapshot already exists;
- never delete an existing model source or artifact unless explicitly requested.

Do not perform a broad dependency upgrade solely to remove the Hugging Face update notice. Version changes must be deliberate and tested.

## Tests required for this fix

Add tests that prove:

1. A missing corpus fails before the downloader is invoked.
2. A relative corpus path is resolved correctly when the Python child has a different working directory.
3. An empty corpus fails before download.
4. An invalid numeric bound fails before download.
5. An existing valid corpus reaches the downloader/capture stage.
6. Existing work is not overwritten accidentally.
7. Error text identifies the failing path and corrective action.
8. `model build` remains deterministic for the same pinned inputs.

Use test doubles or a controlled fake executable/process boundary. Unit and CI tests must not download a 1.59 GB model.

Register strong BDD coverage for the user-visible preflight behavior. Suggested capability IDs, subject to the repository's existing registry:

```text
HF-04  Model build validates local inputs before network access
HF-05  Corpus paths are stable across child working directories
CP-02  Existing compilation work is not overwritten implicitly
```

Do not create duplicate or overlapping IDs if equivalent capabilities already exist.

## Then run a real smoke test

After the implementation is green, create a small local corpus through the supported onboarding path and run a real pinned-model smoke build. Use a deliberately small corpus first. Record the exact command and outputs.

Before downloading, verify that the preflight succeeds and paths shown by the CLI are the paths actually consumed by Python.

The real smoke test may use the already-downloaded source directory when it matches the pinned repository and revision. Do not force a second download.

Inspect the resulting artifact and run parity reporting. Keep these claims separate:

- exact captured-context parity;
- graph-only held-out accuracy with exact lookup disabled;
- full captured-rollout sequence parity;
- arbitrary-prompt generalization.

Never describe captured-context parity as universal generative parity.

## Next architecture work after the bug fix

Once the immediate model-build workflow is reliable, assess and implement the next smallest vertical slice toward the v0.2 milestone:

1. separate construction and held-out observation inputs;
2. autoregressive teacher rollout capture with EOS;
3. tokenizer and chat-template identity binding;
4. artifact identity verification and parser resource limits;
5. graph-only evaluation with exact lookup forcibly disabled;
6. genuine overlapping semantic multi-membership rather than hard one-cluster assignment;
7. learned hierarchical divisible route paths rather than slots derived directly from prototype bits;
8. indexed bounded candidate routing rather than scanning every region.

Do not attempt all eight in one uncontrolled rewrite. Choose the smallest coherent vertical slice, add BDD and conformance tests, and keep `just vv` green after each step.

## Accuracy policy

Highest accuracy is a measured optimization target, not an assertion. Report at least:

```text
exact top-1 agreement
exact top-K token-order agreement
exact coverage
graph-only top-1 accuracy
graph-only top-K recall
graph-only coverage
sequence exact-match rate
EOS-position agreement
artifact size
bytes read per generated token
candidate regions scanned per step
runtime allocations and deallocations
```

Construction, calibration, held-out semantic evaluation, and rollout parity prompts must be disjoint or explicitly labeled when they are not.

## Runtime allocation policy

Run allocation tests in isolation and serially so unrelated test-harness allocations do not contaminate the measurements.

The acceptance evidence must cover:

```text
artifact parse
exact prediction
graph-only prediction
semantic routing
greedy generation
EOS termination
```

All measured runtime allocations and deallocations must be zero after warm-up.

## Completion gate

Before reporting completion, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo check -p uor-semantic --no-default-features --locked
cargo check -p uor-semantic --target thumbv7em-none-eabihf --no-default-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo test --workspace --locked -- --test-threads=1
cargo test --workspace --profile release-assertions --locked -- --test-threads=1
cargo test -p repo-conformance --test no_alloc --locked -- --test-threads=1
just vv
cargo build --release --locked -p uor-semantic-cli --bin uor-semantic
./target/release/uor-semantic self-test
```

Then run the small real-model smoke test if the required local Python/Hugging Face environment is available.

## Final response requirements

Return:

1. the root cause of each reproduced failure;
2. files changed and why;
3. behavior added;
4. exact commands run;
5. exact pass/fail results;
6. real-model smoke results, clearly separated from fixture results;
7. runtime allocation evidence;
8. dependency changes, preferably none;
9. remaining limitations and the next recommended vertical slice.

Do not say “fully verified,” “full parity,” “highest accuracy,” or “production-ready” unless the repository contains a test and captured output that directly supports that exact statement.

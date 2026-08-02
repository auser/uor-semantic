# uor-semantic

`uor-semantic` is a Rust workspace for compiling deterministic language-model
observations into bounded semantic artifacts and executing them through an
allocation-free, CPU-native runtime.

The workspace separates three concerns:

```text
Hugging Face source + corpus
            |
            v
uor-semantic-cli / uor-semantic-compiler
            |
            v
      .uors artifact
            |
            v
uor-semantic no_std runtime
```

The runtime supports divisible semantic paths, overlapping masked-Hamming
regions, exact captured contexts, graph-only prediction, and fixed-capacity
greedy generation. It does not contain attention, dense tensor operations, a
GPU backend, floating-point runtime arithmetic, or dynamic allocation.

## Included crates

| Crate                   | Purpose                                                    | Third-party Rust dependencies |
| ----------------------- | ---------------------------------------------------------- | ----------------------------: |
| `uor-semantic`          | Published `no_std` runtime and artifact reader             |                             0 |
| `uor-semantic-compiler` | Deterministic offline compiler and parity evaluator        |                             0 |
| `uor-semantic-cli`      | Download, capture, compile, inspect, predict, and generate |                             0 |
| `repo-model`            | Private claim-register tooling                             |               `serde`, `toml` |
| `repo-conformance`      | Private BDD, accuracy, allocation, and performance suite   |                        `toml` |
| `xtask`                 | Private repository gates                                   |                    0 external |

The production model lifecycle therefore has no third-party Rust dependency.
The optional Hugging Face capture bridge is Python-based and requires PyTorch,
Transformers, and `huggingface_hub`.

## Runtime contract

The shipped `uor-semantic` crate is:

- `#![no_std]` and `#![forbid(unsafe_code)]`;
- dependency-free and default-feature-free;
- heapless, using borrowed artifact bytes, arrays, slices, const-generic
  capacities, and caller-owned output buffers;
- integer and fixed-point only;
- bounded by declared context, candidate, frontier, and output capacities;
- deterministic under the scalar reference semantics;
- audited for the forbidden runtime arithmetic and allocation surface.

The conformance suite observes zero heap allocations and deallocations for:

- warmed semantic routing;
- artifact parsing;
- exact and graph prediction;
- fixed-capacity greedy generation.

The command-line application may allocate while reading files, printing output,
or orchestrating offline compilation. The zero-allocation contract applies to
the runtime APIs being measured, not to operating-system integration code.

## Toolchain and acceptance gate

This build pins Rust 1.97.1 and Rust edition 2024.

```bash
rustup toolchain install 1.97.1 \
  --component rustfmt,clippy \
  --target thumbv7em-none-eabihf

just vv
```

`just vv` runs:

- formatting verification;
- generated conformance and honesty checks for all registered capabilities;
- strict runtime source audit;
- Clippy with warnings denied;
- host and embedded-target `no_std` checks;
- all-feature and all-target workspace checks;
- all workspace Rust tests in serialized execution;
- documentation tests;
- an optimized release profile with debug assertions and overflow checks;
- isolated allocation-census tests;
- BDD cross-linking between model rows, scenarios, and Rust tests;
- the end-to-end CLI self-test.

The verified CI run and packaged release are described in
[`BUILD-VERIFIED.md`](BUILD-VERIFIED.md).

## CLI quick start

Build the CLI from source:

```bash
cargo build --release --locked \
  -p uor-semantic-cli \
  --bin uor-semantic

./target/release/uor-semantic self-test
```

Expected result:

```text
self-test: PASS
exact_top1_basis_points: 10000
generated_tokens: 3,4
```

The CLI exposes:

```text
self-test
artifact inspect
artifact predict
artifact generate
model download
model capture
model compile
model parity
model build
model generate
```

Run `uor-semantic help` for the complete argument surface.

For callers migrating from the R4 lifecycle API, the CLI crate also exposes
typed `download`/`download_source`, observation `compile`, and source
`compile_source` functions. The equivalent
top-level command aliases are:

```bash
./target/release/uor-semantic download \
  --repository HuggingFaceTB/SmolLM2-135M-Instruct \
  --revision 7e27bd9f95328f0f3b08261d1252705110c806f8 \
  --name smollm2-135m

./target/release/uor-semantic compile \
  .uor-models/smollm2-135m/observations.uorobs \
  --output .uor-models/smollm2-135m/model.uors \
  --r4g1-output .uor-models/smollm2-135m/model.r4g1
```

`model build` remains the end-to-end source-download, teacher-capture,
compile, and parity workflow; the compatibility `compile` entry point starts
from captured observations and always writes a runtime-validated `.uors`
artifact. `compile_source` performs the same source-side orchestration from a
verified local Hugging Face snapshot and a construction corpus. The compiler
also exposes `export_r4g1` for a deterministic structural R4G1 container with
canonical aligned sections and verified BLAKE3 HEAD and artifact CIDs. This is
the interchange boundary, and `compile`/`model compile` can write it with
`--r4g1-output`. It emits structural refinement and bounded predictive kind-2
edges, a target-framed EMIT root prior, and an RX1-framed EXCT table from
synthetic route-code projections.
It is not scored R4G1 equivalence: target graded codes, predictive scoring
certificates, and target replay parity remain future work.
The published `no_std` core now also exposes the bounded R4G1 residual-scoring
semantics: signed saturation, evidence-ID de-duplication, capacity errors, and
deterministic score/ID ordering.

## Hugging Face model pipeline

Create a Python environment for teacher capture:

```bash
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r scripts/requirements-hf.txt
```

Start with the maintained smoke corpus, or replace it with representative
construction, calibration, held-out, and rollout prompt datasets:

    cp data/construction.example.txt data/construction.txt

The example is only for pipeline verification and makes no accuracy claim.
Prepare a UTF-8 corpus with one non-empty sample per line, then run the full
pipeline using an immutable 40-character Hugging Face commit revision:

```bash
./target/release/uor-semantic model build \
  HuggingFaceTB/SmolLM2-135M-Instruct \
  --revision 7e27bd9f95328f0f3b08261d1252705110c806f8 \
  --corpus corpus.txt \
  --work-dir .uor-models/smollm2-135m \
  --top-k 64 \
  --max-context 32
```

To measure transfer separately, add --held-out-corpus
data/held-out.example.txt. The construction corpus is compiled into the
artifact; the held-out report is evaluated through the graph lane with
exact-context lookup forcibly disabled. Keep construction and held-out prompt
families disjoint when making accuracy comparisons.

The build directory contains:

```text
source/              pinned Hugging Face snapshot
observations.uorobs  deterministic teacher observations
model.uors           compiled runtime artifact
parity.json          exact-lane and graph-lane measurements
rollout-parity.json  sequence and EOS-position measurements when rollouts are captured
```

Capture metadata binds the artifact to the exact tokenizer files, chat template,
special-token map, and EOS token ID used by the teacher. Parity rejects a
capture whose identity does not match the compiled artifact. Use
`--rollout-tokens <n>` to emit bounded autoregressive rollouts and
`model parity --rollouts` to measure them.

Inspect or test the artifact directly:

```bash
./target/release/uor-semantic artifact inspect \
  .uor-models/smollm2-135m/model.uors

./target/release/uor-semantic model parity \
  .uor-models/smollm2-135m/model.uors \
  --observations .uor-models/smollm2-135m/observations.uorobs \
  --min-exact-bps 10000 \
  --min-graph-bps 0 \
  --min-graph-coverage-bps 0 \
  --min-graph-top-k-bps 0
```

### Interactive artifact testing

After compilation, the artifact can be exercised without Python or network
access. Prediction and low-level generation accept token IDs; use
`model generate` below when a text prompt and tokenizer are desired.

```bash
./target/release/uor-semantic artifact inspect \
  .uor-models/smollm2-135m/model.uors

./target/release/uor-semantic artifact predict \
  .uor-models/smollm2-135m/model.uors \
  --tokens "1,2,3" \
  --graph-only

./target/release/uor-semantic artifact generate \
  .uor-models/smollm2-135m/model.uors \
  --tokens "1,2" \
  --max-tokens 16
```

The interactive command path is covered by the `CL-02` BDD scenario and Rust
test. Run the traceability and behavior checks with:

```bash
just bdd
cargo test -p repo-conformance --test model_pipeline --locked -- --test-threads=1
```

For text prompts, use the tokenizer-backed command:

```bash
./target/release/uor-semantic model generate \
  .uor-models/smollm2-135m/model.uors \
  --source .uor-models/smollm2-135m/source \
  --prompt "Explain semantic routing." \
  --max-tokens 16 \
  --python .venv/bin/python
```

For held-out certification, set non-zero coverage and top-K floors. These
thresholds are evaluated over the same held-out samples and prevent a narrow
high-recall subset from passing without sufficient graph coverage.

The runtime parser verifies the stored codebook identity against the artifact
bytes and rejects artifacts over its bounded section and total-size limits
before exposing records.

Compiler calibration retains every observation whose prototype distance is
within the nearest-prototype distance plus `overlap_margin`; `model compile`
reports the resulting `region_memberships` count. This is bounded and
deterministic, and is distinct from exact-context lookup.

With no explicit calibration flags, `model compile` uses the current
cross-corpus accuracy profile: 48 regions, 16 update passes, overlap margin
16, and one emission per region. Override `--regions`, `--iterations`,
`--overlap-margin`, or `--region-top-k` when reproducing a sweep.

Compiled region paths are derived from nearest eligible prototype parents and
deterministic sibling slots, rather than copying prototype bits into path
slots. Path depth remains bounded by the artifact format.

Graph prediction probes a serialized 8-bit coarse-Hamming candidate index.
Regions are inserted into every bucket that could contain them under their
calibrated radius; broad regions can therefore remain present in many buckets,
while narrow regions avoid a full region scan. The CLI reports
`regions_scanned` for each graph prediction.

Generate through the same local tokenizer:

```bash
./target/release/uor-semantic model generate \
  .uor-models/smollm2-135m/model.uors \
  --source .uor-models/smollm2-135m/source \
  --prompt "Explain semantic routing." \
  --max-tokens 64
```

## What “parity” means in this build

The exact-context lane can reproduce the captured teacher argmax for every
observation present in the compilation corpus. The acceptance fixture measures
10,000 basis points, or 100%, exact top-1 agreement.

That result is not a certificate of universal generative equivalence. Current
Hugging Face capture is teacher-forced over a supplied corpus, uses a bounded
suffix of at most 32 tokens, and quantizes each captured top-K distribution.
For prompts or contexts absent from exact evidence, generation depends on the
overlapping semantic graph. Graph-only accuracy is measured separately with
exact lookup disabled so memorization cannot be presented as semantic
generalization.

Parity JSON also records `graph_regions_scanned`, the total indexed candidate
regions inspected across graph-only samples, so accuracy and bounded routing
work remain visible in the same evaluation artifact.

It also reports exact top-K order agreement, overall graph top-K recall,
graph coverage, and graph top-K recall conditional on coverage. Compare
calibration candidates under one fixed construction/held-out manifest with:

```bash
scripts/sweep_graph_accuracy.sh \
  construction.uorobs \
  held-out.uorobs \
  sweep-results
```

For symmetric cross-validation, run both construction directions:

```bash
scripts/cross_validate_graph_accuracy.sh \
  corpus-a.uorobs \
  corpus-b.uorobs \
  cross-validation-results
```

This emits separate sweep reports for `a -> b` and `b -> a`, with exact
lookup disabled in both directions.

To reproduce the versioned representative corpus inputs used by the current
validation record, prepare them with the dependency-free helper:

```bash
scripts/prepare_graph_accuracy_corpus.sh validation/graph-accuracy/wikitext-2
```

The sweep emits one artifact and JSON report per candidate. It is an empirical
comparison tool, not a universal accuracy guarantee; retain the sample count,
corpus identities, and candidate configuration with every result.
The candidate set includes both conservative baselines and region-dense,
low-emission-capacity settings that are useful when testing coverage-aware
top-K tradeoffs.

The route toward higher accuracy is therefore empirical:

1. capture a broader and more representative corpus;
2. retain larger top-K teacher distributions;
3. calibrate region count, overlap margin, and region emission capacity;
4. hold out complete context families for graph-only evaluation;
5. accept a new artifact only when its parity and latency report beats the
   pinned baseline under the same evaluation manifest.

## Semantic model

A semantic region accepts context code `x` when:

```text
distance = popcount((x XOR prototype) AND mask)
accepted = distance <= radius
margin   = radius - distance
```

Several regions may remain active at the same time. The runtime retains them in
one canonical order: larger margin, deeper refinement, smaller region ID, then
smaller path ID. Divisible paths preserve broad-to-specific prefixes, while
separate refinement, overlap, and transition edges avoid collapsing hierarchy,
ambiguity, and generation dynamics into one relationship.

## Licensing

Dual-licensed under MIT or Apache-2.0, at your option.
# uor-semantic

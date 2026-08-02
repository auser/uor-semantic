# Architecture

## Component boundary

```text
                         offline / rich boundary

Hugging Face snapshot + corpus
              |
              v
      capture_hf.py
              |
              v
   observations.uorobs
              |
              v
uor-semantic-compiler ---- parity evaluator
              |
              v
         model.uors

                         deployed / strict boundary

borrowed artifact bytes --> ArtifactView --> predict / generate_into
                                      |
                                      v
                            caller-owned fixed buffers
```

`uor-semantic-cli` orchestrates both boundaries. The compiler and CLI may use
heap storage because model download, corpus processing, clustering, and file I/O
are offline or application activities. The published `uor-semantic` crate is the
strict runtime: `no_std`, safe, dependency-free, fixed-capacity, integer-only,
and allocation-free across the audited APIs.

## Identity and semantic locality

A 256-bit `CodebookId` identifies which compiled codebook or artifact is in use.
A `SemanticPath` describes a broad-to-specific semantic direction under that
codebook. Identity and locality remain distinct:

```text
CodebookId + [slot, slot, ...] + membership margin
```

A `SemanticAddressBundle` can retain several paths because one contextual
occurrence can legitimately activate more than one meaning.

## Overlapping routing

For context code `x` and region `n`, the scalar reference router computes:

```text
distance = popcount((x XOR prototype_n) AND mask_n)
accepted = distance <= radius_n
margin   = radius_n - distance
```

Every accepted region enters a caller-owned `RouteCloud` under one total order:

1. larger margin;
2. deeper refinement;
3. smaller region ID;
4. smaller path ID.

Capacity never grows. When more regions match than fit, the canonical prefix is
retained and the route summary reports the omitted count.

## Graph relationships

Three relationships remain distinct:

- `RefinementEdge`: broad region to more precise region;
- `OverlapEdge`: co-active or adjacent regions;
- `TransitionEdge`: likely semantic successor.

This prevents hierarchy, ambiguity, and generation dynamics from becoming one
overloaded edge kind.

## Artifact prediction lanes

The `.uors` artifact has two prediction lanes:

```text
exact context available
        |
        v
captured top-K emission evidence

exact context unavailable or graph-only policy selected
        |
        v
context signature --> overlapping regions --> accumulated emissions
```

The parity evaluator reports exact and graph measurements separately. This is a
load-bearing distinction: exact evidence measures reproduction of captured
teacher behavior, while graph-only held-out evaluation measures transfer beyond
exact keys.

## Generation

`generate_greedy_into` accepts:

- a borrowed `ArtifactView`;
- a fixed-capacity `GenerationState`;
- caller-owned token output;
- caller-owned routing scratch;
- caller-owned prediction storage.

It repeatedly predicts, writes the highest-ranked token, and updates the bounded
context state until output capacity is exhausted or no candidate is available.
It allocates no heap memory in the audited runtime path.

The CLI wraps this API with file loading, a local Hugging Face tokenizer bridge,
and text rendering. Those application operations are outside the strict runtime
allocation boundary.

## Compiler

The offline compiler:

1. parses canonical `UOROBS1` records and their tokenizer/chat-template identity;
2. canonicalizes and checks duplicate contexts;
3. writes optional exact-context evidence;
4. induces deterministic masked-Hamming regions;
5. calibrates overlap and region emissions;
6. serializes one packed artifact accepted by the zero-copy runtime;
7. evaluates exact and graph-only top-1 agreement, rejecting tokenizer identity
   mismatches before measuring parity.

Prototype calibration uses nearest distance only to establish a deterministic
baseline. Every prototype within the configured overlap margin retains the
observation, so one observation can contribute to multiple region emission
profiles. The compiler reports this membership count and bounds it before
serialization.

Region paths are learned from prototype proximity: each later prototype picks
the nearest earlier eligible prototype as parent, then receives a deterministic
sibling slot. The resulting coarse-to-fine path is capped at the artifact's
fixed route depth.

Each artifact also stores a bounded 8-bit coarse-Hamming candidate index. A
region is placed in every bucket whose coarse distance from its prototype key
does not exceed the region radius, which is a sound superset of full-signature
matches. Graph prediction scans only the selected bucket and reports the
candidate count; broad calibrated regions may still occupy many buckets.

The current Hugging Face bridge captures teacher-forced top-K distributions over
a user-supplied corpus. It uses deterministic CPU execution settings and an
immutable model revision. The semantic artifact is therefore a compiled
behavioral representation, not a claim that transformer weights have a unique
lossless mapping into semantic regions.

## Bounded context and parity

Current artifacts retain at most 32 token IDs in one context. Exact parity is
therefore defined against captured observations under that bounded suffix
policy. Contexts absent from exact evidence use semantic regions, and their
accuracy must be measured on held-out data with exact lookup disabled.

Improving graph accuracy is an offline compilation and evaluation problem; it
does not relax the runtime contract.

Artifact parsing also verifies the content identity stored in the header and
rejects excessive exact, region, emission, or total-byte declarations before
record traversal. These checks are fixed and allocation-free.

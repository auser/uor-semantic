# VERIFICATION

The acceptance suite separates structural behavior, measured behavior, and
claims reproduced from cited design inputs.

| Command | What it checks | Principal IDs |
| --- | --- | --- |
| `just fmt-check` | stable formatting | — |
| `just model` | generated claim model, strict source boundary, no unfinished marker | `CM-01`, `RT-01` |
| `just lint` | workspace Clippy with warnings denied | `RT-01` |
| `just check` | dependency-free host and `thumbv7em-none-eabihf` core plus all workspace targets | `RT-01` |
| `just test` | semantic, path, accuracy, work, and tooling tests | all build IDs |
| `just doc` | public examples compile as documentation tests | `SR-01`, `RH-01` |
| `just release-assertions` | optimized behavior with overflow and debug assertions enabled | `SR-02`, `PF-01` |
| `just no-alloc` | warmed allocation/deallocation delta is zero | `RT-02` |
| `just bdd` | every ID has a scenario and named test; honesty levels agree | `CM-02`, `CM-03` |
| `just bench` | emits a hardware-specific empirical latency report | `PF-02` |
| `just deny` | advisories, licenses, sources, versions | R6 |

## Accuracy

`AC-01` is exact accuracy on a committed three-case fixture:

1. context-free `stop` retains three meanings and ranks physical cessation
   first under the fixture margins;
2. `stop the car` retains physical-motion inhibition only;
3. `wait at the bus stop` retains transportation location only.

This validates the conformance mechanism and contextual overlap semantics. It
is not evidence of corpus-scale language understanding. A learned compiler must
pin its own corpus, split, codebook, expected labels, and report without changing
what `AC-01` means.

## Speed

`PF-01` is the portable gate. For the three-region, one-word fixture it checks
candidate scans, packed-word steps, bitwise operations, population counts,
comparisons, reads, and writes against a declared upper bound.

`PF-02` emits minimum, median, and maximum nanoseconds per route along with the
target architecture and sample dimensions. The values are observations from the
machine that produced the report and are never treated as universal constants.

## Falsifiability

| Gate | Planted defect exercised by the suite |
| --- | --- |
| model/BDD cross-check | remove all test names and confirm every registered ID is reported missing |
| strict source audit | inject a product operator into a synthetic source line and confirm a violation |
| source-audit control | place the same symbol in documentation and a string and confirm it is ignored |
| deferral audit | expose a marker outside quoted text and confirm it remains detectable |
| operation budget | use an exact bound tight enough that an added candidate or packed word fails |
| allocation census | count allocator entry and exit calls around the warmed route loop |

## What is not established here

The suite does not establish corpus-scale semantic quality, coherent language
generation, target-specific absence of compiler-introduced arithmetic
instructions, or superiority over transformer inference. Those require separate
content-addressed evaluation and machine-code certificates built on top of this
runtime substrate.

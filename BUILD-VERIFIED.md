# Verified build

This source tree contains the same Rust code that passed the complete
`uor-semantic` acceptance gate in GitHub Actions.

```text
validation commit: d62f2446e4b0d4692288e216d42eba28d213ffca
workflow run:       30683276689
job:                91324257301
artifact:           8813029477
rustc:              1.97.1 (8bab26f4f 2026-07-14)
cargo:              1.97.1 (c980f4866 2026-06-30)
host:               x86_64-unknown-linux-gnu
result:             success
```

The release package includes the exact Linux x86-64 CLI binary produced by that
run. Documentation and `MANIFEST.sha256` were refreshed after the run; no Rust,
Python, TOML, Gherkin, workflow, or build-configuration source was changed after
the successful gate.

The original GitHub Actions artifact digest is:

```text
sha256:3002d26044372f3392e074c9a777ed931da4875eae95e72762a3867e0f6446df
```

The package-level `SHA256SUMS` file identifies every distributed archive and
binary.

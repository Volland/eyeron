# Proof-check reports

Each `.txt` file records the CLI proof-check report for the matching N3 proof
in [`../proof/`](../proof/), checked against its source example and any companion
input. The query answer or other printed output is in [`../output/`](../output/).

Reports list C1–C9 results and any trust obligations or failures. `checked with
obligations` includes assumptions whose truth the checker does not establish.
`variable-predicate-dependency.txt` records an intentionally invalid proof: C6
rejects its contradictory absence assumption.

To reproduce one report from the repository root:

```sh
cargo run --release -- --check-proof examples/proof/socrates.n3 examples/socrates.n3
```

After deliberately changing examples or the checker, regenerate the artifacts:

```sh
EYERON_UPDATE_GOLDENS=1 cargo test --release --test examples
EYERON_UPDATE_GOLDENS=1 cargo test --release --test proof_checking
```

The proof-checking suite verifies every proof and compares these reports on
ordinary runs, without rewriting them.

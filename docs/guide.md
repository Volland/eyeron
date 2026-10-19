# Eyeron guide

Build, run, embed, and test Eyeron here. The [language reference](n3.md) describes evaluation and built-ins; the [proof-checking contract](proof-checking.md) describes what verification establishes.

## Build and run

```bash
cargo build --release
./target/release/eyeron examples/socrates.n3
./target/release/eyeron -c examples/dog-license.n3
```

The commands below use `eyeron`; use `./target/release/eyeron` if the executable is not on your `PATH`.

### Inputs and output

| Input | Selection |
| --- | --- |
| Notation3 | `.n3`, or the default parser for stdin and unrecognized extensions |
| Turtle / TriG | `.ttl` / `.trig` |
| N-Triples / N-Quads | `.nt` / `.nq` |
| RDF Message Log | Recognized by its `VERSION "*-messages"` or `@version` directive |

Local files and HTTP(S) URLs are accepted. Files are parsed separately and merged, keeping their blank-node and quantifier scopes separate. Relative IRIs use each file's location or URL unless `--base-iri` overrides it. Use `-` for stdin, or supply options without files to read stdin; no arguments prints help. Ordinary stdin uses N3; `--rdf` selects Turtle for stdin and rejects unknown file extensions.

```bash
eyeron rules.n3 data.ttl
eyeron --rdf - < data.ttl
eyeron --base-iri https://example.org/ - < rules.n3
```

Ordinary output contains derived results. When `log:query` is present, its answers select the output. If results contain `log:outputString`, the printer emits those strings instead of triples. [Evaluation](n3.md#evaluation-and-output) explains the distinction between output and closure.

### CLI options

| Option | Behavior |
| --- | --- |
| `-a`, `--ast` | Print the parsed document and exit |
| `-p`, `--proof` | Write an N3 proof document |
| `-c`, `--prove-and-check` | Generate a proof in memory and print its checking report |
| `--check-proof FILE` | Check a saved proof against the positional source inputs; `-` reads the proof from stdin |
| `-r`, `--rdf` | Select RDF-compatible output and Turtle parsing for stdin; file extensions still select their own parser |
| `--base-iri IRI`, `--base IRI` | Override the parser's base IRI |
| `--max-backward-depth N` | Set backward search depth; default 32 |
| `--stream-messages` | Process RDF Message Log payloads independently; requires `--rdf` |
| `-s`, `--stream` | Compatibility flag; ordinary output still waits for reasoning to finish |
| `-h`, `--help`; `-v`, `--version` | Print help or the version |

`--proof-comments` aliases `--proof`. `--builtin`, `--store`, `--store-path`, `--store-clear`, and `--enforce-https` are accepted with warnings and have no implementation. `--prove-and-check` rejects `--ast`, `--check-proof`, and `--stream-messages` in the same invocation. `--check-proof` also rejects `--stream-messages`.

Successful reasoning and valid proofs return status 0, including proofs with listed trust obligations. Parse, reasoning-limit, and invalid-proof errors return 1. A fired inference fuse prints its rule and matched instance and returns **65**. Library callers should inspect both completion status and `result.fuse`.

## Proofs and their limits

```bash
eyeron -c examples/dog-license.n3
eyeron -p examples/socrates.n3 > /tmp/socrates.why.n3
eyeron --check-proof /tmp/socrates.why.n3 examples/socrates.n3
```

A proof records derived claims and steps with their justifications, bindings, and premises. The checker checks those recorded steps, premise resolution, dependency cycles, and claim coverage. It assumes the source facts; a valid proof does not establish their truth or prove that every possible conclusion was found.

`checked` means every step justification was verified. `checked with obligations` lists accepted assumptions whose truth was not verified, such as scoped collection or clock-dependent built-ins. An invalid proof lists the affected conclusion and the failed condition. See [the contract](proof-checking.md) for exact rules and test coverage.

`-c` and `--check-proof` use the same source loader: each input is parsed with its format and base IRI, then merged with separate file scopes. A saved proof can come from stdin when the sources are files; sources can come from stdin when the proof is a file. Supplying both through stdin is rejected. The Rust `check_proof_document` API accepts an already prepared `Document`. Proof collection retains additional derivation data and costs memory.

### Example: audit a decision

[verifiable-decision.n3](../examples/verifiable-decision.n3) combines consent, privacy, and ethics assertions to permit a request. Generate and check its proof:

```bash
eyeron -p examples/verifiable-decision.n3 > /tmp/decision.why.n3
eyeron --check-proof /tmp/decision.why.n3 examples/verifiable-decision.n3
```

The report contains `checked: 11 steps, 11 verified, 0 trusted`. Proofs are N3 data, so another program can follow `pe:uses` and apply a policy to the assertions supporting a decision:

```bash
eyeron examples/verifiable-decision-audit.n3 \
       examples/input/verifiable-decision-audit.n3
```

This bundled audit reads a saved copy of the decision proof and identifies its supporting authorities. Removing an authority from its quoted trust policy triggers an inference fuse. To audit the proof you just generated, replace the second input with `/tmp/decision.why.n3`.

## Rust library

```rust
fn main() -> eyeron::Result<()> {
    let output = eyeron::reason(r#"
        @prefix : <http://example.org/>.
        :Socrates a :Human.
        { ?x a :Human } => { ?x a :Mortal }.
    "#)?;
    assert!(output.contains(":Socrates a :Mortal"));
    Ok(())
}
```

The convenience `reason(&str)` parses N3 or an RDF Message Log and returns serialized derived output; it returns an error for incomplete reasoning. For control over inputs and results, use `parse_n3`, `parse_n3_with_source`, `parse_rdf12`, or `parse_rdf_message_log`, then `reason_document(&document, &options)`.

`ReasonerResult` contains `status`, `limits_reached`, `errors`, `statistics`, `derived`, `closure`, `proofs`, and `fuse`. Inspect these before treating the output as complete. `include_explicit` controls copies of input facts and their source labels; it defaults to true and is forced on for proof collection.

`PreparedReasoner::new(document)` takes an already parsed program and prepares its rules and agenda. Each `prepared.reason(&data, &options)` call evaluates an independent data batch. `proof_to_n3` writes proof text; `proof_check_n3::check_proof_document(&document, &proof)` checks it. The [source exports](../src/lib.rs) and `cargo doc --no-deps` provide the API details.

## Browser

The [playground](https://eyereasoner.github.io/eyeron/playground) uses the same Rust engine through WebAssembly. Its JS exports include `reason`, `reasonWithOptions`, `reasonWithData`, and `reasonWithDataReport`. `EyeronSession` prepares a program once for independent data batches; `reasonReport` adds statistics to its JSON result. See [wasm.rs](../src/wasm.rs) for signatures and error envelopes.

To rebuild and serve it locally, install `wasm-pack` and the Rust `wasm32-unknown-unknown` target, then run:

```bash
cargo playground
python3 -m http.server
```

Open <http://localhost:8000/playground.html>. The build updates the committed `pkg/` artifacts. Browser APIs expose reasoning and proof generation; proof checking is currently exposed through the native CLI and Rust library.

## Development and testing

```bash
cargo test --release
./scripts/test-all
cargo test --release --test examples
cargo test --release --test proof_checking
cargo doc --no-deps
```

`scripts/test-all` runs the release test suite and appends totals across test binaries, with separate counts for conformance cases inside suite checks. The example harness compares N3 output as a graph and Markdown output by stable report lines. Proof snapshots are compared as text, with clock values normalized for `age.n3`. The separate proof-checking harness requires every saved proof to check successfully.

The vendored N3 suite has two expectations adjusted for Eyeron's graph-term scoping; see [scope differences](n3.md#conformance-scope-differences). The [W3C runner notes](../tests/w3c_rdf/README.md) cover the local manifest mirror and EARL report. CI also runs native/Wasm Clippy, a Wasm build, rustdoc, and cargo-deny; its formatting job is advisory.

| Code | Responsibility |
| --- | --- |
| `src/ast.rs` | `Term`, scoped `Graph`, triples, rules, documents |
| `src/lexer.rs`, `src/parser.rs`, `src/rdf_compat.rs` | Input syntax and parser profiles |
| `src/reasoner.rs` | Evaluation, built-ins, prepared programs, budgets |
| `src/printing.rs` | N3/TriG serialization, RDF JSON, fuse reports |
| `src/proof_writer.rs` | N3 proof generation |
| `src/proof_check.rs`, `src/proof_check_n3.rs` | Generic checker and N3 reader |
| `src/main.rs`, `src/wasm.rs` | Native CLI and browser entry points |

For maintainers, `./release patch "message"` (or `minor`/`major`) pulls `main`, bumps the version, rebuilds the playground, commits all working-tree changes, and pushes `main` and an annotated tag. Run the checks beforehand; the script does not run tests. Tag pushes trigger the GitHub release workflow.

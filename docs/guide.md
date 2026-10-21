# <img src="https://josd.github.io/images/eye.png" alt="" width="32" height="32"> Eyeron guide

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
| RDF Message Log | Recognized by a `VERSION` or `@version` directive naming a `*-messages` version, or by a line holding only `MESSAGE` or `@message .` |

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

`--proof-comments` aliases `--proof`. `--builtin`, `--store`, `--store-path`, `--store-clear`, and `--enforce-https` are accepted with warnings and have no implementation. `--prove-and-check` rejects `--ast`, `--proof`, `--check-proof`, and `--stream-messages` in the same invocation. `--check-proof` rejects `--ast`, `--proof`, and `--stream-messages`.

Successful reasoning and valid proofs return status 0, including proofs with listed trust obligations. Parse, reasoning-limit, and invalid-proof errors return 1. A fired inference fuse prints its rule and matched instance and returns **65**. Library callers should inspect both completion status and `result.fuse`.

## Proofs and their limits

```bash
eyeron -c examples/dog-license.n3
eyeron -p examples/socrates.n3 > /tmp/socrates.why.n3
eyeron --check-proof /tmp/socrates.why.n3 examples/socrates.n3
```

A proof claims what the run outputs and records the steps those claims depend on, with their justifications, bindings, and premises. The checker applies seven conditions: it re-performs each recorded rule application (C1) and built-in (C5), and checks dependency cycles (C2), justification form (C3), premise and claim coverage (C4), that the proof's own evidence does not contradict its assumptions (C6), and that every step serves a claim (C7). It assumes the source facts; a valid proof does not establish their truth or prove that every possible conclusion was found.

`checked` means every step justification was verified. `checked with obligations` lists accepted assumptions whose truth was not verified, such as negation or collection over the current graph, or clock-dependent built-ins. An invalid proof lists the affected conclusion and the failed condition. See [the contract](proof-checking.md) for exact rules and test coverage.

`-c` and `--check-proof` use the same source loader: each input is parsed with its format and base IRI, then merged with separate file scopes. A saved proof can come from stdin when the sources are files; sources can come from stdin when the proof is a file. Supplying both through stdin is rejected. The Rust `check_proof_document` API accepts an already prepared `Document`. Proof collection retains additional derivation data and costs memory.

### Example: audit a decision

[verifiable-decision.n3](../examples/verifiable-decision.n3) combines DPV descriptions of consent and safeguards with ethics assertions to derive an ODRL Agreement from an ODRL Request. The [slide deck](../examples/deck/verifiable-decision.md) explains the decision and its audit. Generate and check its proof:

```bash
eyeron -p examples/verifiable-decision.n3 > /tmp/decision.why.n3
eyeron --check-proof /tmp/decision.why.n3 examples/verifiable-decision.n3
```

The report contains `checked: 30 steps, 30 verified, 0 trusted`. Proofs are N3 data, so another program can follow `pe:uses` and apply a policy to the assertions supporting a decision:

```bash
eyeron examples/verifiable-decision-audit.n3 \
       examples/input/verifiable-decision-audit.n3
```

This bundled audit reads a saved copy of the decision proof and identifies its supporting authorities. Removing a record's use-permission link from its quoted ODRL evidence policy triggers an inference fuse. To audit the proof you just generated, replace the second input with `/tmp/decision.why.n3`.

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

The [playground](https://eyereasoner.github.io/eyeron/playground) runs eyeron through WebAssembly. Its visible controls correspond to the CLI options above: choose conclusions, AST, proof output, generated-proof checking, or saved-proof checking, then set RDF compatibility, message streaming, the base IRI, or backward depth. Help and version buttons run `--help` and `--version`. The displayed command shows the selected arguments.

Open one or more files, load URLs, or edit input directly. Each input keeps its own name and scope; extensions select the parser just as on the CLI, and `-` represents stdin. Bundled examples load their companion input when one exists. Relative IRIs use a fetched URL or, for an uploaded file, its name resolved against the playground URL; `--base-iri` overrides that base. Browser URL loading requires the remote server to allow cross-origin requests. `--stream-messages` reasons over each message independently and displays each result as it is emitted; uploaded and fetched input texts are loaded into memory first. Stop cancels a fetch or terminates the reasoning worker.

The `runCliReport(request, onOutput)` JS export accepts JSON containing `args`, `sources` (`name`, `text`, and optional `baseIri`), and an optional `proof` (`name` and `text`). It sends stdout chunks to the callback and returns JSON with `ok`, `exitCode`, and `stderr`. It uses the same option parser, source parsing, execution, and proof-check report as the native CLI. Existing `reason`, `reasonWithOptions`, `reasonWithData`, `reasonWithDataReport`, and `EyeronSession` exports remain available. See [wasm.rs](../src/wasm.rs) for their signatures.

To rebuild and serve it locally, install `wasm-pack` and the Rust `wasm32-unknown-unknown` target, then run:

```bash
cargo playground
python3 -m http.server
```

Open <http://localhost:8000/playground.html>. The build updates the committed `pkg/` artifacts.

## Development and testing

```bash
cargo test --release
./scripts/test-all
cargo test --release --test examples
cargo test --release --test proof_checking
node --experimental-vm-modules --test tests/playground_loading.mjs
cargo doc --no-deps
```

`scripts/test-all` runs the release test suite and ends with one line showing individual tests run, the percentage OK, and total elapsed time. It counts conformance cases in place of their suite checks, without double-counting the W3C manifest and aggregate reports. The example harness compares N3 output as a graph and Markdown output by stable report lines. Proof snapshots are compared as text, with clock values normalized for `age.n3`. The separate proof-checking harness requires every saved proof to check successfully, apart from one intentional negative cycle it expects C6 to reject. After an intentional output change, `EYERON_UPDATE_GOLDENS=1 cargo test --release --test examples` rewrites the goldens that no longer match.

The vendored N3 suite has two expectations adjusted for Eyeron's graph-term scoping; see [scope differences](n3.md#conformance-scope-differences). The [W3C runner notes](../tests/w3c_rdf/README.md) cover the local manifest mirror and EARL report. CI has three jobs: native release tests, Clippy, and rustdoc; Wasm Clippy and a release build; and cargo-deny for dependency checks.

| Code | Responsibility |
| --- | --- |
| `src/ast.rs` | `Term`, scoped `Graph`, triples, rules, documents |
| `src/lexer.rs`, `src/parser.rs`, `src/rdf_compat.rs` | Input syntax and parser profiles |
| `src/reasoner.rs` | Evaluation, built-ins, prepared programs, budgets |
| `src/printing.rs` | N3/TriG serialization, RDF JSON, fuse reports |
| `src/proof_writer.rs` | N3 proof generation |
| `src/proof_check.rs`, `src/proof_check_n3.rs` | Generic checker and N3 reader |
| `src/cli.rs`, `src/main.rs`, `src/wasm.rs` | Shared CLI behavior, native I/O, and browser bindings |

For maintainers, `./release patch "message"` (or `minor`/`major`) pulls `main`, bumps the version, rebuilds the playground, commits all working-tree changes, and pushes `main` and an annotated tag. Run the checks beforehand; the script does not run tests. Tag pushes trigger the GitHub release workflow.

<img src="https://josd.github.io/images/eye.png" alt="" width="96" height="96" align="left">

# Eyeron

Eyeron is a Rust reasoner for **Notation3 (N3)**. It derives conclusions from facts and rules, writes N3 proofs, and checks those proofs. It runs as a command-line program, a Rust library, and a browser application.

<br clear="left">

[Try the browser playground](https://eyereasoner.github.io/eyeron/playground).

```bash
cargo build --release
./target/release/eyeron examples/socrates.n3
./target/release/eyeron -c examples/dog-license.n3
```

The second command prints derived conclusions. The third generates a proof and checks it, reporting verified steps and any trust obligations.

## Documentation

- [Guide](docs/guide.md): CLI, input formats, proof workflow, Rust/browser APIs, and development.
- [Language and runtime reference](docs/n3.md): rules, quantifier scopes, built-ins, evaluation, resource limits, and limitations.
- [Proof-checking contract](docs/proof-checking.md): proof syntax, validity conditions, trust obligations, and checker limitations.

Runnable programs live in [`examples/`](examples/), with expected output and proof documents in its `output/` and `proof/` directories. Start with the [Zebra puzzle](examples/zebra.n3), [explicit quantification](examples/explicit-quantification.n3), or the [decision and proof audit](examples/verifiable-decision-audit.n3).

## License

MIT. See [LICENSE.md](LICENSE.md).

# Local W3C RDF test mirror

The Rust-only W3C RDF integration test stores downloaded W3C RDF test-suite resources here by default:

```text
tests/w3c_rdf/rdf-tests/
```

Normal runs are local-only and part of `cargo test`:

```bash
cargo test
```

Run once online to populate or refresh this mirror:

```bash
EYERON_W3C_RDF_REFRESH=1 cargo test --test w3c_rdf
```

During a full refresh, one aggregate pass refreshes the mirror and writes the EARL report; the per-manifest checks inspect that same run.

To make the 1180-test suite fully local in the repository, commit the populated `rdf-tests/` directory.

//! Checks every packaged proof document against the program it was
//! produced from, using `docs/proof-checking.md`'s reference
//! implementation.
//!
//! This is the suite that says what a proof is worth. It is not a golden
//! comparison: a proof that matched its golden byte for byte could still
//! be nonsense, so each document is re-checked here — every recorded
//! inference re-performed against the source rule, every premise resolved,
//! the derivation graph tested for cycles, and every claim accounted for.
//!
//! Uses a custom harness (`harness = false`), like the other example
//! suites, so each document prints its own line.

#[path = "support/report.rs"]
mod report;

use eyeron::proof_check::Report;
use report::{green, progress_line, red};
use std::fs;
use std::path::{Path, PathBuf};

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn sorted_proofs() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(manifest_dir().join("examples/proof"))
        .expect("read examples/proof")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("n3"))
        .collect();
    paths.sort();
    paths
}

/// The program a proof was produced from, assembled the way the example
/// suites assemble it.
fn check(name: &str) -> Result<Report, String> {
    let examples = manifest_dir().join("examples");
    let source = fs::read_to_string(examples.join(format!("{name}.n3"))).map_err(|e| e.to_string())?;
    let proof = fs::read_to_string(examples.join(format!("proof/{name}.n3"))).map_err(|e| e.to_string())?;

    let mut document =
        eyeron::parse_n3_with_source(&source, None, Some(&format!("{name}.n3"))).map_err(|e| e.message)?;
    let mut companion = examples.join(format!("input/{name}.n3"));
    if !companion.exists() {
        companion = examples.join(format!("input/{name}.trig"));
    }
    if companion.exists() {
        let text = fs::read_to_string(&companion).unwrap();
        let parsed = if eyeron::is_rdf_message_log(&text) {
            eyeron::parse_rdf_message_log(&text, None)
        } else {
            eyeron::parse_n3_with_source(&text, None, Some(&companion.to_string_lossy()))
                .or_else(|_| eyeron::parse_rdf12(&text, None, eyeron::RdfFormat::Trig))
        }
        .map_err(|e| e.message)?;
        document.merge(parsed);
    }
    eyeron::proof_check_n3::check_proof_document(&document, &proof).map_err(|e| e.message)
}

fn main() {
    let started = std::time::Instant::now();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut steps = 0usize;
    let mut verified = 0usize;
    let mut trusted = 0usize;

    {
        for path in sorted_proofs() {
            let name = path.file_stem().and_then(|s| s.to_str()).expect("utf8 name").to_string();
            let outcome = check(&name);
            let (valid, summary) = match &outcome {
                Ok(report) => {
                    steps += report.steps;
                    verified += report.verified;
                    trusted += report.obligations.len();
                    (report.valid(), report.verdict())
                }
                Err(message) => (false, format!("could not be read: {message}")),
            };

            if valid {
                passed += 1;
                progress_line(&format!("proof examples/proof/{name}.n3 ... {} ({summary})", green("ok")));
            } else {
                failed += 1;
                progress_line(&format!("proof examples/proof/{name}.n3 ... {} ({summary})", red("fail")));
                if let Ok(report) = &outcome {
                    for failure in report.failures.iter().take(2) {
                        progress_line(&format!("    [{}] {} -- {}", failure.condition, failure.conclusion, failure.detail));
                    }
                }
            }
        }
    }

    let elapsed = started.elapsed().as_secs_f64();
    progress_line(&format!(
        "\nproof_checking result: {}. {passed} passed; {failed} failed; finished in {elapsed:.2}s \
         ({steps} steps, {verified} verified, {trusted} trusted)",
        if failed == 0 { green("ok") } else { red("FAILED") },
    ));
    assert_eq!(failed, 0, "{failed} proof document(s) did not check");
}

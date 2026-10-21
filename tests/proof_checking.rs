//! Checks every packaged proof document against the program it was
//! produced from, using `docs/proof-checking.md`'s reference
//! implementation.
//!
//! This is the suite that says what a proof is worth. Comparing a proof
//! with its golden byte for byte could still accept nonsense, so each
//! document is re-checked here — every recorded
//! inference re-performed against the source rule, every premise resolved,
//! the derivation graph tested for cycles, and every claim accounted for.
//! Its CLI report is compared with `examples/check/<name>.txt`; set
//! `EYERON_UPDATE_GOLDENS=1` to refresh reports after a deliberate change.
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
    if proof.trim().is_empty() {
        return Err(format!("examples/proof/{name}.n3 is empty"));
    }

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
    let report = eyeron::proof_check_n3::check_proof_document(&document, &proof).map_err(|e| e.message)?;
    let mut sources = vec![eyeron::cli::Source {
        name: format!("examples/{name}.n3"), text: source, base_iri: None,
    }];
    if companion.exists() {
        sources.push(eyeron::cli::Source {
            name: format!("examples/input/{}", companion.file_name().unwrap().to_string_lossy()),
            text: String::new(), base_iri: None,
        });
    }
    let actual = eyeron::cli::proof_check_report(&format!("examples/proof/{name}.n3"), &sources, &report);
    let path = examples.join(format!("check/{name}.txt"));
    if std::env::var_os("EYERON_UPDATE_GOLDENS").is_some_and(|value| value == "1") {
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        if fs::read_to_string(&path).ok().as_deref() != Some(&actual) {
            fs::write(&path, &actual).map_err(|e| e.to_string())?;
        }
    } else {
        let expected = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if expected != actual {
            return Err(format!("check report does not match examples/check/{name}.txt\nactual:\n{actual}"));
        }
    }
    Ok(report)
}

/// Packaged proofs that must be rejected, and the condition that rejects
/// them. variable-predicate-dependency.n3 is an intentional negative cycle:
/// the run assumes `:a a :Blocked` is absent and then derives it, so its
/// proof contradicts itself (C6).
const EXPECTED_INVALID: &[(&str, &str)] = &[("variable-predicate-dependency", "C6")];

fn main() {
    let started = std::time::Instant::now();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut steps = 0usize;
    let mut verified = 0usize;
    let mut trusted = 0usize;

    // A report belongs to exactly one packaged proof; stale reports must
    // not survive an example being renamed or removed.
    let check_dir = manifest_dir().join("examples/check");
    if check_dir.exists() {
        for entry in fs::read_dir(check_dir).expect("read examples/check") {
            let path = entry.expect("read check report").path();
            if path.extension().and_then(|s| s.to_str()) == Some("txt") {
                let name = path.file_stem().unwrap().to_string_lossy();
                assert!(manifest_dir().join(format!("examples/proof/{name}.n3")).exists(),
                    "{} has no corresponding proof", path.display());
            }
        }
    }

    {
        for path in sorted_proofs() {
            let name = path.file_stem().and_then(|s| s.to_str()).expect("utf8 name").to_string();
            let outcome = check(&name);
            let expected = EXPECTED_INVALID.iter().find(|(invalid, _)| *invalid == name).map(|(_, condition)| *condition);
            let (valid, summary) = match &outcome {
                Ok(report) => {
                    steps += report.steps;
                    verified += report.verified;
                    trusted += report.obligations.len();
                    match expected {
                        Some(condition) => {
                            let rejected = !report.valid() && report.failures.iter().all(|failure| failure.condition == condition);
                            (rejected, format!("{}, rejected under {condition} as expected", report.verdict()))
                        }
                        None => (report.valid(), report.verdict()),
                    }
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
         ({steps} steps, {verified} verified, {trusted} trusted; {passed} check reports)",
        if failed == 0 { green("ok") } else { red("FAILED") },
    ));
    assert_eq!(failed, 0, "{failed} proof document(s) did not check");
}

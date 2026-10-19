use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eyeron"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run eyeron")
}

fn run_with_stdin(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_eyeron"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run eyeron with stdin");
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn short_and_long_commands_match_the_two_process_proof_check() {
    let source = "examples/dog-license.n3";
    let proof = run(&["-p", source]);
    assert!(
        proof.status.success(),
        "{}",
        String::from_utf8_lossy(&proof.stderr)
    );
    let separate = run_with_stdin(&["--check-proof", "-", source], &proof.stdout);
    assert!(
        separate.status.success(),
        "{}",
        String::from_utf8_lossy(&separate.stderr)
    );

    for flag in ["-c", "--prove-and-check"] {
        let combined = run(&[flag, source]);
        assert_eq!(combined.status, separate.status);
        assert_eq!(combined.stderr, separate.stderr);
        let output = String::from_utf8(combined.stdout).unwrap();
        assert!(output.starts_with("Proof: <generated>\n"), "{output}");
        assert!(output.contains("checked with obligations:"), "{output}");
        assert!(
            output.contains("Trust obligations (accepted without verifying their truth):"),
            "{output}"
        );
        // Only the proof's location differs: no intermediate document is read.
        assert_eq!(
            output.lines().skip(1).collect::<Vec<_>>(),
            String::from_utf8_lossy(&separate.stdout)
                .lines()
                .skip(1)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn generated_proof_is_checked_with_the_input_base_iri() {
    let source = b"<alice> <kind> <Human>. { ?x <kind> <Human> } => { ?x <kind> <Mortal> }.";
    let output = run_with_stdin(&["-c", "--base-iri", "https://example.org/", "-"], source);
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Source program: <stdin>"), "{text}");
    assert!(
        text.contains("checked: 2 steps, 2 verified, 0 trusted"),
        "{text}"
    );
}

#[test]
fn generated_proof_is_checked_against_separately_scoped_merged_inputs() {
    let output = run(&[
        "-c",
        "examples/shared-existential.n3",
        "examples/explicit-existential-rule.n3",
    ]);
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("checked: 6 steps, 6 verified, 0 trusted"),
        "{text}"
    );
}

#[test]
fn separately_checked_invalid_proof_still_returns_failure() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/proof/socrates.n3");
    let proof = std::fs::read_to_string(path)
        .unwrap()
        .replace("pe:value :Mortal", "pe:value :Immortal");
    let output = run_with_stdin(
        &["--check-proof", "-", "examples/socrates.n3"],
        proof.as_bytes(),
    );
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("invalid: 1 failure(s)"), "{text}");
    assert!(text.contains("[C3: step justification]"), "{text}");
}

#[test]
fn prove_and_check_rejects_conflicting_modes() {
    for other in ["--ast", "--check-proof", "--stream-messages"] {
        let mut args = vec!["-c", other];
        if other == "--check-proof" {
            args.push("examples/proof/socrates.n3");
        }
        let output = run(&args);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "eyeron: --prove-and-check cannot be combined with --ast, --check-proof, or --stream-messages\n"
        );
    }
}

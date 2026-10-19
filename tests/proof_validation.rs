use eyeron::proof_check::Report;
use eyeron::proof_check_n3::check_proof;

const PREFIX: &str = "@prefix : <http://example.org/>.\n@prefix pe: <https://eyereasoner.github.io/pe#>.\n@prefix math: <http://www.w3.org/2000/10/swap/math#>.\n@prefix log: <http://www.w3.org/2000/10/swap/log#>.\n";

fn check(source: &str, proof: &str) -> eyeron::Result<Report> {
    check_proof(
        &format!("{PREFIX}{source}"),
        &format!("{PREFIX}{proof}"),
        "test.n3",
    )
}

fn rejects_metadata(proof: &str, message: &str) {
    let error = check("{} => { :a :p :b }.", proof).expect_err("malformed metadata was accepted");
    assert!(error.message.contains(message), "{}", error.message);
}

#[test]
fn every_step_requires_exactly_one_justification() {
    for metadata in [
        "pe:fact \"first\"; pe:unproven \"missing\"",
        "pe:unproven \"missing\"; pe:fact \"last\"",
        "pe:rule 1; pe:rule 1",
        "pe:uses { :a :p :b }",
    ] {
        rejects_metadata(
            &format!("{{ :a :p :b }} {metadata}."),
            "exactly one justification",
        );
    }
}

#[test]
fn malformed_premises_and_step_subjects_cannot_disappear() {
    for used in [":missing", "{}", "{ :a :p :b. :c :p :d }"] {
        rejects_metadata(
            &format!("{{ :a :p :b }} pe:rule 1; pe:uses {used}."),
            "exactly one premise triple",
        );
    }
    for subject in [":a", "{}", "{ :a :p :b. :c :p :d }"] {
        rejects_metadata(&format!("{subject} pe:fact \"source\"."), "step subject");
    }
}

#[test]
fn malformed_bindings_are_rejected_instead_of_ignored_or_overwritten() {
    for binding in [
        "[]",
        "[ pe:var \"x\" ]",
        "[ pe:value :a ]",
        "[ pe:var :x; pe:value :a ]",
        "[ pe:var \"x\", \"y\"; pe:value :a ]",
        "[ pe:var \"x\"; pe:value :a, :b ]",
        "[ pe:var \"x\"; pe:value :a ], [ pe:var \"x\"; pe:value :b ]",
    ] {
        let proof = format!("{{ :a :p :b }} pe:rule 1; pe:binding {binding}.");
        assert!(
            check("{} => { :a :p :b }.", &proof).is_err(),
            "accepted {binding}"
        );
    }
    rejects_metadata("_:b pe:var \"x\"; pe:value :a.", "not referenced");
    rejects_metadata(
        "{ :a :p :b } pe:fact \"source\"; pe:binding [ pe:var \"x\"; pe:value :a ].",
        "only valid on a rule step",
    );
}

#[test]
fn justification_values_and_unknown_metadata_are_validated() {
    for value in [
        "0",
        "-1",
        "\"1\"",
        "\"1\"@en",
        "1.0",
        ":rule",
        "{}",
        "{ :a :p :b }",
    ] {
        rejects_metadata(
            &format!("{{ :a :p :b }} pe:rule {value}."),
            "positive integer",
        );
    }
    rejects_metadata("{ :a :p :b } pe:fact :source.", "string source label");
    rejects_metadata("{ :a :p :b } pe:builtin \"predicate\".", "predicate IRI");
    rejects_metadata("{ :a :p :b } pe:unproven :reason.", "string reason");
    rejects_metadata(
        "{ :a :p :b } pe:typo \"ignored\".",
        "unknown proof predicate",
    );
}

#[test]
fn a_source_fact_cannot_claim_extra_premises() {
    let report = check(
        ":a :p :b. :c :p :d.",
        ":a :p :b. { :a :p :b } pe:fact \"source\"; pe:uses { :c :p :d }.",
    )
    .unwrap();
    assert!(!report.valid());
    assert!(report
        .failures
        .iter()
        .any(|f| f.condition == "C3" && f.detail.contains("must not have premises")));
}

#[test]
fn builtin_labels_cannot_bypass_verification_or_trust_classification() {
    for (value, named) in [("3", "log:equalTo"), ("4", "time:localTime")] {
        let proof = format!("@prefix time: <http://www.w3.org/2000/10/swap/time#>. (1 2) math:sum {value}. {{ (1 2) math:sum {value} }} pe:builtin {named}.");
        let report = check("", &proof).unwrap();
        assert!(!report.valid());
        assert!(report.obligations.is_empty());
        assert!(report
            .failures
            .iter()
            .any(|f| f.condition == "C3" && f.detail.contains("conclusion has predicate")));
    }
    let valid = check(
        "",
        "(1 2) math:sum 3. { (1 2) math:sum 3 } pe:builtin math:sum.",
    )
    .unwrap();
    assert!(valid.valid());
    assert_eq!(valid.verified, 1);
}

#[test]
fn formula_subjects_and_rule_statements_are_claims_too() {
    for claim in [
        "{ :a :p :b } :says :c.",
        "{} => { :a :p :b }.",
        "{ :a :p :b } <= {}.",
        "{} => false.",
        "{} log:query { :a :p :b }.",
    ] {
        let missing = check("", claim).unwrap();
        assert!(!missing.valid(), "unjustified claim accepted: {claim}");
        assert!(missing.failures.iter().any(|f| f.condition == "C4"));
        let given = check(claim, claim).unwrap();
        assert!(
            given.valid(),
            "given claim rejected: {claim}: {:?}",
            given.failures
        );
    }
    // A fuse assertion and a query directive cannot be normalized to ordinary rules.
    assert!(!check("{} => {}.", "{} => false.").unwrap().valid());
    assert!(!check("{} => { :a :p :b }.", "{} log:query { :a :p :b }.")
        .unwrap()
        .valid());
}

#[test]
fn carried_rule_provenance_participates_in_cycle_detection() {
    let source = "{ :a :p :b } => { {} => { :a :p :b } }.";
    let proof = "{} => { :a :p :b }. :a :p :b.\n\
        { {} => { :a :p :b } } pe:rule 1; pe:uses { :a :p :b }.\n\
        { :a :p :b } pe:rule { {} => { :a :p :b } }.";
    let report = check(source, proof).unwrap();
    assert_eq!(
        report.verified, 2,
        "the individual rule applications should match"
    );
    assert!(
        report.failures.iter().any(|f| f.condition == "C2"),
        "{:?}",
        report.failures
    );
}

#[test]
fn carried_rules_with_acyclic_provenance_still_check() {
    let source = ":seed :p :o. { :seed :p :o } => { {} => { :a :p :b } }.";
    let proof = "{} => { :a :p :b }. :a :p :b.\n\
        { {} => { :a :p :b } } pe:rule 1; pe:uses { :seed :p :o }.\n\
        { :seed :p :o } pe:fact \"source\".\n\
        { :a :p :b } pe:rule { {} => { :a :p :b } }.";
    let report = check(source, proof).unwrap();
    assert!(report.valid(), "{:?}", report.failures);
    assert_eq!(report.verified, 3);
}

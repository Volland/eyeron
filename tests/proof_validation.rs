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

fn rejects(source: &str, proof: &str, message: &str) {
    let report = check(source, proof).expect("proof should parse");
    assert!(!report.valid(), "forged proof was accepted");
    assert!(
        report.failures.iter().any(|failure| failure.detail.contains(message)),
        "{:?}",
        report.failures
    );
}

#[test]
fn a_carried_rule_cannot_be_more_general_than_the_rule_it_claims_to_be() {
    let source = ":Socrates a :Human. { ?S a :Human } => { ?S a :Mortal }.";
    rejects(
        source,
        ":Moon :madeOf :Cheese.\n\
         { :Moon :madeOf :Cheese } pe:rule { { ?X ?Y ?Z } => { ?A ?B ?C } }; pe:uses { :Socrates a :Human }.\n\
         { :Socrates a :Human } pe:fact \"test.n3\".",
        "nothing in the proof derives",
    );
    rejects(source, "{ ?X ?Y ?Z } => { ?A ?B ?C }.", "the source does not give it");
}

#[test]
fn an_existential_is_witnessed_by_a_fresh_blank_for_one_firing_only() {
    let source = ":a a :Human. :b a :Human. { ?S a :Human } => { ?S :hasParent _:p }.";
    let facts = "{ :a a :Human } pe:fact \"test.n3\".\n{ :b a :Human } pe:fact \"test.n3\".\n";
    let firing = |subject: &str, witness: &str| {
        format!("{{ {subject} :hasParent {witness} }} pe:rule 1; pe:binding [ pe:var \"S\"; pe:value {subject} ]; pe:uses {{ {subject} a :Human }}.\n")
    };
    rejects(source, &format!("{facts}{}", firing(":a", ":Zeus")), "not a fresh blank node");
    rejects(source, &format!("{facts}{}", firing(":a", ":a")), "not a fresh blank node");
    rejects(
        source,
        &format!("{facts}{}{}", firing(":a", "_:w"), firing(":b", "_:w")),
        "another firing already introduced",
    );
    let report = check(source, &format!("{facts}{}{}", firing(":a", "_:w1"), firing(":b", "_:w2"))).unwrap();
    assert!(report.valid(), "{:?}", report.failures);
}

#[test]
fn a_builtin_step_must_state_its_values() {
    rejects(
        ":x :vals (1 2). { ?x :vals (?a ?b). (?a ?b) math:sum ?c } => { ?x :total ?c }.",
        ":x :total ?z.\n\
         { :x :total ?z } pe:rule 1;\n\
           pe:binding [ pe:var \"a\"; pe:value 1 ], [ pe:var \"b\"; pe:value 2 ], [ pe:var \"c\"; pe:value ?z ], [ pe:var \"x\"; pe:value :x ];\n\
           pe:uses { :x :vals (1 2) }, { (1 2) math:sum ?z }.\n\
         { :x :vals (1 2) } pe:fact \"test.n3\".\n\
         { (1 2) math:sum ?z } pe:builtin math:sum.",
        "unbound",
    );
}

#[test]
fn a_query_answer_is_output_and_never_a_premise() {
    let source = ":s a :Human. { ?x a :Human } log:query { ?x a :Answer }. { ?x a :Answer } => { ?x :bad true }.";
    let answer = "{ :s a :Answer } pe:rule { { ?x a :Human } => { ?x a :Answer } }; pe:binding [ pe:var \"x\"; pe:value :s ]; pe:uses { :s a :Human }.\n\
                  { :s a :Human } pe:fact \"test.n3\".\n";
    rejects(
        source,
        &format!(":s :bad true.\n{{ :s :bad true }} pe:rule 1; pe:binding [ pe:var \"x\"; pe:value :s ]; pe:uses {{ :s a :Answer }}.\n{answer}"),
        "the query answer",
    );
    let report = check(source, &format!(":s a :Answer.\n{answer}")).unwrap();
    assert!(report.valid(), "{:?}", report.failures);
}

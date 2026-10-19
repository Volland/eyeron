use eyeron::{parse_n3_with_source, proof_to_n3, reason_document, ReasonerOptions};

fn checked_proof(source: &str) -> String {
    let document = parse_n3_with_source(source, None, Some("test.n3")).unwrap();
    let result = reason_document(
        &document,
        &ReasonerOptions {
            proof: true,
            ..ReasonerOptions::default()
        },
    );
    assert!(result.is_complete(), "{:?}", result.errors);
    assert!(
        !result.derived.is_empty(),
        "the program must derive a claim"
    );
    let proof = proof_to_n3(&document.prefixes, &result);
    let report = eyeron::proof_check_n3::check_proof_document(&document, &proof).unwrap();
    assert!(report.valid(), "{:?}\n{proof}", report.failures);
    proof
}

#[test]
fn an_open_closure_instance_has_its_own_complete_derivation() {
    let proof = checked_proof(
        r#"
        @prefix : <http://example.org/>.
        @prefix log: <http://www.w3.org/2000/10/swap/log#>.
        :case :theory { :alice :p :b. { ?User :p :b } => { ?User :q :b } }.
        { :case :theory ?Theory. ?Theory log:conclusion ?Closure }
            => { :case :closure ?Closure }.
        { :case :closure ?C. ?C log:includes { ?User :q :b } }
            => { :case :witness ?User; :copied ?C }.
    "#,
    );
    let parsed = parse_n3_with_source(&proof, None, Some("proof.n3")).unwrap();
    let closure = parsed
        .facts
        .iter()
        .find(|t| t.p == eyeron::Term::iri("http://example.org/closure"))
        .unwrap();
    let copied = parsed
        .facts
        .iter()
        .find(|t| t.p == eyeron::Term::iri("http://example.org/copied"))
        .unwrap();
    assert_ne!(
        closure.o, copied.o,
        "the outer binding instantiates the open graph"
    );
    assert!(proof.contains("pe:var \"User\"; pe:value :alice"));
}

#[test]
fn a_prebound_outer_variable_can_use_an_open_closure_instance() {
    checked_proof(
        r#"
        @prefix : <http://example.org/>.
        @prefix log: <http://www.w3.org/2000/10/swap/log#>.
        :alice :enabled true.
        :case :theory { :alice :p :b. { ?User :p :b } => { ?User :q :b } }.
        { :case :theory ?Theory. ?Theory log:conclusion ?Closure }
            => { :case :closure ?Closure }.
        { ?User :enabled true. :case :closure ?C. ?C log:includes { ?User :q :b } }
            => { :case :witness ?User }.
    "#,
    );
}

#[test]
fn backward_explanations_use_the_reasoners_premise_order() {
    checked_proof(
        r#"
        @prefix : <urn:test:>.
        @prefix log: <http://www.w3.org/2000/10/swap/log#>.
        @prefix list: <http://www.w3.org/2000/10/swap/list#>.
        { ?Input :prepare ?Output } <= { ?Input log:equalTo ?Input. ?Input log:equalTo ?Output }.
        () :copy ().
        { ?Input :copy ?Output } <= {
            ((?Head) ?Tail) list:append ?Input.
            ?Tail :prepare ?Prepared.
            ?Prepared :copy ?Rest.
            ((?Head) ?Rest) list:append ?Output.
        }.
        { (2 3) :copy ?Output } => { :answer :value ?Output }.
    "#,
    );
}

// The completed closure can contain a matching premise whose derivation is
// unavailable to backward explanation. That match must not commit partial
// steps, cache them as done, or prevent another support from being tried.
fn checked_with_closure(source: &str, extra: &str) -> String {
    let prefix = "@prefix : <urn:test:>.\n";
    let document =
        parse_n3_with_source(&format!("{prefix}{source}"), None, Some("test.n3")).unwrap();
    let mut result = reason_document(
        &document,
        &ReasonerOptions {
            proof: true,
            ..ReasonerOptions::default()
        },
    );
    assert!(result.is_complete());
    assert!(!result.derived.is_empty());
    result.closure.extend(
        parse_n3_with_source(&format!("{prefix}{extra}"), None, Some("closure.n3"))
            .unwrap()
            .facts,
    );
    let proof = proof_to_n3(&document.prefixes, &result);
    let report = eyeron::proof_check_n3::check_proof_document(&document, &proof).unwrap();
    assert!(report.valid(), "{:?}\n{proof}", report.failures);
    proof
}

#[test]
fn a_cyclic_backward_candidate_does_not_hide_a_later_valid_rule() {
    checked_with_closure(
        r#"
        :seed :p :ok.
        { :answer :p :ok } <= { :middle :p :ok }.
        { :middle :p :ok } <= { :answer :p :ok }.
        { :middle :p :ok } <= { :seed :p :ok }.
        { :answer :p :ok } => { :result :p :ok }.
    "#,
        ":answer :p :ok. :middle :p :ok.",
    );
}

#[test]
fn an_incomplete_body_match_is_rolled_back_before_trying_the_next() {
    checked_with_closure(
        r#"
        :seed :p :ok. :candidate :p :bad, :good. :good :p :ok.
        { :shared :p :ok } <= { :seed :p :ok }.
        { :answer :p :ok } <= {
            :shared :p :ok. :candidate :p ?Candidate. ?Candidate :p :ok.
        }.
        { :answer :p :ok } => { :result :p :ok }.
    "#,
        ":bad :p :ok. :shared :p :ok.",
    );
}

#[test]
fn the_four_previously_invalid_example_proofs_check_when_generated() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for name in [
                "odrl-dpv-campaign-audit",
                "odrl-dpv-conflict-audit",
                "odrl-policy-audit",
                "polynomial",
            ] {
                let source = std::fs::read_to_string(format!(
                    "{}/examples/{name}.n3",
                    env!("CARGO_MANIFEST_DIR")
                ))
                .unwrap();
                checked_proof(&source);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

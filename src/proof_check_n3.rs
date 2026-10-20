//! Reader and step checker for Notation3 proof documents (specification
//! §8).
//!
//! A proof is an N3 document, so it is read with the ordinary N3 parser and
//! its top-level steps are lifted into the abstract model. Checking a
//! `pe:rule` step re-performs exactly one rule application against the
//! cited source rule or justified carried rule: instantiate its premises
//! and conclusion under the step's bindings, and require them to be what
//! the step recorded.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{Term, Triple};
use crate::error::{EyeronError, Result};
use crate::reasoner::Bindings;

use crate::proof_check::{check, Checked, Document as ProofDocument, Kind, Report, Resolution};

const PE: &str = "https://eyereasoner.github.io/pe#";

/// Built-ins whose value is not a function of the triple they appear in, so
/// re-evaluating them cannot confirm anything (specification §5.3, §6.1).
/// Each reads something outside the proof: the clock, the process, or the
/// network.
const IMPURE_BUILTINS: &[&str] = &[
    // Read the clock.
    "http://www.w3.org/2000/10/swap/time#localTime",
    "http://www.w3.org/2000/10/swap/time#currentTime",
    "http://www.w3.org/2000/10/swap/time#gmTime",
    // Read the network or the filesystem.
    "http://www.w3.org/2000/10/swap/log#content",
    "http://www.w3.org/2000/10/swap/log#semantics",
    "http://www.w3.org/2000/10/swap/log#semanticsOrError",
    // Read a random source.
    "http://www.w3.org/2000/10/swap/math#random",
    // Read the rest of the fact set, which is the reasoner's state rather
    // than anything the step itself carries: whether these hold depends on
    // what else had been derived at the time, so a checker looking only at
    // this triple cannot re-establish them.
    "http://www.w3.org/2000/10/swap/log#collectAllIn",
    "http://www.w3.org/2000/10/swap/log#forAllIn",
    "http://www.w3.org/2000/10/swap/log#includes",
    "http://www.w3.org/2000/10/swap/log#notIncludes",
    "http://www.w3.org/2000/10/swap/log#conclusion",
    "http://www.w3.org/2000/10/swap/log#conjunction",
    // Applies a predicate to each element, which means looking the
    // predicate up in the fact set.
    "http://www.w3.org/2000/10/swap/list#map",
];

type Verdict = std::result::Result<Checked, String>;

fn pe(name: &str) -> Term {
    Term::Iri(format!("{}{}", PE, name).into())
}

/// True iff `predicate` is one of the step vocabulary's own terms, which
/// describe the proof rather than state a claim.
fn is_proof_vocabulary(predicate: &Term) -> bool {
    matches!(predicate, Term::Iri(iri) if iri.starts_with(PE))
}

/// The single triple a `{ ... }` proof term wraps.
fn formula_triple(term: &Term) -> Option<&Triple> {
    match term {
        Term::Formula(triples) if triples.len() == 1 => Some(&triples[0]),
        _ => None,
    }
}

struct Step {
    conclusion: Triple,
    kind: Kind,
    rule: Option<usize>,
    /// A rule the engine generated while reasoning, carried by the step
    /// because no document contains it (§5.1).
    carried: Option<(crate::ast::Graph, crate::ast::Graph, Triple)>,
    builtin: Option<Term>,
    bindings: Vec<(String, Term)>,
    uses: Vec<Triple>,
}

pub struct N3Proof {
    rules: Vec<crate::ast::Rule>,
    given: BTreeSet<Triple>,
    /// Written rules, as the statements they are, for a premise or a
    /// `pe:fact` step that names one up to renaming.
    rule_statements: Vec<Triple>,
    /// Given statements that carry variables. N3 reads those as
    /// universally quantified, so each gives every instance of itself.
    general: Vec<Triple>,
    /// Query rules, as the implications a step answering one carries. An
    /// answer is output, so no step may use it as a premise.
    query_rules: Vec<Triple>,
    /// Blank nodes the source mentions, which no existential can stand for.
    source_blanks: BTreeSet<crate::ast::Name>,
    steps: Vec<Step>,
    claims: Vec<Triple>,
    index: BTreeMap<Triple, usize>,
    /// Each existential witness, with the firing that introduced it.
    witnesses: RefCell<BTreeMap<crate::ast::Name, Firing>>,
}

/// A rule application: the rule, and the bindings of the head's variables
/// it fired under.
type Firing = (String, Vec<(crate::ast::Name, Term)>);

/// Read `proof` as a proof document for the program in `source`, and check
/// it.
pub fn check_proof(source: &str, proof: &str, label: &str) -> Result<Report> {
    let document = crate::parser::parse_n3_with_source(source, None, Some(label))?;
    check_proof_document(&document, proof)
}

/// As `check_proof`, but for a source already parsed — an example assembled
/// from several documents has to be merged before it is numbered.
pub fn check_proof_document(source: &crate::ast::Document, proof: &str) -> Result<Report> {
    let document = N3Proof::read_document(source, proof)?;
    Ok(check(&document))
}

impl N3Proof {
    pub fn read_document(document: &crate::ast::Document, proof: &str) -> Result<Self> {
        // `reason` numbers the rules it was given, having dropped the query
        // rules; a checker must number them the same way (§5.1).
        let rules: Vec<crate::ast::Rule> = document.rules.iter().filter(|rule| !rule.is_query).cloned().collect();
        // N3 treats a rule as data, so a rule written in the document is
        // itself a statement the document gives. That includes the query
        // rules left out of the numbering above, as `log:query` statements.
        // A step recording a query answer carries its rule as an
        // implication instead, which is kept apart in `query_rules`.
        let mut given: BTreeSet<Triple> = document.facts.iter().cloned().collect();
        let rule_statements: Vec<Triple> = document.rules.iter().map(written_rule_statement).collect();
        given.extend(rule_statements.iter().cloned());
        let query_rules: Vec<Triple> = document.rules.iter().filter(|rule| rule.is_query).map(crate::proof_writer::rule_statement).collect();
        let source_blanks = triple_blank_labels(&document.facts);
        let general: Vec<Triple> = document.facts.iter().filter(|fact| !fact.is_ground()).cloned().collect();

        let parsed = crate::parser::parse_n3(proof, None)?;
        // A step is a formula subject carrying the `pe:` vocabulary; what
        // the document claims is written plainly alongside.
        let steps = read_steps(&parsed.facts)?;
        let mut claims: Vec<Triple> = parsed
            .facts
            .iter()
            .filter(|triple| !is_proof_vocabulary(&triple.p))
            .cloned()
            .collect();
        claims.extend(parsed.rules.iter().map(written_rule_statement));

        let mut index = BTreeMap::new();
        for (position, step) in steps.iter().enumerate() {
            index.entry(step.conclusion.clone()).or_insert(position);
        }
        Ok(Self { rules, given, rule_statements, general, query_rules, source_blanks, steps, claims, index, witnesses: RefCell::default() })
    }

    fn resolve(&self, statement: &Triple) -> Resolution {
        // N3 derives ground triples, so resolution is equality; the
        // instance case (C1) cannot arise without variables to instantiate.
        if let Some(position) = self.index.get(statement) {
            return Resolution::Step(*position);
        }
        // Closed graph terms keep their identity under consistent local
        // quantifier renaming, including between separately printed steps.
        if statement.is_ground() {
            if let Some(position) = self.steps.iter().position(|step| ground_statements_equal(&step.conclusion, statement)) {
                return Resolution::Step(position);
            }
        }
        if self.given.contains(statement) || self.gives_rule(statement) {
            return Resolution::Given;
        }
        Resolution::Unresolved(describe(statement))
    }

    /// Is the rule this step carries one the proof derives? Backward
    /// search renames a rule's variables apart, so the carried statement is
    /// a variant of the derived one rather than equal to it; a rule is the
    /// same rule under consistent renaming, so this matches instead of
    /// comparing.
    fn resolve_rule(&self, statement: &Triple) -> Resolution {
        if self.given.iter().any(|candidate| matches_rule(candidate, statement)) {
            return Resolution::Given;
        }
        if let Some(index) = self.steps.iter().position(|step| matches_rule(&step.conclusion, statement)) {
            return Resolution::Step(index);
        }
        if self.query_rules.iter().any(|candidate| matches_rule(candidate, statement)) {
            return Resolution::Given;
        }
        Resolution::Unresolved(format!("carried rule {}", describe(statement)))
    }

    /// Does this step answer a query, carrying a query rule and no rule
    /// that may feed further inference?
    fn answers_query(&self, step: &Step) -> bool {
        let Some((_, _, statement)) = &step.carried else { return false };
        self.query_rules.iter().any(|candidate| matches_rule(candidate, statement))
            && !self.given.iter().chain(self.steps.iter().map(|step| &step.conclusion)).any(|candidate| matches_rule(candidate, statement))
    }

    /// §5.1: re-perform the one rule application this step records.
    fn check_rule(&self, step: &Step) -> Verdict {
        // A generated rule is carried by the step; a written one is taken
        // from the source, never from the document.
        let (premise, conclusion, names, number) = match &step.carried {
            Some((premise, conclusion, statement)) => {
                // The carried rule has to be justified too, or a step could
                // invent any rule it liked.
                if matches!(self.resolve_rule(statement), Resolution::Unresolved(_)) {
                    return Err("carries a generated rule that nothing in the proof derives".to_string());
                }
                (premise.clone(), conclusion.clone(), BTreeMap::new(), "the generated rule it carries".to_string())
            }
            None => {
                let number = step.rule.ok_or_else(|| "cites no rule".to_string())?;
                let rule = self.rules.get(number.wrapping_sub(1)).ok_or_else(|| format!("cites rule {}, which the source does not have", number))?;
                (rule.premise.clone(), rule.conclusion.clone(), rule.proof_var_source_names.clone(), format!("rule {}", number))
            }
        };

        // The proof records a variable by its source name; the rule knows
        // it by its internal one.
        let mut source_names: BTreeMap<&str, &str> = BTreeMap::new();
        for (internal, source) in &names {
            source_names.insert(source.as_str(), internal.as_str());
        }
        let mut bindings = Bindings::new();
        for (name, value) in &step.bindings {
            let internal = source_names.get(name.as_str()).copied().unwrap_or(name.as_str());
            bindings.insert(internal.to_string().into(), value.clone());
        }

        // `{ :a :b ?C. } => ?C.` takes its conclusion from a term resolved
        // when the rule fires, which the rule alone does not state. The step
        // records the binding, so the conclusion is recoverable from there.
        let unquoted = unquoted_conclusion(&conclusion, &bindings);
        let conclusion = unquoted.unwrap_or(conclusion);

        let rule = RuleView { premise: &premise, conclusion: &conclusion };
        if rule.premise.len() != step.uses.len() {
            return Err(format!("uses {} premise(s), but {} has {}", step.uses.len(), number, rule.premise.len()));
        }
        // One environment across every premise and the conclusion, so a
        // variable the bindings did not mention is still forced to take one
        // consistent value (§5.1). Only the rule's own variables bind: the
        // step may not instantiate itself to meet the rule halfway.
        for (position, (premise, used)) in rule.premise.iter().zip(step.uses.iter()).enumerate() {
            if !match_triple(premise, used, &mut bindings) {
                return Err(format!("premise {} is {}, but {} requires {}", position + 1, describe(used), number, describe(premise)));
            }
        }
        let universal: Vec<(crate::ast::Name, Term)> = bindings.iter()
            .filter(|(name, _)| !name.starts_with("_:")).map(|(name, value)| (name.clone(), value.clone())).collect();
        // Firings that agree on the head's variables conclude the same
        // instance of it, so they share its witnesses.
        let head_variables = conclusion.free_variables();
        let frontier: Vec<(crate::ast::Name, Term)> = universal.iter()
            .filter(|(name, _)| head_variables.contains(&Term::Var(name.clone()))).cloned().collect();
        let Some(bindings) = rule.conclusion.iter().find_map(|candidate| {
            let mut trial = bindings.clone();
            match_triple(candidate, &step.conclusion, &mut trial).then_some(trial)
        }) else {
            return Err(format!("does not follow from {}: it concludes none of what this step claims", number));
        };

        // A blank node scoped to the head is an existential: the engine
        // instantiates it with a fresh blank per firing. So its witness
        // must be a blank the source and this firing's values do not
        // mention, and no other firing may claim the same one. A blank
        // quantified outside the rule is one individual for every firing.
        // Which of a firing's existentials a witness stands for is left
        // open: head triples that differ only in their blank match alike.
        let mut used_blanks = BTreeSet::new();
        for (_, value) in &universal {
            blank_labels(value, &mut used_blanks);
        }
        for label in &conclusion.existentials {
            let Some(witness) = bindings.get(format!("_:{}", label).as_str()) else { continue };
            let Term::Blank(witness) = witness else {
                return Err(format!("instantiates existential _:{} of {} with {}, which is not a fresh blank node", label, number,
                    crate::printing::term_to_n3_object(witness, &BTreeMap::new())));
            };
            if self.source_blanks.contains(witness) || used_blanks.contains(witness) {
                return Err(format!("instantiates existential _:{} of {} with _:{}, which is not fresh", label, number, witness));
            }
            let firing = (number.clone(), frontier.clone());
            let mut witnesses = self.witnesses.borrow_mut();
            match witnesses.get(witness) {
                Some(other) if *other != firing => {
                    return Err(format!("instantiates existential _:{} of {} with _:{}, which another firing already introduced", label, number, witness));
                }
                Some(_) => {}
                None => { witnesses.insert(witness.clone(), firing); }
            }
        }
        Ok(Checked::Verified)
    }

    /// A written rule, matched up to renaming: the engine standardizes a
    /// rule's variables apart before using it.
    fn gives_rule(&self, statement: &Triple) -> bool {
        self.rule_statements
            .iter()
            .chain(self.general.iter())
            .any(|candidate| candidate.p == statement.p && match_triple(candidate, statement, &mut Bindings::new()))
            || self.given.iter().any(|candidate| ground_statements_equal(candidate, statement))
    }

    fn check_fact(&self, step: &Step) -> Verdict {
        if !step.uses.is_empty() {
            return Err("a source fact step must not have premises".to_string());
        }
        if self.given.contains(&step.conclusion) || self.gives_rule(&step.conclusion) {
            Ok(Checked::Verified)
        } else {
            Err("is justified as a fact, but the source does not give it".to_string())
        }
    }

    /// §5.3: re-evaluate the built-in on the step's own triple.
    fn check_builtin(&self, step: &Step) -> Verdict {
        let Some(Term::Iri(name)) = step.builtin.clone() else {
            return Err("names no built-in".to_string());
        };
        if step.builtin.as_ref() != Some(&step.conclusion.p) {
            return Err(format!("names built-in {}, but the conclusion has predicate {}", name,
                crate::printing::term_to_n3_predicate(&step.conclusion.p, &BTreeMap::new())));
        }
        if IMPURE_BUILTINS.contains(&name.as_str()) {
            return Ok(Checked::Trusted("impure built-in"));
        }
        // Re-evaluation fills in whatever is left open, which would turn a
        // computed value into a universal claim.
        let triple = &step.conclusion;
        if [&triple.s, &triple.p, &triple.o].into_iter().any(has_open_variable) {
            return Err(format!("leaves a variable of {} unbound", name));
        }
        if crate::reasoner::verify_builtin_triple(&step.conclusion) {
            Ok(Checked::Verified)
        } else {
            Err(format!("re-evaluating {} did not hold", name))
        }
    }
}

/// Is `statement` the rule `candidate` asserts, up to renaming and the
/// direction its implication is written in?
fn matches_rule(candidate: &Triple, statement: &Triple) -> bool {
    if candidate.p == statement.p && match_triple(candidate, statement, &mut Bindings::new()) {
        return true;
    }
    // Forward and backward implication assert the same rule with
    // their subject and object reversed. Quantified assertions are
    // registered as empty-premise backward rules for goal search.
    let reversed = match &candidate.p {
        Term::Iri(iri) if iri.as_str() == crate::ast::LOG_IMPLIES => Some(crate::ast::LOG_IMPLIED_BY),
        Term::Iri(iri) if iri.as_str() == crate::ast::LOG_IMPLIED_BY => Some(crate::ast::LOG_IMPLIES),
        _ => None,
    };
    if let Some(predicate) = reversed {
        let equivalent = Triple::new(candidate.o.clone(), Term::iri(predicate), candidate.s.clone());
        if equivalent.p == statement.p && match_triple(&equivalent, statement, &mut Bindings::new()) {
            return true;
        }
    }
    false
}

/// Preserve query predicates and false conclusions when lifting rule claims.
fn written_rule_statement(rule: &crate::ast::Rule) -> Triple {
    let mut statement = crate::proof_writer::rule_statement(rule);
    if rule.is_query { statement.p = Term::iri(crate::ast::LOG_QUERY); }
    if rule.is_fuse {
        statement.o = Term::Literal(crate::ast::Literal {
            value: "false".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()),
            language: None,
        });
    }
    statement
}

/// Ground statements compare their closed graph terms modulo local renaming.
fn ground_statements_equal(left: &Triple, right: &Triple) -> bool {
    left.is_ground() && right.is_ground()
        && crate::reasoner::terms_equal_semantic(&left.s, &right.s)
        && crate::reasoner::terms_equal_semantic(&left.p, &right.p)
        && crate::reasoner::terms_equal_semantic(&left.o, &right.o)
}

/// One-way matching of a rule's pattern against a recorded statement.
/// Only the pattern's variables bind. A blank node in a rule head is an
/// existential, which the engine skolemizes, so it binds too — under a
/// name that cannot collide with a variable's.
pub(crate) fn match_term(pattern: &Term, target: &Term, bindings: &mut Bindings) -> bool {
    let key = match pattern {
        Term::Var(name) => Some(name.clone()),
        Term::Blank(label) => Some(format!("_:{}", label)).map(Into::into),
        _ => None,
    };
    if let Some(key) = key {
        return match bindings.get(&key) {
            Some(bound) => bound == target,
            None => {
                bindings.insert(key, target.clone());
                true
            }
        };
    }
    match (pattern, target) {
        (Term::List(left), Term::List(right)) => {
            left.len() == right.len() && left.iter().zip(right.iter()).all(|(l, r)| match_term(l, r, bindings))
        }
        (Term::Formula(left), Term::Formula(right)) => {
            // Free existential placeholders belong to the enclosing rule;
            // let them bind just as blank patterns in ordinary triples do.
            // Locally quantified blanks remain rigid alpha variables.
            let mapping = left.free_variables().into_iter().filter_map(|term| {
                if let Term::Blank(name) = &term {
                    Some((term.clone(), Term::var(format!("_:{}", name))))
                } else {
                    None
                }
            }).collect();
            let left = left.substitute(&mapping);
            // A recorded use substitutes the rule's free variables. Its
            // blank values are identities from the recorded environment,
            // even when the proof's quoted syntax locally binds their labels.
            let mut captured = BTreeSet::new();
            for free in left.free_variables() {
                if let Term::Var(n) = free {
                    if let Some(value) = bindings.get(&n) { blank_labels(value, &mut captured); }
                }
            }
            fn recorded_term(term: &Term, captured: &BTreeSet<crate::ast::Name>) -> Term {
                match term {
                    Term::Formula(g) => Term::Formula(recorded_graph(g,captured)),
                    Term::List(items) => Term::List(items.iter().map(|t|recorded_term(t,captured)).collect()),
                    _ => term.clone(),
                }
            }
            fn recorded_graph(graph: &crate::ast::Graph, captured: &BTreeSet<crate::ast::Name>) -> crate::ast::Graph {
                let mut out=graph.map_triples(|t|Triple::new(recorded_term(&t.s,captured),recorded_term(&t.p,captured),recorded_term(&t.o,captured)));
                out.existentials.retain(|n|!captured.contains(n));
                out
            }
            let recorded = recorded_graph(right,&captured);
            // Matching is one-way: the recorded side's free variables are
            // rigid, or a proof could carry `{ ?a ?b ?c } => { ?d ?e ?f }`
            // as an instance of any rule. Freeze them as IRIs no source can
            // spell for the unification, and thaw what it bound.
            let frozen: BTreeMap<Term, Term> = recorded.free_variables().into_iter()
                .filter_map(|term| match &term {
                    Term::Var(name) => Some((term.clone(), Term::iri(format!("{}{}", FROZEN, name)))),
                    _ => None,
                })
                .collect();
            if frozen.is_empty() {
                return crate::reasoner::unify_term(&Term::Formula(left), &Term::Formula(recorded), bindings);
            }
            let thawed: BTreeMap<Term, Term> = frozen.iter().map(|(var, iri)| (iri.clone(), var.clone())).collect();
            let mut trial: Bindings = bindings.iter().map(|(name, value)| (name.clone(), replace_terms(value, &frozen))).collect();
            if !crate::reasoner::unify_term(&Term::Formula(left), &Term::Formula(recorded.substitute(&frozen)), &mut trial) {
                return false;
            }
            *bindings = trial.into_iter().map(|(name, value)| (name, replace_terms(&value, &thawed))).collect();
            true
        }
        _ => pattern == target,
    }
}

/// Where `match_term` parks a recorded variable while it is held rigid.
const FROZEN: &str = "urn:eyeron:proof-check:frozen:";

fn replace_terms(term: &Term, mapping: &BTreeMap<Term, Term>) -> Term {
    if let Some(value) = mapping.get(term) {
        return value.clone();
    }
    match term {
        Term::List(items) => Term::List(items.iter().map(|item| replace_terms(item, mapping)).collect()),
        Term::Formula(graph) => Term::Formula(graph.substitute(mapping)),
        _ => term.clone(),
    }
}

/// A variable outside any graph term: a built-in step has to state the
/// values it was evaluated on, not leave them open.
fn has_open_variable(term: &Term) -> bool {
    match term {
        Term::Var(_) => true,
        Term::List(items) => items.iter().any(has_open_variable),
        _ => false,
    }
}

/// Blank labels a term mentions, including those free in its graph terms.
fn blank_labels(term: &Term, out: &mut BTreeSet<crate::ast::Name>) {
    match term {
        Term::Blank(name) => { out.insert(name.clone()); }
        Term::List(items) => for item in items { blank_labels(item, out); },
        Term::Formula(graph) => for free in graph.free_variables() { blank_labels(&free, out); },
        _ => {}
    }
}

fn triple_blank_labels(triples: &[Triple]) -> BTreeSet<crate::ast::Name> {
    let mut out = BTreeSet::new();
    for triple in triples {
        for term in [&triple.s, &triple.p, &triple.o] {
            blank_labels(term, &mut out);
        }
    }
    out
}

pub(crate) fn match_triple(pattern: &Triple, target: &Triple, bindings: &mut Bindings) -> bool {
    match_term(&pattern.s, &target.s, bindings) && match_term(&pattern.p, &target.p, bindings) && match_term(&pattern.o, &target.o, bindings)
}

/// The triples a rule concludes when its conclusion is not written out but
/// resolved from a term at firing time. `None` when the rule states its
/// conclusion in the ordinary way.
fn unquoted_conclusion(conclusion: &[Triple], bindings: &Bindings) -> Option<crate::ast::Graph> {
    let [only] = conclusion else { return None };
    let unquote = Term::Iri(crate::ast::EYERON_UNQUOTE.to_string().into());
    if only.s != unquote || only.p != unquote {
        return None;
    }
    let resolved = match &only.o {
        Term::Var(name) => bindings.get(name)?,
        other => other,
    };
    match resolved {
        Term::Formula(triples) => Some(triples.clone()),
        _ => None,
    }
}

/// A rule's two halves, however the step got hold of them.
struct RuleView<'a> {
    premise: &'a [Triple],
    conclusion: &'a [Triple],
}

fn describe(triple: &Triple) -> String {
    crate::printing::triples_to_n3(&BTreeMap::new(), std::slice::from_ref(triple)).trim().to_string()
}

/// Lift a proof document's triples into steps, grouping by the
/// formula subject each step is about.
fn read_steps(body: &[Triple]) -> Result<Vec<Step>> {
    #[derive(Default)]
    struct Binding<'a> {
        name: Option<&'a str>,
        value: Option<&'a Term>,
        used: bool,
    }
    let mut bindings: BTreeMap<&Term, Binding<'_>> = BTreeMap::new();
    let mut order = Vec::new();
    let mut grouped: BTreeMap<&Term, Vec<&Triple>> = BTreeMap::new();
    for triple in body {
        if !is_proof_vocabulary(&triple.p) { continue; }
        let Term::Iri(predicate) = &triple.p else { unreachable!() };
        match &predicate[PE.len()..] {
            "var" | "value" => {
                if !matches!(triple.s, Term::Blank(_) | Term::Iri(_)) {
                    return Err(metadata_error(&triple.s, "a binding must be identified by a blank node or IRI"));
                }
                let binding = bindings.entry(&triple.s).or_default();
                if triple.p == pe("var") {
                    let name = plain_string(&triple.o).filter(|name| !name.is_empty())
                        .ok_or_else(|| metadata_error(&triple.s, "pe:var requires a nonempty string"))?;
                    if binding.name.replace(name).is_some() {
                        return Err(metadata_error(&triple.s, "a binding must have exactly one pe:var"));
                    }
                } else if binding.value.replace(&triple.o).is_some() {
                    return Err(metadata_error(&triple.s, "a binding must have exactly one pe:value"));
                }
            }
            "rule" | "fact" | "builtin" | "unproven" | "uses" | "binding" => {
                if formula_triple(&triple.s).is_none() {
                    return Err(metadata_error(&triple.s, "a step subject must be a graph containing exactly one triple"));
                }
                if !grouped.contains_key(&triple.s) { order.push(&triple.s); }
                grouped.entry(&triple.s).or_default().push(triple);
            }
            _ => return Err(metadata_error(&triple.s, &format!("unknown proof predicate {}", predicate))),
        }
    }

    let mut steps = Vec::new();
    for subject in order {
        let conclusion = formula_triple(subject).expect("validated step subject");
        let mut step = Step {
            conclusion: conclusion.clone(), kind: Kind::Unproven, rule: None,
            carried: None, builtin: None, bindings: Vec::new(), uses: Vec::new(),
        };
        let mut justifications = 0;
        let mut names = BTreeSet::new();
        for triple in &grouped[subject] {
            let Term::Iri(predicate) = &triple.p else { unreachable!() };
            let kind = &predicate[PE.len()..];
            if matches!(kind, "rule" | "fact" | "builtin" | "unproven") {
                justifications += 1;
            }
            match kind {
                "rule" => {
                    step.kind = Kind::Rule;
                    step.rule = literal_number(&triple.o);
                    step.carried = carried_rule(&triple.o);
                    if step.rule.is_none() && step.carried.is_none() {
                        return Err(metadata_error(subject, "pe:rule requires a positive integer or a graph containing one implication"));
                    }
                }
                "fact" => {
                    step.kind = Kind::Fact;
                    if plain_string(&triple.o).is_none() {
                        return Err(metadata_error(subject, "pe:fact requires a string source label"));
                    }
                }
                "builtin" => {
                    step.kind = Kind::Builtin;
                    if !matches!(triple.o, Term::Iri(_)) {
                        return Err(metadata_error(subject, "pe:builtin requires a predicate IRI"));
                    }
                    step.builtin = Some(triple.o.clone());
                }
                "unproven" => {
                    step.kind = Kind::Unproven;
                    if plain_string(&triple.o).is_none() {
                        return Err(metadata_error(subject, "pe:unproven requires a string reason"));
                    }
                }
                "uses" => {
                    let used = formula_triple(&triple.o).ok_or_else(||
                        metadata_error(subject, "pe:uses must wrap exactly one premise triple"))?;
                    step.uses.push(used.clone());
                }
                "binding" => {
                    let binding = bindings.get_mut(&triple.o).ok_or_else(||
                        metadata_error(subject, "pe:binding must refer to a node with pe:var and pe:value"))?;
                    let (Some(name), Some(value)) = (binding.name, binding.value) else {
                        return Err(metadata_error(subject, "each binding requires exactly one pe:var and one pe:value"));
                    };
                    if !names.insert(name) {
                        return Err(metadata_error(subject, &format!("duplicate binding for variable {}", name)));
                    }
                    binding.used = true;
                    step.bindings.push((name.to_string(), value.clone()));
                }
                _ => unreachable!(),
            }
        }
        if justifications != 1 {
            return Err(metadata_error(subject, "a step must have exactly one justification (pe:rule, pe:fact, pe:builtin, or pe:unproven)"));
        }
        if step.kind != Kind::Rule && !step.bindings.is_empty() {
            return Err(metadata_error(subject, "pe:binding is only valid on a rule step"));
        }
        steps.push(step);
    }
    for (node, binding) in bindings {
        if !binding.used {
            return Err(metadata_error(node, "binding metadata is not referenced by any step"));
        }
    }
    Ok(steps)
}

fn metadata_error(subject: &Term, detail: &str) -> EyeronError {
    EyeronError::new(format!("invalid proof metadata for {}: {}",
        crate::printing::term_to_n3_object(subject, &BTreeMap::new()), detail))
}

fn plain_string(term: &Term) -> Option<&str> {
    match term {
        Term::Literal(literal) if literal.language.is_none()
            && literal.datatype.as_deref().is_none_or(|dt| dt == "http://www.w3.org/2001/XMLSchema#string") => Some(&literal.value),
        _ => None,
    }
}

/// A `pe:rule { {premises} => {conclusion} }` object: the generated rule
/// the step carries, plus the statement asserting it, which a checker holds
/// the step to.
fn carried_rule(term: &Term) -> Option<(crate::ast::Graph, crate::ast::Graph, Triple)> {
    let statement = formula_triple(term)?;
    // A forward rule reads `{premises} => {conclusion}`, a backward one
    // `{conclusion} <= {premises}`.
    let forward = statement.p == Term::Iri(crate::ast::LOG_IMPLIES.to_string().into());
    let backward = statement.p == Term::Iri(crate::ast::LOG_IMPLIED_BY.to_string().into());
    match (&statement.s, &statement.o) {
        (Term::Formula(left), Term::Formula(right)) if forward => Some((left.clone(), right.clone(), statement.clone())),
        (Term::Formula(left), Term::Formula(right)) if backward => Some((right.clone(), left.clone(), statement.clone())),
        _ => None,
    }
}

fn literal_number(term: &Term) -> Option<usize> {
    match term {
        Term::Literal(literal) if literal.language.is_none()
            && literal.datatype.as_deref() == Some("http://www.w3.org/2001/XMLSchema#integer") => {
            literal.value.parse::<usize>().ok().filter(|number| *number > 0)
        },
        _ => None,
    }
}

impl ProofDocument for N3Proof {
    fn step_count(&self) -> usize {
        self.steps.len()
    }

    fn kind(&self, index: usize) -> Kind {
        self.steps[index].kind
    }

    fn describe(&self, index: usize) -> String {
        describe(&self.steps[index].conclusion)
    }

    fn dependencies(&self, index: usize) -> Vec<Resolution> {
        let step = &self.steps[index];
        let mut dependencies: Vec<_> = step.uses.iter().map(|used| match self.resolve(used) {
            Resolution::Step(target) if self.answers_query(&self.steps[target]) =>
                Resolution::Unresolved(format!("the query answer {}", describe(used))),
            resolution => resolution,
        }).collect();
        if let Some((_, _, statement)) = &step.carried {
            dependencies.push(self.resolve_rule(statement));
        }
        dependencies
    }

    fn claims(&self) -> Vec<String> {
        self.claims.iter().map(describe).collect()
    }

    fn claim_resolution(&self, index: usize) -> Resolution {
        self.resolve(&self.claims[index])
    }

    fn check_step(&self, index: usize) -> Verdict {
        let step = &self.steps[index];
        match step.kind {
            Kind::Rule => self.check_rule(step),
            Kind::Fact => self.check_fact(step),
            Kind::Builtin => self.check_builtin(step),
            Kind::Unproven => Err("is recorded as unproven: the engine could not justify it".to_string()),
            Kind::Absent | Kind::Collected => Ok(Checked::Trusted(step.kind.label())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "@prefix : <http://example.org/> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n:Socrates a :Human .\n:Human rdfs:subClassOf :Mortal .\n{ ?S a ?A . ?A rdfs:subClassOf ?B . } => { ?S a ?B . } .\n";

    fn proof_of(source: &str) -> String {
        let document = crate::parser::parse_n3_with_source(source, None, Some("test.n3")).unwrap();
        let result = crate::reasoner::reason(&document, &crate::reasoner::ReasonerOptions { proof: true, ..Default::default() });
        crate::proof_writer::proof_to_n3(&document.prefixes, &result)
    }

    #[test]
    fn a_generated_proof_checks() {
        let report = check_proof(SOURCE, &proof_of(SOURCE), "test.n3").unwrap();
        assert!(report.valid(), "{:?}", report.failures);
        assert!(report.verified >= 3);
    }

    #[test]
    fn a_tampered_conclusion_is_rejected() {
        let proof = proof_of(SOURCE).replace(":Socrates a :Mortal", ":Plato a :Mortal");
        let report = check_proof(SOURCE, &proof, "test.n3").unwrap();
        assert!(!report.valid());
    }
}

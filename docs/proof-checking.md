# <img src="https://josd.github.io/images/eye.png" alt="" width="32" height="32"> Proof-checking contract

This is the validity contract implemented by [proof_check.rs](../src/proof_check.rs) and its [N3 reader](../src/proof_check_n3.rs). The numbered sections are also referenced by code comments. [The guide](guide.md#proofs-and-their-limits) covers commands and a decision/audit example.

## 1. Purpose

A proof records derived claims and the steps supporting them. Checking asks whether those recorded steps establish their claims from the supplied source program. The requirements below use MUST and SHOULD for the contract; section 10 describes implementation coverage.

## 2. Checking and its scope

The checker MUST check the recorded inferences rather than search for replacement derivations. It assumes source assertions. It establishes neither their real-world truth nor completeness of the reasoner's output. Trusted justifications remain explicit obligations.

`--check-proof` reads a saved proof. `-c` first generates a proof, then invokes the same checker on that document. Generation and checking are separate stages even within that command.

## 3. Model

A step has a conclusion triple, one justification, bindings, and an ordered list of premises (`uses`). A proof also has claims: what the run outputs, which for a program with `log:query` rules is their answers alone. The source supplies facts and rules, numbered from 1 in source order after excluding query rules. Written rules are also available as statements. Universally quantified source assertions can justify their instances. The proof's evidence is the source facts together with every step conclusion.

## 4. Validity conditions

| Condition | Requirement |
| --- | --- |
| **C1: resolution** | Each rule step MUST re-perform one application of the cited source rule, or of a justified carried rule, under consistent bindings (5.1). Each fact step MUST state a source assertion or a permitted instance of one (5.2). |
| **C2: well-foundedness** | The resolved step dependencies, including carried-rule provenance, MUST be acyclic. |
| **C3: justification** | Each step MUST have exactly one supported justification of a well-formed kind; `unproven`, a fact step with premises, and a built-in named for another predicate fail. No statement may be claimed twice. |
| **C4: coverage** | Every premise, carried rule, and claim MUST resolve to a step conclusion or a source assertion. |
| **C5: re-decision** | Each built-in step MUST hold when re-evaluated on its recorded arguments (5.3), unless it is a trust obligation (6). |
| **C6: boundary consistency** | The proof's evidence MUST NOT contradict what its obligations assume (6), and MUST NOT satisfy the premise of a source inference fuse. |
| **C7: relevance** | Every step MUST serve a claim: some claim depends on it. With `log:query` rules in the source, every claim MUST be an instance of a query's conclusion. |

C1 and C5 re-perform what the proof records; C2 and C4 check its structure; C3 its form; C6 looks inside the proof for evidence against its assumptions; C7 keeps it to what the source asked for. Malformed proof metadata (section 8) is a reader error rather than a C3 failure, because no report can attribute it to a step.

The report records failures with their condition, affected conclusion, and detail, and lists every condition with how many items it examined and how many failures it found. `checked` means no failures or obligations; `checked with obligations` means no failures but some justifications are trusted. `invalid` means at least one condition failed. Empty proofs can check with zero steps; this does not demonstrate that a source program derived anything.

## 5. Verified justifications

### 5.1 Rules

For `pe:rule N`, the checker MUST take rule N from the supplied program, not an unverified copy in the proof. It checks that recorded bindings, premises, and conclusion form one application of that rule:

1. Map source variable names to the rule's internal names and initialize a fresh binding environment.
2. Require the number of `uses` entries to match the rule's premises.
3. Match corresponding premises in order, sharing that environment.
4. Require at least one rule-head triple to match the claimed conclusion under the same environment. For unquoting conclusions, resolve the graph supplied by the bindings first.

Bindings may be partial; matching extends them consistently. Matching is one-way: only the rule's variables bind, and variables in the recorded step stay rigid. Local graph binders retain their scopes. A blank node scoped to the rule head is an existential, and its witness MUST be a blank node that neither the source facts nor the step's bound values mention. Firings that agree on the head's variables share their witnesses; a witness MUST NOT be introduced by two firings that differ. A blank quantified outside the rule is one individual for every firing.

Generated rules and query rules can be carried as `pe:rule { { ... } => { ... } }`, or with `<=`. Their rule statement MUST be an instance of an available source statement or a proof-step conclusion, up to the reader's renaming rules, before the application is checked. A step that carries a query rule records a query answer: it may be claimed, but no step may use it as a premise. When a proof step supplies that rule, it adds a dependency subject to C1/C2; a rule cannot justify its own derivation through a cycle. The reader recognizes reversed `log:implies`/`log:impliedBy` representations of the same rule.

### 5.2 Facts

For `pe:fact`, the checker MUST find the conclusion among the source assertions, their permitted instances, or equivalent rule statements. A fact step MUST have no premises. Its file label is descriptive; source membership is what the checker verifies, not authentication of a document or authority.

### 5.3 Built-ins

For `pe:builtin`, the named relation MUST agree with the conclusion's predicate (C3) and hold when re-evaluated on its arguments (C5), which MUST NOT contain variables outside graph terms; a collection's template and condition may be open, its result may not. Scoped built-ins whose scope is a quoted graph — `log:includes`, `log:notIncludes`, `log:collectAllIn`, `log:forAllIn` — and `log:conjunction` are functions of their arguments and are re-decided like any other. Built-ins that read outside the proof, and scoped tests over the current graph, are obligations instead (section 6).

## 6. Trust obligations

An obligation is a step the checker accepts without re-establishing it. Each is listed with its kind and conclusion:

- `impure built-in`: `time:localTime`, `time:currentTime`, `time:gmTime`, `math:random`, `log:content`, `log:semantics`, `log:semanticsOrError`, `list:map`, and `log:conclusion`, which reads the clock, the network, a random source, the fact set, or runs the reasoner itself. This is a checker classification: `math:random`, for example, is not generated by the current evaluator.
- `absent`: `log:notIncludes` over the current graph. Its scope is anything but a quoted graph, or the stand-in graph `{ <http://example.org/a> <http://example.org/b> <http://example.org/c> }` an unbound subject is bound to after the current graph has been tested.
- `collected`: `log:collectAllIn` and `log:forAllIn` over the current graph.
- `current graph`: `log:includes` over the current graph.

Whether a test over the current graph held depends on everything the run derived, which a proof does not record, so its truth is assumed. Its consistency is not (C6): the checker re-evaluates the test over the proof's evidence alone and fails when that evidence contradicts it. A negation fails when the evidence includes what it says is absent. A collection fails when the evidence supplies a member its recorded list omits. A universal test fails when the evidence has a counterexample whose conclusion consists of built-ins, since only those can be refuted without inference. The evidence is only what the proof records, so C6 can find a contradiction but cannot establish that a negation or collection was complete.

Acceptance records an assumption; it does not establish that the trusted relation holds. Consumers can reject reports with nonempty `obligations` even when `Report::valid()` is true. The CLI returns success for both successful verdicts.

## 7. Unproven steps

`pe:unproven` means the proof writer could not supply support. The checker MUST report it as a C3 failure. It is not a trusted assumption.

## 8. N3 syntax and reading

The proof vocabulary is `https://eyereasoner.github.io/pe#`.

| Meaning | Syntax |
| --- | --- |
| Claim | A top-level statement with a non-`pe:` predicate, including formula subjects and rule assertions |
| Step conclusion | `{ s p o }` as the subject of its `pe:` statements |
| Numbered rule | `pe:rule N` |
| Carried rule | `pe:rule { { premises } => { conclusions } }` (or `<=`) |
| Source assertion | `pe:fact "label"` |
| Built-in relation | `pe:builtin predicate` |
| Missing support | `pe:unproven "reason"` |
| Binding | `pe:binding [ pe:var "name"; pe:value value ]` |
| Premise | `pe:uses { s p o }`, repeated for multiple premises |

The reader groups step metadata by its quoted subject, which must contain one triple. Uses also wrap one triple. It resolves recorded conclusions directly, allowing consistent local renaming in closed graph terms. Source facts containing free universals and written rule statements are matched as instances.

The `pe:` namespace is reserved for proof metadata. Every step must have exactly one recognized justification, and each premise must wrap exactly one triple. Rule numbers must be positive `xsd:integer` values; fact labels and unproven reasons must be strings; built-in names must be IRIs. Bindings require one nonempty string name and one value, may occur only on rule steps, and cannot repeat a variable name. Unknown predicates, malformed metadata, and unreferenced binding records are reader errors.

Both CLI checking modes use the same parsed source `Document`, preserving input formats, base IRIs, and file scopes; see [input handling](guide.md#proofs-and-their-limits).

## 9. Reporting

Reports contain step totals, verified-justification counts, counts by kind, obligations, and failures. An invalid proof can have verified justifications while failing premise resolution, cycle checking, or claim coverage. Failure counts are counts of diagnostics, not necessarily distinct steps.

```console
$ eyeron --check-proof examples/proof/socrates.n3 examples/socrates.n3
Proof: examples/proof/socrates.n3
Source program: examples/socrates.n3
checked: 3 steps, 3 verified, 0 trusted
Step kinds:
  source facts: 2
  rule applications: 1
Conditions:
  C1 resolution: 3 checked, 0 failed
  C2 well-foundedness: 3 checked, 0 failed
  C3 justification: 3 checked, 0 failed
  C4 coverage: 3 checked, 0 failed
  C5 re-decision: 0 checked, 0 failed
  C6 boundary consistency: 0 checked, 0 failed
  C7 relevance: 4 checked, 0 failed
```

Trusted conclusions are listed under “Trust obligations (accepted without verifying their truth)”; failures name their condition, such as `[C6: boundary consistency]`. Invalid proofs return status 1. Parsing errors occur before a checking report exists.

## 10. Implementation coverage

[tests/proof_validation.rs](../tests/proof_validation.rs) covers malformed metadata, mislabeled built-ins, unsupported formula/rule claims, cycles through carried-rule provenance, over-general carried rules, reused or non-fresh existential witnesses, open built-in arguments, query answers used as premises, re-decided quoted-graph tests, negations and collections contradicted by the proof's evidence, tripped inference fuses, and steps or claims that serve nothing the source asked for. [CLI tests](../tests/cli_proof_check.rs) cover saved and generated proofs, source bases, separate input scopes, RDF formats, and stdin. Malformed proof metadata returns a reader error before a report is constructed; semantic C1–C7 failures produce an invalid report.

[tests/proof_checking.rs](../tests/proof_checking.rs) requires every packaged proof to check successfully, except `variable-predicate-dependency`: an intentional negative cycle that assumes `:a a :Blocked` is absent and then derives it, which C6 rejects. [Generation regressions](../tests/proof_generation.rs) also check fresh proofs, including quoted closures with overlapping variable names, backward premise ordering, and backtracking past incomplete or circular explanations.

# A decision with reasons you can check

How Eyeron makes a decision, records its reasons, and lets another program inspect what it depends on.

This deck follows two examples: [the decision](../verifiable-decision.n3) and [the audit](../verifiable-decision-audit.n3).

---

## The story: a request for research data

Dr Okafor asks to use a dataset called **cohort B** for secondary research.

The example supplies three supporting assertions:

- an ethics approval covers the requested purpose;
- the cohort's consent covers that purpose;
- a registry records the dataset as de-identified.

These are inputs to a small demonstration policy, not a complete assessment of research law or privacy.

---

## What it takes to say yes

The policy requires all three conditions:

```text
Ethics approval covers this research ─┐
Consent covers this research ─────────┼──→ Permit the request
Dataset is recorded as de-identified ─┘
```

Eyeron applies written rules to the supplied facts.

For this request, it derives:

```n3
:request-42 :decision :permit .
```

Read that as: “Under these rules and assertions, request 42 is permitted.”

---

## A yes with a trail of reasons

An answer alone leaves an important question open:

> How did you reach it?

Eyeron can write a **proof**: a record of the facts and rule applications supporting its conclusions.

For example, the consent check uses three facts:

```text
Request 42 asks for cohort B.
Request 42 is for secondary research.
Cohort B's consent covers secondary research.
                   ↓
The request meets the consent condition.
```

The final permit depends on this check and the other two conditions.

---

## Another check can inspect the reasoning

From the repository root, generate a proof and check it against the source program:

```bash
eyeron -p examples/verifiable-decision.n3 > /tmp/decision.why.n3

eyeron --check-proof /tmp/decision.why.n3 \
  examples/verifiable-decision.n3
```

The report includes:

```text
checked: 11 steps, 11 verified, 0 trusted
```

Those are seven source facts and four rule applications.

The checker checks the recorded steps, their support, and their dependencies. It does not search for a replacement explanation.

“0 trusted” means no step justifications were accepted without verification. The source assertions are still assumed.

---

## A correct chain still starts somewhere

The proof can show that the permit follows from a consent assertion.

It cannot establish, by itself, that the consent record is accurate or authentic.

That leads to a second question:

> Whose assertions does this decision depend on, and do we accept those authorities?

This is what the audit example explores.

---

## Four sources behind the decision

The audit supplies an explicit mapping between assertions and documents:

| Document | What it vouches for in the example |
| --- | --- |
| Intake record | Who made the request, which dataset, and which purpose |
| Ethics record | The approval and the purpose it covers |
| Consent record | The cohort's consent coverage |
| Registry record | The dataset's de-identification status |

It also lists which documents its trust policy accepts.

These mappings are supplied as data. They are not digital signatures.

---

## The proof becomes input to the audit

Eyeron's proof is written in N3, the same language the audit rules can read.

The proof records links meaning “this conclusion uses that premise.”

The audit follows those links:

```text
Permit
  → supporting checks
    → source assertions
      → documents that vouch for them
```

It then compares the identified documents with its trust policy.

This lets a second program ask questions about the grounds for a decision.

---

## Run the audit on the proof you generated

```bash
eyeron examples/verifiable-decision-audit.n3 \
  /tmp/decision.why.n3
```

The result identifies four supporting documents:

```n3
:audit :standsOn doc:intake .
:audit :standsOn doc:consent .
:audit :standsOn doc:registry .
:audit :standsOn doc:ethics .
```

Here, `standsOn` means “the decision depends on assertions attributed to this document.”

The bundled [audit input](../input/verifiable-decision-audit.n3) is a saved copy of the [decision proof](../proof/verifiable-decision.n3), so the example can also run without generating a new one.

---

## What if we stop accepting one source?

In a copy of the audit program, remove this line from the trust policy:

```n3
:policy :trusts doc:registry .
```

The decision still depends on the registry's assertion.

When the audit finds that the registry is outside the accepted list, an **inference fuse** fires: Eyeron stops and reports the violated rule and its matching evidence, with exit status **65**.

This does not change the original permit's derivation. It shows that the audit's acceptance policy rejects one of its supporting sources.

A surrounding application must decide what to do with that result.

---

## What this small demonstration leaves open

The current audit catches an identified authority missing from its trust policy. It does not reject a source assertion whose attribution is missing entirely. A complete acceptance check needs that additional coverage check.

It also does not authenticate issuers, check whether evidence has expired, or establish legal compliance.

The audit itself can produce a proof, but some operations in Eyeron's checker remain explicit **trust obligations**: assumptions whose truth the checker does not verify. Always read the report.

See the [proof-checking contract](../../docs/proof-checking.md) for the precise boundary.

---

## Why this matters when a decision travels

Imagine another organisation receives the permit.

Given the proof and the relevant source program, it can check whether the recorded reasoning supports that decision. Given authority mappings and its own trust policy, it can also inspect whose assertions are involved.

It still needs to assess those sources and whether the rules apply to its intended use.

The two examples make three questions visible:

1. **Decision:** does the request meet the supplied rules?
2. **Proof:** do the recorded steps support the answer?
3. **Audit:** which sources does the answer rely on, and are they acceptable to us?

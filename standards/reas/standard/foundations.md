---
type: Engineering Standard
title: Foundations
description: What REAS is for, why it raises the floor for early-stage work, and the principles every other rule rests on.
tags: [foundations, principles, technical-debt, mvp, instrument-gauge]
updated: 2026-11-22
---

# Foundations

REAS is an instrument gauge, not the black box.
It establishes a high engineering baseline, runs bounded checks, surfaces meaningful departures in terms a person can act on, and keeps the evidence behind each one.
The maintainer decides what to investigate.

AI-assisted development changed the economics of writing software, and changed them for the better.
The practices this standard asks for used to cost days of work that an early-stage project could not spare: validated domain types, typed errors, tests that fail when behavior breaks, a production database engine from the first commit, documentation written in the same change, one visible composition root.
They now cost minutes to produce.
The bargain that justified most early shortcuts, correctness traded for time, no longer holds for this class of work.
This standard therefore grants no exception for a first version.

What became cheaper is producing code and exploring alternatives.
What did not become cheaper is reading code, verifying that it is correct, and migrating data and contracts once they exist.
A person maintaining AI-assisted software will not read most of what the agents produce, and the standard is designed for that reality.
So the standard raises its floor for early-stage work and holds its ceiling where it was, and it asks every check and every report to spend the reader's attention on what matters.

The principles below form one philosophy, in this order:

1. Raise the floor and hold the ceiling ([REAS-FOUND-001](#reas-found-001)): do not defer engineering that is now cheap, and do not add structure because it is cheap to generate.
2. Avoid foreseeable rework ([REAS-FOUND-002](#reas-found-002)): decide early what is costly to reverse or likely to be copied; keep the rest reversible.
3. Make the first instance the template ([REAS-FOUND-003](#reas-found-003)): agents and people copy what the repository already contains.
4. Apply by property, scale by consequence ([REAS-FOUND-004](#reas-found-004)): a system's properties decide which checks apply; the consequence of a failure, judged where it lives, decides how much effort each check gets.
5. A check is not an investigation ([REAS-FOUND-005](#reas-found-005)): a finding says attention may be needed and carries enough context to start from; the maintainer decides whether to go further.

Three stances that other concepts state in full support them: evidence, in [REAS-TEST-002](validation.md#reas-test-002), a check is passed only when a cited run on a named revision shows it; seams, in [Security at seams](security.md), the defects that survive type checking live where two correct pieces disagree; and floor, not ceiling, in [Native code](native-code.md), the mechanical practices clear known failure classes so that judgment is free for what no tool watches.

<a id="reas-found-001"></a>
## REAS-FOUND-001: Raise the floor, hold the ceiling

### Intent

The cost of sound engineering fell; the cost of owning unsound engineering did not.
A standard that still prices correctness in days of typing would license shortcuts that no longer save anything and would miss the one cost that grew: the reader's.

### Enforcement

Advisory.

### Scope

Applies to every decision about how much engineering a change deserves, in every project that adopts this standard, at every stage including the first version.
Does not apply to product scope, which the project's owner decides.

### Rules

Write responsibly from the first commit.
A minimum viable product limits product scope; it is not a license for engineering deficiencies in what is built.
Reduce scope by building fewer features, not by building the chosen features badly.

Treat the cost of producing code as near zero and the cost of reading, verifying, and migrating it as unchanged.
Do not defer a known engineering responsibility because it can be completed later.
Do not add a type, a trait, a layer, or a test because it is cheap to generate.
Add it because a recorded requirement or an existing responsibility needs it, as [REAS-ARCH-004](ports-and-adapters.md#reas-arch-004) and [REAS-RUST-003](core-rust.md#reas-rust-003) specify.

The floor is the set of practices whose cost fell: the rules of this standard.
The ceiling is judgment: which boundaries the domain actually has, which requirement will survive contact with users, which seam the tools do not watch.
The floor exists to free attention for the ceiling, not to replace it.

### Verification

- Review question: which shortcut in this change saves production time, and what does it cost the next reader?
- Review question: which structure in this change exists because it was cheap to generate rather than because a recorded requirement needs it?

<a id="reas-found-002"></a>
## REAS-FOUND-002: Avoid foreseeable rework

### Intent

Rework that the author could see coming is engineering effort spent twice and reviewed twice.
Its cost is not fixed at the moment of the shortcut: it grows with every feature written in the shortcut's image and with every agent that reads the shortcut as the intended design.

### Enforcement

Required.

### Scope

Applies to every design decision whose alternative is a known, maintainable implementation that is practical now.
Does not apply to a decision that rests on a requirement nobody has recorded; that decision stays reversible and is deferred.

### Rules

Do not knowingly choose a temporary or structurally deficient implementation to save initial effort when a maintainable implementation is practical and the requirement it serves is recorded.
A requirement is recorded when it appears in a decision document, in the answers to [REAS-ARCH-003](ports-and-adapters.md#reas-arch-003), or in the project's stated service life.
A requirement inferred from the code or from a guess about the future is not recorded, and building for it is speculative.

Weigh each decision by three costs over the recorded service life:

1. Its expected cost if the project reaches that life.
2. Its reversal cost: a decision that touches persisted data, a wire format, an identifier handed to another party, or the boundary between domain and I/O is costly to reverse.
3. Its propagation: a decision that later code will copy has a reversal cost that grows with every copy. A convention is a decision of this kind.

Make the decisions with high reversal or propagation cost deliberately and early.
Keep the others reversible and defer them without guilt.
Reversibility bounds the damage of a wrong guess; it is not a reason to choose a known-worse design.

Develop against the production database engine from the first commit when the engine is a recorded requirement, as [REAS-DATA-001](persistence.md#reas-data-001) specifies.
Keep domain behavior separate from I/O from the first commit, even in a flat module layout, because that separation is what keeps the later move to ports cheap.
Model domain values as validated types from the first commit, because a value passed as a bare string or integer is copied at every call site and becomes the most expensive refactoring in the project, as [REAS-RUST-001](core-rust.md#reas-rust-001) specifies.

### Allowances

A spike written to answer one question and deleted afterward is not a first version; record that it will be deleted, and delete it.

### Verification

- Review question: what foreseeable rework does this shortcut create, and what will it cost once the code that copies it exists?
- Review question: which recorded requirement does this structure serve? If none, why is it not speculative?
- Review question: which decision here is costly to reverse, and was it made deliberately?

<a id="reas-found-003"></a>
## REAS-FOUND-003: Make the first instance the template

### Intent

An agent reads the repository as the specification of intent and reproduces the patterns it finds.
A person does the same with less consistency.
The first implementation of any pattern is therefore not an instance; it is the template every later instance copies.

### Enforcement

Required.

### Scope

Applies to the first occurrence of any pattern in a repository: the first handler, the first repository adapter, the first domain type, the first test, the first error enum.
Does not apply to a later occurrence that follows an established pattern; that occurrence is reviewed against the template.

### Rules

Make the first instance small and correct before you make it complete.
One pure domain function with validated inputs, one adapter that owns its SQL, one test that fails when the behavior it names is removed: these cost little and set the pattern for everything after them.
Do not let the first instance carry the shortcut you plan to remove later; it will be copied before you remove it.
When you find that a copied pattern is wrong, fix the template and the copies in one change, as [REAS-DOC-006](documentation.md#reas-doc-006) specifies for defect classes, so that the next copy is of the fixed version.
Examples in documentation are instructions in the same way, as [REAS-DOC-003](documentation.md#reas-doc-003) specifies.

### Verification

- Review question: if this is the first instance of its pattern in the repository, is it the one you want copied?
- Review question: which existing instance did this change copy, and does that instance still represent the intended design?

<a id="reas-found-004"></a>
## REAS-FOUND-004: Apply by property, scale by consequence

### Intent

Whether a verification practice applies is a fact about the system.
How much of it to run is a judgment about what a failure would do.
Keeping the two decisions separate prevents the first from being bargained down by the second, and prevents the second from becoming a project label that excuses whole practices.

### Enforcement

Required.

### Scope

Applies to every project that adopts this standard, at every stage including the first version, and to every rule whose Verification names a budget, a schedule, or a hardware matrix.
Does not apply to product scope, which the project's owner decides.

### Rules

Whether a practice applies is a property of the system: who supplies its input, whether it contains native code, what data it holds, what it exposes, what hardware it depends on.
A property that holds makes the practice apply, and no judgment about consequence removes it.

How much of a practice to run is a judgment about consequence, made where the consequence lives: at the component, boundary, or operation, not for the project as a whole.
A component is consequential when one defect in it can expose sensitive data, misuse a credential that can change or spend, lose money, or cause physical, legal, or safety harm, through a failure path the maintainer can name.
Record that path in the component's decision document in one sentence: the defect, the exposure it travels through, and the party it reaches.

A consequential component receives more verification effort than the baseline, and the amount is a recorded decision with its reason, not a schedule.
Where no consequence is recorded for a component that handles outside data, credentials, or money, treat it as consequential until a maintainer records otherwise.
An agent proposes the failure path; a maintainer records the judgment.

### Verification

- Review question: which components in this change are consequential, and what is the one-sentence failure path for each?
- Review question: did any budget in this change fall below what the component's recorded consequence requires, and if so, where is the decision?

### Limits

The recorded path establishes that a maintainer considered the component and named a route from defect to effect.
It does not establish that the route is the worst one, and no check verifies the judgment itself.

<a id="reas-found-005"></a>
## REAS-FOUND-005: A check is not an investigation

### Intent

A person maintaining AI-assisted software cannot read everything the agents produce.
The standard must therefore tell that person, in a few sentences, whether attention is needed and where to begin, and must not convert every reading into an open-ended inquiry.

### Enforcement

Required.

### Scope

Applies to every check this standard defines, to every report of a check's result, and to every agent working under this standard, in this repository or in a project that adopts it.
Does not apply to the bounded diagnostics an agent needs to finish authorized work, such as reproducing a failing test or reading the code the test exercises.

### Rules

A check evaluates a defined property against an established expectation and reports one of the six outcomes of [REAS-TEST-002](validation.md#reas-test-002).
A finding is a check that failed, or a consequential property that a check could not establish.
An investigation establishes causes and remedies for an uncertain problem.
REAS defines checks.
A finding says that an investigation may be warranted; the maintainer decides whether to start one.
A check never widens into an investigation on its own, and no rule in this standard prescribes one.
A failed mandatory check stays a failure, and no finding is concealed or reported as a pass.

Report in proportion.
A routine pass needs its outcome and its evidence reference.
A finding needs, in a few sentences: what changed or failed, why it matters, how sure the evidence is, whether it needs intervention, and what remains uncertain.
A possible need for deeper work is a recommendation, not work the agent starts.
Keep logs, test output, and analysis where a reader can reach them, and out of the primary report.

Give a finding a starting point.
At each engineering boundary, preserve the context that is cheap to keep and expensive to reconstruct: the affected component or operation, the expected behavior and the observed deviation, the error chain, the revision and environment, an identifier that connects the failure to the operation that caused it, and the check or event that exposed it.
State which of these are established facts and which are suspected causes.
Do not preserve sensitive data, credentials, or unbounded logs to do it; redact or omit a sensitive value and keep the correlation identifier, because encoding a value for a log makes it safe for the log's syntax, not safe to disclose.
The existing rules on errors, state transitions, and input handling say where this context comes from: [REAS-RUST-005](core-rust.md#reas-rust-005), [REAS-STATE-003](state-machines.md#reas-state-003), and [REAS-SEC-002](security.md#reas-sec-002).

### Verification

- Review question: does this report give the reader the finding in a few sentences and the evidence on request, or the reverse?
- Review question: from this finding alone, can a person name the component and the boundary to look at first?

## Quality test

Does this change defer engineering that is now cheap, add structure that only generation made cheap, establish a pattern you would not want copied, bargain an applicable check down by calling the project small, or bury a finding in the evidence behind it?

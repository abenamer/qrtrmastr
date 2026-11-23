---
type: Engineering Standard
title: Validation and evidence
description: Tests, test doubles, failing-test triage, fuzzing, and evidence for reported results.
tags: [testing, fuzzing, evidence]
updated: 2026-11-20
---

# Validation and evidence

<a id="reas-test-001"></a>
## REAS-TEST-001: Test invariants and failures

### Intent

A test earns its place by failing exactly when the contract is violated and passing otherwise.
A test that a do-nothing change would pass, or that asserts only that no panic occurred, proves nothing about the delivered behavior.

### Enforcement

Required.

### Scope

Applies to every automated test in every crate.
Does not apply to a benchmark or a profiling harness.

### Rules

Write unit tests for domain invariants and each state transition.
Do tests for error paths and invalid inputs.
Use a failing test double for error propagation tests through a port.
Use property tests when generated inputs can exercise an invariant.
Use simulations for behavior that depends on sequences of events.
Record the seed when a simulation fails.
Use that seed to reproduce the failure.

For a database adapter, use a container with the production database engine for integration tests.
Do not use an in-memory adapter as evidence for database behavior.
Add a compile-fail test for an attempted downstream implementation of each sealed state trait.

Make each test non-vacuous:

- Ask whether a do-nothing change would already pass the test. If yes, reword the assertion or pair the test with a positive control.
- Pair each assertion that something is absent or rejected with a positive control on the same fixture that shows the probe sees the thing when it is present.
- Call the production entry point, not only a helper that the production path happens to use.
- For a fix made of several independent changes, revert each part in turn and confirm that the test goes red each time.
- A reproduction test for a bug must fail on the unfixed code with the bug's symptom. A test that can run only after the fix exists, because it imports the fix's new symbol, captures the fix's shape, not the bug.
- Do not write a change detector: a test that restates configuration and fails only when someone changes that configuration on purpose. A recorded real run is the evidence for configuration.

Design tests so that a mutation of the code under test fails them:

- For each `>=`, `<=`, `<`, or `>` guard, test the value below the threshold, at the threshold, and above it.
- Use inputs where an operator swap changes the result: `3` not `1` for multiplication, a nonzero value for addition.
- For each `&&` or `||`, include a case where exactly one operand is true.
- Assert the typed error variant and the diagnostic facts its message must carry, such as the identifiers and sizes a person needs. Pin exact wording only when the wording is itself a requirement, for example a message shown to a user or written into a contract.
- Assert an observable outcome: a return value, a state change, or an effect. A test with no assertion cannot fail.

Do not write a test for a constructor or an accessor alone; a behavior test covers construction.
Validation rules are behavior and get explicit tests.
A reviewer must be able to say what property a test checks, why its oracle is independent of the implementation, which mutations would make it fail, and what it cannot establish.
Prefer one property test with an independent oracle to many example tests that share one; the measure is what a reviewer can verify, not line count.

Mutation testing challenges the tests.
It is Required for decision code in a consequential component under [REAS-FOUND-004](foundations.md#reas-found-004), and Advisory elsewhere.
Decision code is logic whose incorrect behavior could violate a domain invariant, a state transition, a security property, a persistence guarantee, or an external contract.
Before review of a change to such code, run the diff-scoped pass:

```bash
git diff --merge-base origin/main > pr.diff
cargo mutants --in-diff pr.diff
```

Dispose of each line in `mutants.out/missed.txt` as one of three outcomes, in the pull request:

- Killed: a new or strengthened test fails on the mutant.
- Equivalent: the mutant does not change observable behavior; say why.
- Accepted survivor, non-contractual configuration: the mutant changes behavior, but the exact value is not required by an external contract, a documented operational requirement, or a correctness or security invariant; the tests establish that the behavior is correct for the configured value; and the survivor and its reason stay visible. This outcome is never reported as killed or equivalent.

Report survivors and their dispositions, not a score.
A survivor that reveals an untested contract is the finding; a survivor on a configurable value is one line.
Do not add a test that pins incidental wording or a configurable value only to kill a mutant; test the diagnostic facts, the error behavior, or the source chain, which are requirements.
Exclude a test double from the pass with `exclude_re` in `.cargo/mutants.toml` when its own tests show that it can both accept and reject.
Mutation testing does not substitute for Miri, sanitizers, or fuzzing on the paths those cover.

### Allowances

A test that pins an interface for a consumer outside this repository may restate that interface; name the consumer in the test.

### Verification

- Review question: for each new test, what change to the code under test would make it fail?
- Review question: which assertion in this test is a positive control for its negative assertions?
- Tool: the diff-scoped `cargo mutants` pass on decision code in a consequential component, each survivor disposed as one of the three outcomes.

### Limits

A killed mutant shows that one test distinguishes the code from one specific wrong version of it, and a survivor shows that no test does.
The pass says nothing about defects no mutation operator produces, about memory safety, or about lines outside the diff.

<a id="reas-test-002"></a>
## REAS-TEST-002: Report observable evidence

### Intent

A report that reads greener than the run misleads every reader after it.
Skipped is not passed, a retry is not a clean pass, and a check that evaluated nothing is not green.

### Enforcement

Required.

### Scope

Applies to every report of a build, test, scan, migration check, language review, or recovery drill, including the [self-audit](self-audit.md).
Does not apply to a local run that nobody cites.

### Rules

Report a check as passed only when execution evidence shows that result.
Report these outcomes separately: passed, failed, errored, timed out, skipped, and not run.
For a check with no pass criterion of its own, such as a mutation pass, report the tool's result and the maintainer's disposition of it as two separate statements; do not fold the disposition into a pass.
Errored means the check could not start or crashed before it produced a verdict.
Skipped is never reported as passed.
Not run is never reported as passed, and a not-applicable check names the reason.
For a check that passed after a retry, report the number of attempts and each attempt's outcome; do not report it as a plain pass.
State the size of the population a percentage refers to; a percentage of the tests that ran hides the ones that did not.
A check that evaluated an empty population is not green; report it as not run.
For each passed check, record the date and commit or immutable workflow run.
For evidence from a different environment, identify the source and the commands that ran.
Do not apply a passing result to code that changed after that execution.
Before you cite a run, confirm that it tested the revision you are describing.

### Verification

- Review question: for each cited run, does the commit it tested contain the code the report describes?
- Record: the self-audit lists each check with one of the six outcomes and its evidence.

<a id="reas-test-003"></a>
## REAS-TEST-003: Fuzz each adapter that reads outside bytes

### Intent

A parser's test suite covers the inputs its author imagined.
A fuzzer finds the inputs the author did not, and a target with property assertions finds wrong results, not only crashes.

### Enforcement

Required.

### Scope

Applies to every inbound adapter and every parser that reads bytes from another party.
Does not apply to a private tool that reads no bytes from another party; it records an exception for the budgets and the architecture matrix in an architecture decision.

### Rules

Write one fuzz target for each inbound adapter and each parser that reads outside bytes.
Call the adapter's public entry point with the fuzzer's bytes.
Assert properties in the target.
A target that only checks for the absence of a panic finds fewer defects.
Assert these properties where they apply:

- The adapter returns a value or a typed error.
- Where the format has one canonical encoding, as the trust-boundary frame does, a parsed value encodes back to the input bytes. Where several encodings denote one value, encoding the parsed value and parsing it again yields an equal value, and encoding is idempotent.
- Each duplicate implementation returns the same result as its reference, as [REAS-SEC-005](security.md#reas-sec-005) specifies.
- The adapter rejects an input above the resource limit before it allocates.

Build each target with `cargo fuzz`.
Keep debug assertions and overflow checks enabled in the fuzz build.

Persist the corpus wherever this rule applies.
Commit a seed corpus with at least one valid input for each target.
Store the inputs and crash artifacts that runs discover in artifact storage or a corpus repository, and start the next run from them; without persistence, successive short runs repeat each other.
Minimize the corpus on a schedule.
Record each crash artifact as a bug and add a regression test.

Run each target on every pull request for 30 to 60 seconds from the persisted corpus.
That is the baseline check.
Report what ran: the executions, the corpus size, and whether the run reached the end of the corpus; a short run over a large corpus has not replayed it.
For a consequential component under [REAS-FOUND-004](foundations.md#reas-found-004), run a longer campaign, and record its duration, its hardware, and the reason in the component's decision document.
Run the targets on each CPU architecture and target-feature set where the code has SIMD or endianness-dependent paths, because those paths differ between them.

### Verification

- CI: the pull-request fuzz job, with its duration in the workflow file; a longer campaign where a component's decision document calls for one.
- Review question: which inbound adapter has no target, and why?

### Limits

A fuzz run establishes that no input it actually exercised violated the asserted properties.
A short run may not have replayed the whole persisted corpus, which is why the report states the executions and the corpus coverage.
It does not establish the absence of violating inputs, and it says nothing about properties the target does not assert or hardware it did not run on.

The [trust-boundary example](../examples/trust-boundary/) includes one target with a seed corpus.
Its nightly CI job builds the target and runs it for 30 seconds on x86_64.

<a id="reas-test-004"></a>
## REAS-TEST-004: Double only at ports

### Intent

A double of code this application owns couples the test to an implementation shape and can manufacture a state the real code never reaches.
A double at a port replaces a system the application does not own, which is what ports exist for.

### Enforcement

Required.

### Scope

Applies to every test that uses a test double: a mock, stub, fake, spy, or in-memory substitute.
Does not apply to a test that uses only real collaborators.

### Rules

Substitute a collaborator only where control leaves the application: at a port.
Behind a port sit the things a test may replace: another bounded context reached through its published contract, the filesystem, a database, the network to a system the application does not own, the clock, randomness, a subprocess the application does not own, and an outbound notification.
Everything the application owns runs for real in the test, including validation rules and domain logic.
A module boundary is not a port; a sibling module that shares the domain model runs for real.
The list permits a substitute; it never requires one. A real collaborator in a temporary location, such as a database in a container or a repository in a temporary directory, is always allowed.
Assert on observable outcomes, not on which methods the double received.
When a unit test needs several doubles, the code under test reaches infrastructure directly; extract the behavior as a function of domain inputs, as [REAS-ARCH-005](ports-and-adapters.md#reas-arch-005) specifies, and keep one thin test at the real boundary.

### Verification

- Review question: for each double in this test, which port does it implement, and what system behind that port does it replace?
- Review question: does any assertion count calls on a double instead of checking an outcome?

<a id="reas-test-005"></a>
## REAS-TEST-005: Triage a failing test before you touch it

### Intent

A red test is a finding about the product or about the test.
Weakening it to go green hides the first; keeping a wrong test as friction wastes every later change.

### Enforcement

Required.

### Scope

Applies to every failing test and every test identified as friction, in local runs and in CI.
Does not apply to a test that fails because its dependency did not install; that is an errored check under [REAS-TEST-002](#reas-test-002).

### Rules

Classify the test before you change it or the product:

- Stale: the test captures an old contract. Change the assertion to the current contract and keep the setup and the edge cases it encodes.
- Patchwork: the test asserts on a double of owned logic, or passes for a reason unrelated to the contract. Re-pin the assertion to the observable contract, or delete the test only when it provably carries no unique coverage.
- Valid: the test drives a real entry point and the contract it names still holds. It caught a product defect. Fix the product or file the defect. Do not weaken, skip, retry to green, or delete the test.

Establish causation by code-path coupling: does the suspect change touch the path the test exercises?
Do not establish it from version-control history alone.
When you replace an assertion, keep the test's setup and the edge cases it excavated; a regression returns through the coverage that was deleted with the test.
Never add a skip or an expected-failure marker to hide a known defect.
A flaky test is a defect in the test or in the product; record it as one and fix the cause.

### Verification

- Review question: which of the three classes is this failing test, and what evidence of code-path coupling supports that?
- Record: a changed or deleted test names its class and the coverage that replaces it in the commit message.

## Moved rules

<a id="reas-sec-001"></a>
REAS-SEC-001 moved to [Security at seams](security.md#reas-sec-001) in 1.5.0-draft.
The identifier and its purpose did not change.

## Quality test

Would each test go red if the behavior it names were removed, and does any test double stand in for code this application owns?

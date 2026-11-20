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
- Assert the exact error message or the typed error variant, not a substring.
- Assert an observable outcome: a return value, a state change, or an effect. A test with no assertion cannot fail.

Do not write a test for a constructor or an accessor alone; a behavior test covers construction.
Validation rules are behavior and get explicit tests.
`cargo mutants` reports which tests a mutation survives; run it on a module before review, and read the score as a triage signal, not a target.

### Allowances

A test that pins an interface for a consumer outside this repository may restate that interface; name the consumer in the test.

### Verification

- Review question: for each new test, what change to the code under test would make it fail?
- Review question: which assertion in this test is a positive control for its negative assertions?
- Tool: `cargo mutants --in-diff` on the changed files before review, with surviving mutants either killed or recorded as equivalent.

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
- A parsed value encodes back to the input bytes.
- Each duplicate implementation returns the same result as its reference, as [REAS-SEC-005](security.md#reas-sec-005) specifies.
- The adapter rejects an input above the resource limit before it allocates.

Build each target with `cargo fuzz`.
Keep debug assertions and overflow checks enabled in the fuzz build.

Persist the corpus.
Commit a seed corpus with at least one valid input for each target.
Store the corpus that CI finds in artifact storage or a corpus repository.
Start the next run from that corpus.
Minimize the corpus on a schedule.
Record each crash artifact as a bug and add a regression test.

Set a time budget for each stage and record it in the workflow file:

- Pull request: a short run for each target, for example 30 to 60 seconds, from the persisted corpus.
- Nightly: a longer run, for example 30 minutes for each target.
- Release: a run from the full corpus, for example several hours, before the release is cut.

Run the targets on each CPU architecture and target-feature set that the code deploys to.
SIMD paths and endianness differ between them.

### Verification

- CI: one fuzz job for each stage, with the budget in the workflow file.
- Review question: which inbound adapter has no target, and why?

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

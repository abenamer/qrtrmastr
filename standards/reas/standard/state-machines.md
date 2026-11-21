---
type: Engineering Standard
title: State machines and typestate
description: Runtime states, type-based states, and persisted transitions.
tags: [rust, fsm, typestate]
updated: 2026-11-20
---

# State machines and typestate

<a id="reas-state-001"></a>
## REAS-STATE-001: Model runtime states

### Intent

Several Boolean flags can represent combinations that no workflow reaches.
One enum names exactly the states that exist and makes each transition a function the compiler can check.

### Enforcement

Required.

### Scope

Applies to every set of mutually exclusive workflow states that the program holds at run time.
Does not apply to independent flags that can combine freely.

### Rules

Use a state machine for mutually exclusive workflow states.
Use one enum for the runtime state.
Do not encode that state with several Boolean flags.
Write each legal transition as a function.
Return a typed error when a runtime state does not permit the transition.
Match all variants of the state enum without a wildcard arm.

### Verification

- Review question: which combination of the flags this enum replaced could the old code reach that the workflow never defined?
- Test: one test for each legal transition and one for each refused transition, as [REAS-TEST-001](validation.md#reas-test-001) specifies.

<a id="reas-state-002"></a>
## REAS-STATE-002: Reject invalid operations with typestate

### Intent

An operation that is invalid in a state should not compile, not fail at run time.
A verification step modelled as a state cannot be skipped, because the handler's parameter type refuses the unverified value.

### Enforcement

Advisory.

### Scope

Applies when operation order, resource initialization, or a verification step is an invariant.
Does not apply to state that arrives from a file, a database, or a network, which a runtime enum represents.

### Rules

Use typestate when operation order or resource initialization is an invariant.
Expose an operation only on the state types that permit it.
Bound the state parameter with a sealed trait when downstream crates must not add states.
Use one type parameter for each independent state dimension.
If the dimensions are not independent, use one state type or runtime enum.

Validate state from a file, database, or network at run time.
Use a runtime enum to select the state-specific operation.
After validation, you can construct a typestate value for later operations.
Typestate does not replace validation of external state.

Model each authentication, authorization, and signature verification step as a state.
Give the handler a parameter of the verified state type only, for example `fn handle(webhook: Webhook<Verified>)`.
Then a call with the unverified type does not compile, and the code cannot skip the step.
Construct the verified type only in the function that does the check.
Keep that construction private to its module.
A fuzzer cannot detect a skipped check; the type does.

### Verification

- Test: a compile-fail doctest that passes the unverified type to the handler, and one that implements the sealed state trait from outside the crate, as [REAS-TEST-001](validation.md#reas-test-001) specifies.
- Review question: which function constructs the verified type, and is it the one that performs the check?

<a id="reas-state-003"></a>
## REAS-STATE-003: Guard persisted transitions

### Intent

Between a read and a write, another writer can change the persisted state.
A transition that does not check the expected state or version at write time overwrites that writer's change without noticing.

### Enforcement

Required.

### Scope

Applies to every state transition that is written to a database or another shared store.
Does not apply to in-process state that one owner holds.

### Rules

A persisted state can change between a read and a write.
Use a conditional update or a row lock for each transition.
For a conditional update, include the record identifier and expected state in the condition.
For example, use `UPDATE jobs SET state = $1 WHERE id = $2 AND state = $3`.
Report success only when the update changes one row.
Report a conflict or missing record when the update changes no rows.

Use a version column when the operation must detect intervening changes.
This includes changes that return to the same state.
The expected version in the condition is a fencing token: a write that carries an older version than the stored one is refused, so at most one writer wins each version.
For a row lock, read the state with `SELECT ... FOR UPDATE`.
Make the transition decision after that read.
Write the new state in the same transaction.
Keep the lock until the transaction ends.
Typestate alone cannot prevent a stale read or a lost update.

### Verification

- Test: two writers read the same row, both attempt the transition, and exactly one succeeds; run it against the production database engine, as [REAS-TEST-001](validation.md#reas-test-001) specifies.
- Review question: which code path examines the affected-row count, and what does it do when the count is zero?

The [typestate example](../examples/typestate/) demonstrates in-process state restrictions.
It does not demonstrate persisted transitions.
The [trust-boundary example](../examples/trust-boundary/) demonstrates a verification step as a state.

## Quality test

Is there a state the program can reach that no enum variant or type parameter names, and can any handler be called with a value that skipped its verification?

---
type: Engineering Standard
title: Native code
description: The obligations that begin where the compiler stops vouching, for unsafe, FFI, SIMD, and inline assembly.
tags: [rust, unsafe, ffi, simd, fuzzing, miri]
updated: 2026-11-20
---

# Native code

Native code is code where the language's safety guarantees no longer hold.
In Rust that means an `unsafe` block or function, an `extern` block or a `-sys` crate, a `core::arch` intrinsic or a `#[target_feature]` function, inline assembly, and a dependency that contains any of these on a path your data reaches.
Inside that boundary the compiler and the type system stop vouching for memory safety, so something else must take their place, on purpose.

The rules in this concept are a floor, not a ceiling.
They clear the mechanical, known failure classes cheaply: an unsafe block nobody counted, a length nobody checked, a precondition nobody wrote down, a memory error the compiler would have caught in safe code.
They do that so that attention is free for the part only judgment catches: the seam where two correct pieces disagree, the invariant that lives between modules, the bug no tool is watching for.
The tools named here do not think for the engineer.
They make sure that when a real problem arises, the obvious causes are already ruled out and the path to a solution is shorter.
A rule followed as ritual, without an understanding of what it buys, is the failure this concept exists to prevent.
Each rule therefore states its reasoning next to its practice, so that the practice survives a problem it did not anticipate.

Scale the response to exposure.
Question 5 of [REAS-ARCH-003](ports-and-adapters.md#reas-arch-003) records who can supply outside data.
A private tool whose only source of outside data is its maintainer applies REAS-NATIVE-001 and REAS-NATIVE-003 in full and may record an exception for the CI jobs of REAS-NATIVE-002 and REAS-NATIVE-004.
An application that accepts input from other parties applies all four.

<a id="reas-native-001"></a>
## REAS-NATIVE-001: Recognize the boundary

### Intent

In safe Rust the compiler vouches for memory safety and the type system carries the invariants.
Inside native code neither does.
The response to that loss starts with noticing it, so the crossing itself must trigger a standing response rather than a case-by-case judgment about whether this particular block is fine.

### Enforcement

Advisory.

### Scope

Applies to every engineer who writes, reviews, or depends on native code, and to every change that adds or touches it.
Does not apply to safe Rust, where the compiler's guarantees hold and the other REAS rules apply.

### Rules

Treat the crossing into native code as an event.
When you write, review, or depend on it, respond the same way each time:

1. Bound it, as [REAS-NATIVE-002](#reas-native-002) specifies: the unsafe surface is counted, and new unsafe is a reviewed event.
2. Mark it, as [REAS-NATIVE-003](#reas-native-003) specifies: the unsafe is confined behind a safe API, with its reasoning written where the code is, and forbidden where it is not needed.
3. Exercise it, as [REAS-NATIVE-004](#reas-native-004) specifies: the code runs on inputs the author did not choose, under a checker that watches memory, on the hardware the path needs.

Before you write native code, say why safe Rust cannot express the operation.
A reviewed crate with a safe API usually can.

Understand what each practice buys before you apply it, and write that reason into the code and the pull request.
A count without a reader, a `SAFETY` comment that restates the code, or a fuzz target that only asserts the absence of a panic satisfies the letter of a rule and none of its purpose.

For motivation, one instance of the class: in 2014 the Heartbleed defect in OpenSSL read a payload length from the network and copied that many bytes from a buffer that held fewer, inside code where no language rule could refuse the copy.
A bounded, marked, and exercised native path makes that class of defect a build or test failure instead of a disclosure.
The class is general; the rules here are not a fix for that incident.

### Verification

- Review question: name each place this change crosses into native code, and for each one the rule that bounded, marked, and exercised it.
- Record: a pull request that adds native code states why safe Rust could not express the operation.

<a id="reas-native-002"></a>
## REAS-NATIVE-002: Bound and track the unsafe surface

### Intent

An unsafe surface that nobody can enumerate grows silently, one justified block at a time, until no reviewer can read it in one sitting.
A counted surface makes each new block a visible, reviewed event and keeps the review effort proportional to what exists.

### Enforcement

Required.

### Scope

Applies to every crate the project builds and to the dependencies on paths that outside data reaches.
Does not apply to code that `unsafe_code = "forbid"` excludes, where the count is zero by construction; the rule asks only that the zero be recorded.

### Rules

Keep the unsafe surface enumerable.
At any moment, list every `unsafe` block and function in your crates and in the dependencies on a path that outside data reaches.

Keep two counts.
The project's own crates form the shrink-only baseline, committed to the repository: growth fails the build and needs a justification comment on the changed line, a review, and the name of the allowlisted module that holds the new code.
Hold unsafe only in allowlisted modules; unsafe outside the allowlist fails the build.
Dependency counts are reported on every run and compared with the previous run.
A change in a dependency count is a review event that names the dependency and what moved; it is not a build failure, and it does not by itself mean a defect.
A dependency whose count rises is a finding with a bounded response, as [REAS-SEC-001](security.md#reas-sec-001) specifies: a trusted `cargo vet` audit of the new version closes it, and your own review is needed only when no audit covers it and the dependency sits in a consequential component or on a path outside data reaches.

The current enforcement is `cargo geiger`, which produces the per-crate count of unsafe items in a crate and its dependency graph.
Run it in CI on each pull request and compare the result with the baseline.
The tool counts; it does not judge, and it does not see unsafe hidden in a build script or a macro expansion it cannot follow.
Another counter that produces the same enumeration satisfies this rule.

### Allowances

A private tool whose only source of outside data is its maintainer may run the count locally before each release instead of in CI.
It records the exception and the last count in an architecture decision.

### Verification

- CI: a job that runs the counter, fails when a first-party crate's count exceeds its baseline or when unsafe appears outside an allowlisted module, and reports dependency changes.
- Evidence: the [self-audit](self-audit.md) cites the CI run, its revision, and the baseline file for the current count, as [REAS-TEST-002](validation.md#reas-test-002) specifies.
- Review question: what is the unsafe count of each crate this change touches, and did it move?

### Limits

The count establishes how many unsafe items exist where the counter can see them.
It does not see unsafe in a build script or a macro expansion it cannot follow, and a stable count says nothing about whether the existing items are sound.

<a id="reas-native-003"></a>
## REAS-NATIVE-003: Confine and mark

### Intent

A reviewer can hold one small module in mind and check each precondition against the safe API that establishes it.
A `SAFETY` comment written at the point of the operation records the reasoning while the author still has it.
A crate that forbids unsafe lets the compiler enforce its absence everywhere else.

### Enforcement

Required.

### Scope

Applies to every `unsafe` block and function the project writes.
Does not apply to unsafe inside dependencies, which [REAS-NATIVE-002](#reas-native-002) counts and [REAS-SEC-001](security.md#reas-sec-001) reviews.

### Rules

Forbid unsafe where it is not needed.
Set `unsafe_code = "forbid"` in every crate, and lower it to `deny` only in a crate that holds an allowlisted module.

Confine it.
Put each native capability in one small module.
Give the module a safe public API that checks every precondition before the unsafe operation.
Keep the unsafe operation private to the module.

Mark it.
Write a `SAFETY` comment before each `unsafe` block that states the conditions and why they hold at that point.
Write a `# Safety` section on each public unsafe function that states the caller's obligations.
Do one unsafe operation in each block, so that each comment covers one claim.

Before you write your own, use a reviewed crate with a safe API for the same operation.
For byte-level reinterpretation that is `zerocopy` or `bytemuck`; for SIMD it is autovectorization first, then a crate with a safe SIMD API; `std::arch` intrinsics come only after a measurement shows the need.

[REAS-SEC-004](security.md#reas-sec-004) holds the mechanics and the lint settings.
This rule states the habit.

### Verification

- Lint: the rustc `unsafe_code` lint; `clippy::undocumented_unsafe_blocks` and `clippy::multiple_unsafe_ops_per_block` set to `deny`.
- Review question: which line of the safe API checks each precondition that the `SAFETY` comment relies on?

<a id="reas-native-004"></a>
## REAS-NATIVE-004: Exercise it

### Intent

In safe Rust the compiler catches a wrong length or a dangling reference before the program runs.
In native code nothing does until the code runs on the input that triggers it.
So the code must run, on inputs the author did not choose, under a checker that watches memory, on the hardware the path needs.

### Enforcement

Required.

### Scope

Applies to every native path, and to every path that outside bytes travel before they become validated types.
Does not apply to safe code with no native path and no outside bytes, which [REAS-TEST-001](validation.md#reas-test-001) covers.

### Rules

Fuzz the bytes.
Every parser or decoder that reads outside bytes has a fuzz target with property assertions, as [REAS-TEST-003](validation.md#reas-test-003) specifies.
Natively-written parsing is where the language can no longer catch a mistake for you, so this is where the fuzzer earns its budget.
Where the parser has a native fast path, the target asserts that it equals the reference, as [REAS-SEC-005](security.md#reas-sec-005) specifies.

Check the memory.
Run the tests of each module that contains unsafe under an interpreter that detects undefined behavior.
The current enforcement is `cargo miri test` in CI, on the crate that holds the allowlisted module.

Name what the interpreter cannot see.
Miri cannot execute a call across FFI, a vendor SIMD intrinsic, or inline assembly.
Mark each such path Miri-blind in the module's documentation and in the self-audit.
Cover it with a sanitizer build, AddressSanitizer and MemorySanitizer where the target supports them, on hardware that has the CPU feature, and compile the fuzz target for that feature.

Record the hardware.
A CPU-gated path is exercised only on a target that has the feature; a run on other hardware exercises the fallback, not the path.
Say which one ran.

### Allowances

A private tool whose only source of outside data is its maintainer may replace the CI jobs with a recorded local run before each release.
The Miri-blind marks stay in all cases, because they tell the next reader where no tool is watching.

### Verification

- CI: a Miri job for each crate with an allowlisted module; the fuzz jobs of REAS-TEST-003; a sanitizer job for each Miri-blind path on a runner that has the feature.
- Evidence: the [self-audit](self-audit.md) cites each run with its revision and, for a CPU-gated path, the runner's hardware target and target-feature set, as [REAS-TEST-002](validation.md#reas-test-002) specifies.
- Review question: which native path in this change did Miri execute, which is Miri-blind, and on which hardware did the sanitizer and the fuzzer run?

### Limits

Miri reports that it detected no instance of the undefined behavior it supports, on the paths it executed, with the inputs the tests supplied; that is not a general absence guarantee.
It cannot execute FFI, vendor intrinsics, or inline assembly, and a sanitizer run reports the same, only for the hardware it ran on.

## Quality test

Can you list every unsafe block, FFI call, and CPU-gated path in this crate now, and for each one name the safe code that checks its preconditions, the test that exercises it, and the hardware it ran on?

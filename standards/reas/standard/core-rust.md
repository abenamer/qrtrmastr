---
type: Engineering Standard
title: Core Rust
description: Rules for Rust types, errors, ownership, documentation, and builds.
tags: [rust, types, traits, ownership, async, money, docs]
updated: 2026-11-20
---

# Core Rust

Each rule has the same shape: Intent, Enforcement, Scope, Rules, optional Allowances, and Verification.
The Enforcement section holds the level; the rule text does not repeat it.

<a id="reas-rust-001"></a>
## REAS-RUST-001: Make invariants explicit

### Intent

A value that can be invalid forces every function that receives it to check it again, and one missed check is a defect.
A type that cannot hold an invalid value moves the check to one place and lets the compiler enforce it everywhere else.

### Enforcement

Required.

### Scope

Applies to domain values and enums in every crate.
Does not apply to a local variable that does not leave the function that creates it.

### Rules

Use newtypes with private fields for domain values with extra invariants.
Use smart constructors to validate input.
Route deserialization through the same validation, as [REAS-SEC-002](security.md#reas-sec-002) specifies.
Make sure that all safe public APIs preserve the invariants.
Use `Option` for an optional value.
Use `Result` for an operation that can fail.

Use enum variants for the legal cases.
For enums that this project controls, match all variants without a wildcard arm.
For an external non-exhaustive enum, handle the unknown case explicitly.
Do not expose a field or setter that can make a domain value invalid.
This rule has no exception for a first version: a value passed as a bare `String` or integer is copied at every call site, and every later change and every agent copies the pattern, so the newtype is cheapest on the first commit, as [REAS-FOUND-002](foundations.md#reas-found-002) explains.

Record a new variant in a public exhaustive enum as a breaking API change.
Mark a public enum that is expected to grow with `#[non_exhaustive]`.
Then a new variant is not a breaking change, and a downstream match needs a wildcard arm.
For a published library crate, run `cargo semver-checks` before each release.

### Verification

- Review question: can any caller construct a value of this type without its smart constructor?
- Lint: `clippy::wildcard_enum_match_arm` set to `deny`. A match on an external non-exhaustive enum carries `#[expect]` with the reason.
- Test: one rejection test for each invariant, through the constructor and through the deserializer, as [REAS-TEST-001](validation.md#reas-test-001) specifies.
- CI: `cargo semver-checks` for each published library crate.

<a id="reas-rust-002"></a>
## REAS-RUST-002: Select abstractions for the reader

### Intent

A reader must follow the code path from the use case to the adapter.
Runtime cost is measured, not assumed, and the term zero-cost describes runtime only.

### Enforcement

Advisory.

### Scope

Applies to the choice between concrete types, generics, trait objects, cloning, and allocation in application and library code.
Does not apply to code that a benchmark has shown to be on a hot path; there, the measurement decides.

### Rules

Select an abstraction that makes the code path easy to follow.
Then measure its runtime cost.
A zero-cost abstraction adds no runtime cost compared with equivalent lower-level code.
The term does not describe readability or compilation time.

Generics and `impl Trait` do not add dynamic dispatch by themselves.
Generic code can also call a trait object.
A call to a statically known implementation uses static dispatch.
Trait objects use dynamic dispatch.

A new `Box<T>` allocates storage when `T` has nonzero size.
Conversion from `Box<T>` to `Box<dyn Trait>` does not add an allocation.
A reference to a trait object does not allocate storage for the value.
Each `Clone` implementation has its own cost.
For an abstraction with extra runtime cost, explain why it is necessary.
Do not remove an abstraction only to decrease compilation time.

### Verification

- Review question: can a reader follow one operation from the use case to the adapter with one file open for each step?
- Benchmark: when runtime cost is the reason for a choice, the pull request includes the measurement.

<a id="reas-rust-003"></a>
## REAS-RUST-003: Make trait relationships explicit

### Intent

A trait implementation that is correct for some types and silently wrong for others, or a method that resolves to a different implementation than the reader expects, is a defect the compiler does not report.

### Enforcement

Required.

### Scope

Applies to trait definitions and implementations in every crate.
Does not apply to derived implementations of standard traits.

### Rules

Use a blanket implementation only when its behavior is correct for all types that meet its bounds.
Examine coherence and orphan rules before you add an implementation.
Record implementation conflicts that a future adapter could cause.
Do not build a generic repository framework before a second concrete implementation exists.
The ease of generating the framework is not evidence that it is needed.

Do not give an inherent method the same name as a trait method on that type.
For the same candidate receiver type, Rust searches inherent methods before trait methods.
Automatic borrowing and dereferencing affect the candidate order.
A trait method can have priority.

### Verification

- Compiler: coherence and orphan-rule violations do not compile; the rule is to design so that a later adapter does not meet them.
- Lint: `clippy::same_name_method` set to `deny`.
- Review question: which second concrete implementation justifies each generic trait?

<a id="reas-rust-004"></a>
## REAS-RUST-004: Make ownership explicit

### Intent

A reader must find the owner of each value and the end of each borrow without running the program.
Shared ownership and interior mutability hide both.

### Enforcement

Required.

### Scope

Applies to the data model of application and library code.
Does not apply to test code.

### Rules

Make responsibility for each owned value easy to find.
End each borrow after its last necessary use.
Use `Arc<Mutex<T>>` only for shared ownership and synchronized mutation.
If lifetime parameters obscure the data relationships, first examine a different data model.
Record the reason when the current model is necessary.

Accept `&str`, `&[T]`, `&Path`, or `impl AsRef<_>` for a value that the function only reads.
Take ownership when the function stores the value.
Clone deliberately; a clone that exists only to satisfy the borrow checker hides an ownership design problem.

Do not use `unsafe` to violate aliasing or lifetime requirements.
The borrow checker also does its checks on references in an `unsafe` block.
When safe Rust cannot express an operation, treat the crossing as [Native code](native-code.md) specifies: bound it, mark it, and exercise it. [REAS-SEC-004](security.md#reas-sec-004) holds the confinement mechanics.

### Verification

- Lint: `clippy::needless_pass_by_value` and `clippy::redundant_clone` set to `warn`; each remaining hit carries `#[expect]` with the reason.
- Review question: for each `Arc<Mutex<T>>`, who else owns the value, and how long is the lock held?

<a id="reas-rust-005"></a>
## REAS-RUST-005: Handle errors explicitly

### Intent

A caller can act on a failure only when the error names its cause.
A panic on a recoverable path turns a bad input into a crash.

### Enforcement

Required.

### Scope

Applies to every operation that can fail, in library and binary crates.
Does not apply to test code, where `unwrap` and `expect` on a fixture are acceptable.

### Rules

In a library crate, return `Result` with a typed error.
Define one error enum for each module or crate surface, with one variant for each cause a caller can act on.
Carry the data the caller needs to act, and the data a person needs to start troubleshooting: the operation, the expected condition, and the observed one.
Wrap a lower-level error with `#[source]` or `#[from]` so that the error chain is kept; the chain is the diagnostic trail from the boundary that failed back to its cause.
`thiserror` is the usual derive.
A type-erased error such as `anyhow::Error` does not appear in a library's public API.

In a binary crate, add context at each step with `anyhow` and `.context()`, so that the report says what the program was doing, not only what failed underneath.
Return a failure exit status when the command fails.
An inbound adapter converts an error to a response or message after it handles that error.
Do not return success after a failure.

Use `?` to propagate an error that the current function cannot handle.
Panic only for a bug.
For example, an invariant violation after validation can be a bug.
Do not call `unwrap` when `None` or `Err` can occur.
When failure is not possible at that point, use `expect` with the reason.
Do not substitute a default for a missing required input.

### Verification

- Lint: `clippy::unwrap_used` set to `deny`; `clippy::expect_used` set to `warn` with `#[expect]` and a reason on each accepted use.
- Review question: can a caller of this library match on the cause of each failure it must handle?
- Test: each error variant has a test that produces it, as [REAS-TEST-001](validation.md#reas-test-001) specifies.

<a id="reas-rust-006"></a>
## REAS-RUST-006: Compile after each implementation step

### Intent

A compiler error found after one change names the change.
The same error found after ten changes names none of them.

### Enforcement

Required.

### Scope

Applies to every change to a type, function, module, manifest, or lint setting.
Does not apply to a documentation-only change.

### Rules

Compile after each change to a type, function, or module.
When Cargo is available, run these commands from the workspace root:

```bash
cargo fmt --all
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

For packages outside a shared workspace, use `--manifest-path` for each package.
Report only the checks that ran.
If the hosted build cannot run Cargo, compile locally.
Do not make compilation speed a design objective.

Declare lint levels once, under `[workspace.lints]` in the workspace manifest, and set `[lints] workspace = true` in each crate.
For a package outside a workspace, declare them under `[lints]` in its own manifest.
Suppress a lint with `#[expect(lint, reason = "...")]`, not `#[allow]`.
Then a suppression that no longer applies becomes a warning.

### Verification

- CI: the same commands run in CI as in this list; the workflow file is the record.
- Lint: `clippy::allow_attributes` and `clippy::allow_attributes_without_reason` set to `deny`.
- Review question: does the pull request state which of these commands ran?

<a id="reas-rust-007"></a>
## REAS-RUST-007: Separate async work and CPU work

### Intent

An async executor thread that blocks stops every task scheduled on it.
A lock held across an await point can deadlock the executor.

### Enforcement

Required.

### Scope

Applies to every crate that uses an async runtime.
Does not apply to a synchronous program.

### Rules

Do not run blocking I/O or CPU-intensive work on an async executor thread.
Use `tokio::task::spawn_blocking`, a Rayon pool, or a bounded worker pool.
Set a limit on concurrent CPU work before submission.
Do not hold a `std::sync::Mutex` guard across an `.await`.

Record how each task responds to cancellation and shutdown.
A started `spawn_blocking` task does not stop when its handle receives `abort`.
For work that must stop, use cooperative cancellation.
Set a shutdown deadline.
Record the shutdown behavior for unfinished work after that deadline.

### Verification

- Lint: `clippy::await_holding_lock` is in the default set and fails under `-D warnings`.
- Review question: which task runs CPU work, on which pool, and with which concurrency limit?
- Test: a shutdown test that starts long work, requests shutdown, and asserts the process exits by the deadline.

<a id="reas-rust-008"></a>
## REAS-RUST-008: Represent money exactly

### Intent

Binary floating point cannot represent most decimal fractions, so totals drift and comparisons fail.
A bare integer loses the currency and overflows silently in release.

### Enforcement

Required.

### Scope

Applies to every value that represents an amount of money and every rate applied to one.
Does not apply to counts, durations, or measurements.

### Rules

Hold money as integer minor units inside a type that also carries the currency, when the scale is fixed and the arithmetic adds and subtracts.
Use checked arithmetic and return a typed error on overflow.
Refuse to add or compare amounts in different currencies.
Use a decimal type with an explicit rounding mode when the arithmetic divides, applies a rate, or the scale varies.
Name the rounding mode at each rounding point.
Never use `f32` or `f64` for money.
Never pass money as a bare `i64`.
In PostgreSQL, store money as an integer of minor units or as `NUMERIC`, never as a floating-point column.
Convert between the storage type and the domain type at the adapter.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money {
    minor: i64,
    currency: Currency,
}

impl Money {
    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch(self.currency, other.currency));
        }
        let minor = self.minor.checked_add(other.minor).ok_or(MoneyError::Overflow)?;
        Ok(Money { minor, currency: self.currency })
    }
}
```

### Verification

- Lint: `clippy::float_arithmetic` set to `deny` in each module that handles money.
- Review question: what is the type of each money field, and where is each rounding mode named?
- Test: a rounding test at the half-unit boundary, an overflow test, and a currency-mismatch test.

<a id="reas-rust-009"></a>
## REAS-RUST-009: Document public items

### Intent

A public item without documentation forces the reader into the implementation.
A fallible function whose errors are not listed forces the caller to guess which failures to handle.

### Enforcement

Required.

### Scope

Applies to library crates and to the public items of application crates.
Does not apply to private items or to test code.

### Rules

Give each public item a `///` doc comment.
Set `missing_docs = "warn"` under `[lints.rust]` for each library crate.
For a fallible function, add an `# Errors` section that names each error variant and its cause.
For a function that can panic, add a `# Panics` section that names the condition.
For a public unsafe function, add a `# Safety` section, as [REAS-SEC-004](security.md#reas-sec-004) specifies.
Write a public usage example as a doctest, so that it compiles and runs.
Build the documentation with `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`, so that a broken intra-doc link fails the build.

### Verification

- Lint: rustc `missing_docs`; `clippy::missing_errors_doc` and `clippy::missing_panics_doc` set to `warn`; `clippy::missing_safety_doc` is in the default set.
- CI: the documentation build with `-D warnings`.
- Review question: does each public fallible function list the errors a caller must handle?

## Quality test

Can any caller construct a domain value or a money amount without a fallible constructor, and would a new variant on a domain enum compile without a change to each match on it?

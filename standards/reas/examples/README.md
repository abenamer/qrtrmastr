# Rust examples

Each subdirectory is an independent Cargo package.
The examples commit `Cargo.lock` and use the root `rust-toolchain.toml`.

- [Ports and adapters](ports-and-adapters/) includes a CLI command function, use case, port trait, and in-memory outbound adapter.
  One composition root connects them.
  The tests include input boundaries, missing records, and repository failures.
- [Typestate](typestate/) includes two legal transitions and a sealed state trait.
  Three compile-fail doctests cover an invalid operation, an invalid state argument, and an attempted downstream state implementation.
- [Trust boundary](trust-boundary/) includes one inbound adapter that parses a length-prefixed frame with checked arithmetic, verifies its origin through a port before it parses the payload, and decodes a validated event type through serde `try_from`.
  A verification state makes the handler reject an unverified webhook at compile time.
  A SWAR checksum duplicates a scalar reference, with a differential test.
  The manifest forbids `unsafe`, denies unchecked arithmetic and indexing, and enables overflow checks in release.
  The `fuzz/` subcrate holds one libFuzzer target with a seed corpus.

The CLI example returns rendered text from a library function.
A CLI executable must also set the exit status for a failed command.
The examples do not include a database adapter, persisted transitions, transaction ownership, an async port, a message authentication code, output encoding, or a dependency scan.

Run the stable checks for every crate, including the fuzz crate's lint and the documentation build with warnings denied:

```bash
scripts/check.sh rust
```

Each manifest declares `rust-version = "1.88"`, forbids `unsafe`, and warns on a missing doc comment; the `msrv` CI job compiles each crate on that toolchain.

Stable rustdoc tests that a `compile_fail` snippet does not compile.
It does not test the expected error code.
The nightly job also tests the expected diagnostic codes:

```bash
rustup toolchain install nightly-2026-10-23 --profile minimal
cargo +nightly-2026-10-23 test --locked --manifest-path examples/typestate/Cargo.toml --doc
```

Nightly rustdoc fails a doctest when an expected error code is absent.
It can accept extra diagnostics.

Run the nightly checks, which include the fuzz target with the pull-request budget from REAS-TEST-003:

```bash
cargo +nightly-2026-10-23 install cargo-fuzz --locked
scripts/check.sh nightly
```

The run reads the seed corpus in `fuzz/corpus/receive/` and writes new inputs there.
CI discards those inputs; a project that deploys the code must persist them.
The [self-audit](../standard/self-audit.md) identifies the code revision and commands for each recorded result.

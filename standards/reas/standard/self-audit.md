---
type: Audit Report
title: REAS self-audit
description: Revision-specific validation evidence and remaining limitations.
tags: [quality, documentation, audit]
updated: 2026-11-20
---

# REAS self-audit

**Recorded validation: Actions run 24 passed for implementation commit `843b94d`.**

The language review does not establish full ASD-STE100 compliance.
See the [language-review record](ste-review.md) for its scope and open checks.

## Remote evidence for this revision

[Actions run 24](https://github.com/abenamer/reas/actions/runs/44831443937) passed on 2026-11-20.
Its source commit is `843b94d` on `claude/zen-newton-74jyn8`, which added the Native code concept and changed no code, test, lockfile, workflow, or script.
The job list and step conclusions were read from the Actions API after execution.
Each row reports one of the six outcomes of [REAS-TEST-002](validation.md#reas-test-002); no job was skipped, retried, or timed out.

| Job | Outcome | Evidence |
| --- | --- | --- |
| [msrv](https://github.com/abenamer/reas/actions/runs/44831443937/job/136617111096) | Passed | 17 seconds, conclusion success, every step success |
| [rust-examples-arm64](https://github.com/abenamer/reas/actions/runs/44831443937/job/136617111346) | Passed | 31 seconds, conclusion success, every step success |
| [docs](https://github.com/abenamer/reas/actions/runs/44831443937/job/136617111483) | Passed | 8 seconds, conclusion success, every step success |
| [rust-examples](https://github.com/abenamer/reas/actions/runs/44831443937/job/136617111528) | Passed | 26 seconds, conclusion success, every step success |
| [nightly-doctests](https://github.com/abenamer/reas/actions/runs/44831443937/job/136617111689) | Passed | 107 seconds, conclusion success, every step success |

No job in this run exercises a native path, because the examples contain none; see the native-paths row of the coverage table.
This audit section was added in the next commit.

## Remote evidence for 1.6.0-draft

[Actions run 20](https://github.com/abenamer/reas/actions/runs/44820161148) passed on 2026-11-20.
Its source commit is `f208fb5` on `claude/zen-newton-74jyn8`.
The job list and step conclusions were read from the Actions API after execution.
Each row reports one of the six outcomes of [REAS-TEST-002](validation.md#reas-test-002); no job was skipped, retried, or timed out.

| Job | Outcome | Evidence |
| --- | --- | --- |
| [docs](https://github.com/abenamer/reas/actions/runs/44820161148/job/136578673670) | Passed | 9 seconds, conclusion success, every step success |
| [msrv](https://github.com/abenamer/reas/actions/runs/44820161148/job/136578674564) | Passed | 16 seconds, conclusion success, every step success |
| [rust-examples-arm64](https://github.com/abenamer/reas/actions/runs/44820161148/job/136578674730) | Passed | 37 seconds, conclusion success, every step success |
| [rust-examples](https://github.com/abenamer/reas/actions/runs/44820161148/job/136578674779) | Passed | 43 seconds, conclusion success, every step success |
| [nightly-doctests](https://github.com/abenamer/reas/actions/runs/44820161148/job/136578674810) | Passed | 102 seconds, conclusion success, every step success |

The `msrv` job ran for the first time and compiled every example on Rust 1.88.0.
The `rust-examples` and `rust-examples-arm64` jobs ran `scripts/check.sh rust`, which now includes the fuzz crate's Clippy and rustfmt and each crate's documentation build with warnings denied.
Run 20 verifies the files at `f208fb5`.
This audit section was added in the next commit and did not change the checker, tests, example code, lockfiles, workflow, or check script.

## Path-to-job coverage

[REAS-DOC-006](documentation.md#reas-doc-006) forbids a zero-gated path: a file a change can touch that no job checks.
Every job runs one subcommand of `scripts/check.sh`, so this table and the workflow cannot drift apart.

| Path group | Job | Check |
| --- | --- | --- |
| `standard/**`, root `*.md`, `templates/**` | `docs` | `scripts/check.sh docs`: frontmatter, index reachability, rule shape, quality tests, retired terms, links, freshness |
| `scripts/check_docs.py`, `tests/**`, `requirements-docs.txt` | `docs` | The checker's 59 unit tests, including a planted violation for each gate |
| `examples/*/src/**`, `examples/*/Cargo.toml`, `examples/*/Cargo.lock` | `rust-examples`, `rust-examples-arm64`, `msrv` | check, test, Clippy with `-D warnings`, rustfmt, documentation build with `-D warnings`, on x86_64, aarch64, and Rust 1.88.0 |
| `examples/trust-boundary/fuzz/**` | `rust-examples`, `nightly-doctests` | Clippy and rustfmt on stable; build and a 30-second run on nightly |
| Compile-fail doctests | `nightly-doctests` | Expected diagnostic codes asserted |
| `scripts/check.sh` | every job | A defect in the script fails every job |
| Native paths ([REAS-NATIVE-002](native-code.md#reas-native-002), [REAS-NATIVE-004](native-code.md#reas-native-004)) | `rust-examples` | Every example sets `unsafe_code = "forbid"`, so the count is zero by construction and the compiler is the counter; no FFI; the one fast path, the SWAR checksum, is portable integer arithmetic with no target feature, so no path is CPU-gated, Miri is not required, and no path is Miri-blind. `cargo geiger` was not run. |
| `.github/workflows/docs.yml` | none beyond its own execution | Not gated by a workflow linter. A syntax error fails every job; a wrong step name does not. |
| `LICENSE-*`, `.gitignore`, `rust-toolchain.toml` | none | Not gated. The toolchain file is read by every Cargo job but its content is not asserted. |

## Local evidence: 2026-11-20

The local environment uses Python 3, Rust 1.99.1 from `rust-toolchain.toml`, Rust 1.88.0 for the MSRV check, `nightly-2026-10-23`, and cargo-fuzz 0.13.2.
Each row reports one of the six outcomes of [REAS-TEST-002](validation.md#reas-test-002).
The checks ran on the 1.6.0-draft worktree before its first commit.
This local result has no immutable source commit and is not release evidence.

| Check | Outcome | Evidence |
| --- | --- | --- |
| `scripts/check.sh docs`: `python3 scripts/check_docs.py` | Passed | PASS line; zero freshness warnings on 2026-11-20 |
| `scripts/check.sh docs`: `python3 -m unittest discover -s tests -v` | Passed | 59 tests, 0 failed, 0 skipped |
| `scripts/check.sh rust`: adapter example | Passed | check, 12 unit tests, Clippy, rustfmt, doc build |
| `scripts/check.sh rust`: typestate example | Passed | check, 3 unit tests, 4 doctests, Clippy, rustfmt, doc build |
| `scripts/check.sh rust`: trust-boundary example | Passed | check, 24 unit tests, 3 doctests, Clippy, rustfmt, doc build |
| `scripts/check.sh rust`: fuzz crate Clippy and rustfmt | Passed | first run of this check; it closes the zero-gated path the 1.5.0-draft audit missed |
| MSRV: `cargo +1.88.0 check --locked --all-targets` for each crate | Passed | three crates, exit 0 |
| `scripts/check.sh nightly`: typestate doctests | Passed | 4 doctests, 3 compile-fail with expected codes |
| `scripts/check.sh nightly`: trust-boundary doctests | Passed | 3 doctests, 2 compile-fail with expected codes |
| `scripts/check.sh nightly`: `cargo fuzz build receive` | Passed | built on nightly-2026-10-23 |
| `scripts/check.sh nightly`: `cargo fuzz run receive -- -max_total_time=30` | Passed | 5,059,587 executions in 31 seconds, no crash, no artifact; corpus trimmed back to the two seeds before commit |
| Differential tests on aarch64 | Not run | No aarch64 machine locally; the `rust-examples-arm64` job runs them |
| `cargo deny check` | Not run | Not installed, and the examples are not applications, so REAS-SEC-001 does not require it for them |
| `cargo mutants` | Not run | Not installed; REAS-TEST-001 names it as a pre-review tool, not a CI gate |
| Miri | Not run | Every example forbids `unsafe`, so REAS-SEC-004 requires no Miri job for them |
| ASD-STE100 language review of the 1.6.0-draft text | Not run | See the review record |

## Local evidence: 2026-11-18

The local environment uses Python 3, Rust 1.99.1 from `rust-toolchain.toml`, `nightly-2026-10-23`, and cargo-fuzz 0.13.2.
The checks below ran on the 1.5.0-draft worktree before its first commit.
This local result has no immutable source commit and is not release evidence.

| Check | Outcome |
| --- | --- |
| `python scripts/check_docs.py` | Passed locally. |
| `python -m unittest discover -s tests -v` | Passed locally. |
| Adapter example: `cargo check`, `cargo test`, Clippy, rustfmt, all `--locked` | Passed locally, 12 unit tests. |
| Typestate example: same commands | Passed locally, three unit tests and four doctests. |
| Trust-boundary example: same commands | Passed locally, 24 unit tests, one doctest, two compile-fail doctests. |
| Trust-boundary example: nightly `cargo test --locked --doc` | Passed locally. Two compile-fail tests asserted expected diagnostic codes E0308 and E0277. |
| Trust-boundary fuzz target: `cargo fuzz build receive` | Passed locally on nightly-2026-10-23. |
| Trust-boundary fuzz target: `cargo fuzz run receive -- -max_total_time=30` | Passed locally on x86_64. 5,873,517 executions in 31 seconds from the two seed inputs, no crash, no artifact. The run grew the corpus to 66 inputs; only the two seeds are committed. |
| Fuzz crate: `cargo fmt --all -- --check` | Passed locally. |
| Trust-boundary differential test on aarch64 | Not run locally. The `rust-examples-arm64` workflow job runs it; its first result arrives with the pull request checks. |
| Dependency scan with `cargo deny` or `cargo audit` | Not run. Neither tool is installed in this environment, and the workflow does not yet include the job that REAS-SEC-001 requires. |
| Miri on the examples | Not run. The examples forbid `unsafe`, so REAS-SEC-004 requires no Miri job for them. |
| ASD-STE100 language review of `security.md`, REAS-TEST-003, and the new example | Not run. See the review record. |

## Remote evidence for 1.5.0-draft

[Actions run 16](https://github.com/abenamer/reas/actions/runs/44304113162) passed on 2026-11-18.
Its source commit is `73ebe1d` on `claude/zen-newton-74jyn8`.
The job list and step conclusions were read from the Actions API after execution.

| Job | Result |
| --- | --- |
| [rust-examples-arm64](https://github.com/abenamer/reas/actions/runs/44304113162/job/134801114074) | Passed in 14 seconds. |
| [docs](https://github.com/abenamer/reas/actions/runs/44304113162/job/134801114297) | Passed in 6 seconds. |
| [rust-examples](https://github.com/abenamer/reas/actions/runs/44304113162/job/134801114424) | Passed in 20 seconds. |
| [nightly-doctests](https://github.com/abenamer/reas/actions/runs/44304113162/job/134801115368) | Passed in 99 seconds. |

The `rust-examples-arm64` job ran the trust-boundary differential test on aarch64 for the first time.
The `nightly-doctests` job installed cargo-fuzz, built the `receive` target, and ran it for the 30-second budget.
Run 16 verifies the files at `73ebe1d`.
This audit section was added in the next commit and did not change the checker, tests, example code, lockfiles, or workflow.

## Historical evidence

[Actions run 9](https://github.com/abenamer/reas/actions/runs/39990157419) passed on 2026-10-23 at commit `63c9434d241d899ff9e91258d7584ad85c25a9a6`.
It tested the 1.3.0-draft code on `main`.
Its Rust job used Rust 1.99.1.

That run executed seven Python tests and seven adapter-example unit tests.
It also executed one typestate unit test and three typestate doctests.
Two of those doctests were compile-fail tests.
Formatting and Clippy passed for both examples.
That workflow did not run nightly diagnostic-code assertions or a separate `cargo check` command.

The former audit cited run 5 at `43f1d1a` as current-code evidence.
That run tested the earlier `TeamId(u32)` implementation.
It did not test the subsequent `NonZeroU32` implementation.
Neither historical run verifies the changed code in this revision.

## Local evidence: 2026-10-23

The local environment uses Python 3 and Rust 1.99.1.
The diagnostic-code test uses `nightly-2026-10-23`.
The checks below ran on the implementation worktree before its first commit.
This local result has no immutable source commit and is not release evidence.
The remote evidence section must identify a committed revision before a release.

| Check | Outcome |
| --- | --- |
| `python scripts/check_docs.py` | Passed locally. |
| `python -m unittest discover -s tests -v` | Passed locally, 36 test methods with additional parameterized cases. |
| Adapter example: `cargo test --locked` | Passed locally, 12 unit tests. |
| Typestate example: `cargo test --locked` | Passed locally, three unit tests and four doctests. |
| Typestate example: nightly `cargo test --locked --doc` | Passed locally. Three compile-fail tests include expected diagnostic codes. |
| Both examples: `cargo check --locked --all-targets` | Passed locally. |
| Both examples: `cargo clippy --locked --all-targets -- -D warnings` | Passed locally. |
| Both examples: `cargo fmt --all -- --check` | Passed locally. |
| Complete STE vocabulary and grammar review | Not run in full. See the review record. |
| Database adapter, persisted transitions, and transaction integration tests | Not run. These examples do not contain a database adapter. |

## Remote evidence for 1.4.0-draft

[Actions run 11](https://github.com/abenamer/reas/actions/runs/39995381881) passed on 2026-10-23.
Its source commit is `d746f9adae750e8810a57652878cea83dc465d7b` on `codex/reas-precision-ste-review`.
The job logs were inspected after execution.

| Job | Verified result |
| --- | --- |
| [docs](https://github.com/abenamer/reas/actions/runs/39995381881/job/120557440061) | The structural checker and all 36 Python test methods passed. |
| [rust-examples](https://github.com/abenamer/reas/actions/runs/39995381881/job/120557440283) | Rust 1.99.1 passed Cargo check, tests, formatting, and Clippy for both packages. Adapter tests: 12 passed. Typestate tests: three unit tests and four doctests passed. |
| [nightly-doctests](https://github.com/abenamer/reas/actions/runs/39995381881/job/120557440532) | Rust nightly-2026-10-23 passed four doctests. Three compile-fail tests asserted expected diagnostic codes. |

Run 11 verifies the files at that commit, not subsequent edits.
The follow-up audit and wording edits do not change the checker, tests, example code, lockfiles, or workflow.
Their structural validation ran locally again.
Use the pull request checks for the results of later committed text revisions.

## Limits

The [checker reference](checker.md) states the supported structural checks.
Its metadata checks do not establish the truth of verification claims.
Stable rustdoc does not assert expected error codes.
Nightly rustdoc asserts the presence of expected codes, not the absence of extra diagnostics.
Historical identifier reuse requires review of [the identifier record](rule-identifiers.md).

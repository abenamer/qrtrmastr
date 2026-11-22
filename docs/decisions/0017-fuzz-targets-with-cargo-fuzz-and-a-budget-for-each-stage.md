---
type: Architecture Decision
title: Fuzz targets with cargo fuzz, a pinned nightly toolchain, and a budget for each stage
description: Why each reader of outside bytes has a fuzz target, what the targets assert, which toolchain and library they need, where they run, and what they do not cover.
tags: [decision, security, testing, dependencies, tooling]
generated: { by: "process:claude-code", at: "2026-11-21T23:10:00Z" }
updated: 2026-11-21
sources:
  - id: reas-test-003
    resource: ../../standards/reas/standard/validation.md
    title: REAS-TEST-003, fuzz each adapter that reads outside bytes
  - id: package
    resource: ../../fuzz/Cargo.toml
    title: The fuzz package
  - id: workflow
    resource: ../../.github/workflows/fuzz.yml
    title: The fuzz workflow, with the budget of each stage
  - id: check
    resource: ../../scripts/check.sh
    title: The targets and fuzz groups of the check script
---

# ADR 0017: Fuzz targets with cargo fuzz, a pinned nightly toolchain, and a budget for each stage

## Context

Mail is outside data from other parties ([ADR 0001](/decisions/0001-ports-and-adapters-for-a-private-tool.md), question 5). REAS-TEST-003 asks for one fuzz target for each inbound adapter and each parser that reads bytes from another party, with property assertions, a committed seed corpus, and a time budget for each stage in the workflow file. REAS-NATIVE-004 asks the same for every parser of outside bytes. This is step 7 of [ADR 0015](/decisions/0015-reas-1-7-0-draft-and-the-order-of-its-gaps.md); before it the project had no fuzz target, and the self-audit named seven readers.

`cargo fuzz` builds a target with libFuzzer and needs a nightly compiler. The project pins a stable compiler in `rust-toolchain.toml`. The maintainer said on 2026-11-21 to go on with the steps of ADR 0015, which names the nightly toolchain and `cargo fuzz` for this step.

## Decision

* The system shall have one fuzz target for each of the seven readers of outside bytes, in the package `fuzz/`:

  | Target | Entry point | What it asserts beside "a value or a typed error" |
  | --- | --- | --- |
  | `takeout` | `TakeoutExport::messages`, with the bytes as a mailbox file and as an archive | An error ends the stream and is a read error. The messages of a mailbox are together not larger than the file. A file that starts as a mailbox starts gives a message or an error, never an import of nothing. Fewer bytes than an archive has are an error. An archive whose mailbox holds no message gives nothing, as an empty label does. |
  | `mime` | `mime::parse` and `PlatformMail::prepare` | The stored hash is the hash of the bytes. The stored key comes back from its kind and its text as the same key. The key, the subject and the two bodies hold no configured term. |
  | `classify` | The four functions of `mail::classify` | A domain is in lower case and holds no `@`. Only a configured address gives a platform. A DKIM pass needs a domain, and a header of the trusted server that names it. The classification of a subject has no property: a kind is a variant of its type. |
  | `redact` | `Redactor::redact_text`, `redact_html`, `carries_configured_term` | A redacted text holds no configured term, or the error is the one for a term that survived. A text with no digit, no `@`, no `%` and no term is not refused. A term in a short text is found. |
  | `receipt` | `Registry::parse`, then `domain::receipt::reconcile` | One text gives one result. A receipt has lines numbered from 1 without a gap. The checks of the domain give a result. |
  | `pack` | `domain::pack::pack_amount` | An amount is above zero. The case of an ASCII letter and a space at an end change nothing. |
  | `gmail_json` | `list_page_from_json`, `raw_message_from_json` | An id is 1 to 64 ASCII letters and digits. The bytes of a message encode back to the text of the answer. |

* When a reader of outside bytes is added, the same change shall add its target, its seed and its name in the three lists; `scripts/check.sh targets` fails when the files of `fuzz/fuzz_targets/`, the manifest, the seeds and the list of the script do not name the same targets.
* The targets shall use made-up terms only: the person of the synthetic receipts. No seed and no target holds a value of a real person, and no real mail is used as a corpus.
* The seeds shall be derived files: `scripts/fuzz_seeds.py` writes them from the synthetic fixtures, and its `--check` fails on a seed that was edited by hand.
* The fuzz build shall use the library versions of the program: `fuzz/Cargo.lock` starts as a copy of `Cargo.lock`, and the `targets` group fails when a package that both hold differs in version.
* The budget of each stage is in `.github/workflows/fuzz.yml`: 30 seconds for each target on a pull request, 300 seconds for each target once a day on the default branch. The project publishes no release, so the release stage of the rule has no run; when a release exists, this decision is changed first.
* The toolchain of the fuzz build is `nightly-2026-11-15`, named once in `scripts/check.sh` and installed by the workflow. It moves in a pull request of its own, like the stable toolchain.
* When a target fails, the input is a defect of the reader or a property that is not true. The fix and a regression test in the program's own tests go into one change; the input is not committed as a file.
* The fuzz job is not one of the seven required checks. To require it is a setting of the repository, which is the maintainer's.

## Consequences

* Two new tools for a developer who runs the targets: `cargo-fuzz` 0.13.1 and the nightly toolchain above. Neither is needed to build, test or run `qm`. `scripts/check.sh all` does not run the targets; `scripts/check.sh fuzz` does.
* One new library, in the fuzz package only: `libfuzzer-sys` 0.4.13, published 2026-06-04, with `arbitrary` 1.5.0 and `jobserver` 0.1.35. `libfuzzer-sys` has a build script of 46 lines, read on 2026-11-21: it compiles the C++ sources of libFuzzer that the package carries, with the `cc` crate, or links a library that the variable `CUSTOM_LIBFUZZER_PATH` names. It runs no other command and fetches nothing. The other two have no build script and no procedural macro. The C++ code is native code in the fuzz binaries; it is in no build of `qm`, and no outside data reaches it outside a fuzz run.
* The license of `libfuzzer-sys` adds the NCSA license to the two of the program. `cargo deny` does not read the fuzz package, so `deny.toml` did not change; `cargo audit` reads both lockfiles.
* The first runs and their review found six things, all corrected in the change that added the targets. The first three came from the fuzzer in the first minute of the targets `classify`, `mime` and `redact`; the fourth from a reviewer who wrote an input by hand that the fuzzer had not reached; the last two are properties that were not true:
  * `dkim_passed` gave a pass for an empty domain when a signature's domain ended with a dot or was empty. No configured sender has such an address, so no stored order is tagged wrongly.
  * The check of a short text did not see the digits of a configured phone number inside a longer run of digits. A `Message-ID` with such a run would have been stored and printed as the message key.
  * The same check read its text as markup only, so a term between `<` and `>` was not seen.
  * The same check read its text only after the percent pairs were decoded. A `%` before a term that starts with two hexadecimal digits took them away as a pair, so a `Message-ID` with `%` before the phone number would have been stored as the key. The targets `redact` and `mime` fail on such an input; the fuzzer had not made one in about nine minutes of each of the two targets.
  * The target of the JSON reader asserted more than is true: the reader skips a field that it does not use without a look at its bytes, so it accepts a body that a stricter reader refuses. The property was corrected, not the reader.
  * The target of the size reader asserted that a change of case changes nothing. For a few letters outside ASCII a change of case is a change of the letter. The property is about ASCII letters now.
* The check of a short text is wider now, and it also decides whether a link keeps its address, so the redaction version is 8 ([ADR 0004](/decisions/0004-redaction-before-storage.md)).
* A page of the message list with an id that is no id is refused whole. Before, the messages before that id were fetched first, and with the limit of a run reached the id was never read. The error is the same.
* The Gmail adapter reads the body of an answer into memory before it reads the JSON, so that the two readers take bytes and a target can call them. The text of a message is borrowed from that body, so the memory for one message is what it was, except for an answer that writes the text with JSON escapes, which is copied once more.
* A change of `Cargo.lock` makes the `targets` group fail until `fuzz/Cargo.lock` follows: copy the lockfile and build the targets once.

## Alternatives

* Property tests with `proptest` only, on the stable toolchain. Rejected: the rule names `cargo fuzz`, and a generator of valid-looking inputs does not find what a coverage-guided fuzzer finds in a parser of bytes; three of the defects above came in the first minute of three targets.
* Record an exception for the fuzz job. Rejected: REAS admits it only for a tool whose maintainer is the only source of outside data, and mail is not that.
* A target that only looks for a stop of the program. Rejected by the rule itself; the three defects that the first runs found were wrong results, not stops.
* Real receipts as a corpus. Rejected: real receipts never enter git, and a corpus in a cache of CI is outside the machine.
* A floating `nightly`. Rejected: a compiler of another day can fail the build on a day when nothing changed.
* The fuzz package in the package of the program. Rejected: the program would then depend on the fuzzing library, and the layers check and the lint table are for one plain package.

## What the targets do not cover

* One architecture: the workflow runs on x86_64 Linux. The image of `qm` can be built for another one; no target runs there.
* The answers of the token endpoint, which the Gmail adapter also reads as JSON, have no target: their two fields go into a secret and are not parsed further.
* The path through the TLS library is not in any target: a target calls the readers with bytes and opens no connection.
* The inputs that a run finds are kept in the cache of CI, which GitHub removes after a week without use, and are not minimized on a schedule.
* The properties are the ones in the table. A reader can be wrong in a way that no property names: no target asserts a limit on memory or time, a duplicate of a message inside an archive would pass, and a redactor that refuses a text with a digit in it would pass.
* A term inside a tag, such as a name in an attribute, is removed from a body by the sequence pass only; the assertion of the redactor reads the text without its tags. With that one pass taken out, a name in a `style` attribute came back in the result. This is older than the targets, and the `redact` target holds it now.
* Nothing tests the fuzz workflow before it runs for the first time, on the pull request of this change.
* Dependabot does not read the fuzz package: its lockfile follows the lockfile of the program by hand, and `libfuzzer-sys` moves by hand.

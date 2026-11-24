---
type: Technical Reference
title: Unsafe and native code in the dependencies
description: The count of unsafe items for each package of the build, the dependencies that outside data reaches and that hold unsafe code, and what Miri cannot run in each.
tags: [security, dependencies, native-code, unsafe]
generated: { by: "process:claude-code", at: "2026-11-22T02:40:00Z" }
updated: 2026-11-24
sources:
  - id: baseline
    resource: ../../unsafe-baseline.tsv
    title: The count of unsafe items for each package
  - id: count
    resource: ../../scripts/unsafe_count.py
    title: The script that writes the baseline and compares a count with it
  - id: paths
    resource: ../../scripts/outside_data_paths.py
    title: The script that writes the table of this page
  - id: reas-native
    resource: ../../standards/reas/standard/native-code.md
    title: REAS-NATIVE-001 to REAS-NATIVE-004
---

# Unsafe and native code in the dependencies

`qm` has no unsafe code of its own: `unsafe_code = "forbid"` in `Cargo.toml` makes the compiler refuse it. Its dependencies have some, and a mail passes through many of them. This page records how much, where, and what was and was not done about it (REAS-NATIVE-001, REAS-NATIVE-002, REAS-SEC-001).

## The count and its baseline

`unsafe-baseline.tsv` holds one line for each package that the counter names: the name, the version, the unsafe items in the files that the build uses, the unsafe items in the other files of the package, and whether the package forbids unsafe code with an attribute in its source. An item is a function, an expression, an implementation, a trait or a method that the counter marks as unsafe. The counter is `cargo geiger` 0.13.0.

On 2026-11-21, for Linux on x86_64 with Rust 1.100.0: 237 packages, 123 of them with unsafe items in the files of the build, 32,191 such items. The 237 are the packages that the counter names, which is not the same set as the packages that its build compiles: the build compiled 226, the baseline has 18 that it did not compile, such as the libraries of the tests, and it does not have 7 that the build compiled, each a dependency of a build script.

`scripts/check.sh unsafe` runs the counter and fails when a line of its result is not the line of the baseline. CI runs it in the `rust` job on each pull request. The counter removes the build directory that it works in, so the group gives it one of its own, `target/unsafe-count`.

* A count that is larger, a new package with unsafe items, or a package that no longer forbids unsafe code is a growth. A growth needs a reason in [the log](/log.md) and a review. Then write the baseline again: `scripts/check.sh unsafe write`.
* Any other difference, such as a count that is smaller or a package that is gone, needs a new baseline only. The file is the record of the current count, so the check asks for the same lines.
* A change of `Cargo.lock` changes versions, so it changes the baseline. The pull request shows the old and the new count of each package side by side.

What the count does not say:

* The counter counts. It does not review, and a large count is not a defect.
* It does not read each file that the build reads. On 2026-11-21 the build used 232 files that the counter did not scan. 148 of them are the data files of two Unicode libraries, `icu_properties_data` and `icu_normalizer_data`: Rust source that a macro includes, and each of the 148 holds the word `unsafe`. The baseline says 0 for both packages, and both are on a path of outside data, below `url`. So the count is too low by at least these two.
* It does not read the code that a build script writes.
* The line of `qrtrmastr` says "no" for "forbids unsafe" and 4 items in other files. The counter looks for an attribute in the source; this project forbids unsafe code in `Cargo.toml`. The 4 items are the planted violation of `examples/lint_canary.rs`, which the compiler refuses.
* It counts for one platform. A build for another one uses other files.
* The fuzz package is not counted. It forbids unsafe code of its own, and its one library with native code, `libfuzzer-sys`, is in no build of `qm` ([ADR 0017](/decisions/0017-fuzz-targets-with-cargo-fuzz-and-a-budget-for-each-stage.md)).

## The dependencies that outside data reaches

`python3 scripts/outside_data_paths.py` writes the table below from the dependency graph, the baseline and the source of each package. Run it when the baseline changes, and put its table here. No check compares this page with the baseline.

How a package gets into the table:

* A reader is a direct dependency that takes bytes or text of another party: a mail, an archive, an answer of the Gmail API, or a stored message or order that is read back. The script names 23 readers and what each one takes. Four of them take the data second hand: the error of an import, a log line of a library that reads a mail, and a token of an answer of Google.
* A package is on a path of outside data when a reader is that package or depends on it, over any number of steps. A build dependency and a procedural macro run on the build machine and not on a mail, so they are not followed.
* The table has the package when the baseline counts an unsafe item in a file of its build.

This is the dependency graph, not a trace of the data. It names a package when a reader can call it, also when no byte of a mail gets there. It is the larger of the two sets, on purpose.

On 2026-11-21: 191 packages are on a path of outside data in the graph. The counter names 166 of them; the other 25 belong to a feature that is off and are in no build. 107 hold unsafe code in a file of the build, with 31,594 unsafe items together. The two Unicode data packages above are not among the 107.

The last column is what Miri cannot run (REAS-NATIVE-004). Miri runs Rust code. It does not run a call into a foreign function, an intrinsic of one processor, or assembly. The script looks for the text of each in the Rust files of the package, outside its tests, and counts the C and assembly files that the package carries. A mark says that the text is there. It does not say that the build uses it or that a mail reaches it. The text `extern "C"` is also in a Rust function that another language can call, and the script does not find `naked_asm!` or a name that a `use` with braces brings in. 32 packages have a mark.

| Package | Version | Unsafe items | Reached from | What Miri cannot run |
| --- | --- | --- | --- | --- |
| `aho-corasick` | 1.1.5 | 1097 | `regex`, `tracing-subscriber` | processor intrinsic (2 files) |
| `allocator-api2` | 0.2.21 | 862 | `jiff-sqlx`, `sqlx` | foreign function (2 files) |
| `anyhow` | 1.0.104 | 380 | `anyhow` | — |
| `arrayvec` | 0.7.8 | 353 | `jiff-sqlx`, `rust_decimal`, `sqlx` | — |
| `base64` | 0.23.1 | 295 | `base64`, `ureq` | processor intrinsic (1 files) |
| `block-buffer` | 0.12.1 | 76 | `jiff-sqlx`, `sha2`, `sqlx` | — |
| `byteorder` | 1.5.0 | 180 | `jiff-sqlx`, `sqlx` | — |
| `bytes` | 1.12.1 | 849 | `jiff-sqlx`, `rust_decimal`, `sqlx`, `tokio`, `ureq` | — |
| `chacha20` | 0.10.2 | 611 | `jiff-sqlx`, `sqlx` | processor intrinsic (3 files) |
| `cmov` | 0.5.4 | 40 | `jiff-sqlx`, `sha2`, `sqlx` | processor intrinsic (3 files); assembly (3 files) |
| `const-oid` | 0.10.3 | 5 | `jiff-sqlx`, `sha2`, `sqlx` | — |
| `constant_time_eq` | 0.6.1 | 49 | `zip` | processor intrinsic (4 files); assembly (4 files) |
| `crc32fast` | 1.5.5 | 321 | `zip` | processor intrinsic (2 files) |
| `crossbeam-queue` | 0.3.14 | 310 | `jiff-sqlx`, `sqlx` | — |
| `crossbeam-utils` | 0.8.23 | 98 | `jiff-sqlx`, `sqlx` | — |
| `cssparser` | 0.38.0 | 58 | `scraper` | — |
| `dtoa` | 1.0.11 | 171 | `scraper` | — |
| `dtoa-short` | 0.3.5 | 7 | `scraper` | — |
| `ego-tree` | 0.11.1 | 153 | `ego-tree`, `scraper` | — |
| `either` | 1.19.0 | 2 | `jiff-sqlx`, `sqlx` | — |
| `encodify` | 1.0.2 | 1387 | `mail-parser` | processor intrinsic (6 files) |
| `event-listener` | 5.4.2 | 55 | `jiff-sqlx`, `sqlx` | assembly (1 files) |
| `flate2` | 1.1.11 | 69 | `zip` | foreign function (1 files) |
| `foldhash` | 0.2.0 | 205 | `jiff-sqlx`, `sqlx` | — |
| `form_urlencoded` | 1.2.3 | 2 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `futures-channel` | 0.3.34 | 92 | `jiff-sqlx`, `sqlx` | — |
| `futures-core` | 0.3.34 | 38 | `jiff-sqlx`, `sqlx` | — |
| `futures-intrusive` | 0.5.0 | 922 | `jiff-sqlx`, `sqlx` | — |
| `futures-sink` | 0.3.34 | 2 | `jiff-sqlx`, `sqlx` | — |
| `futures-task` | 0.3.34 | 112 | `jiff-sqlx`, `sqlx` | — |
| `futures-util` | 0.3.34 | 564 | `jiff-sqlx`, `sqlx` | — |
| `getrandom` | 0.2.17 | 74 | `jiff-sqlx`, `rust_decimal`, `sqlx`, `ureq` | foreign function (9 files); processor intrinsic (1 files) |
| `getrandom` | 0.4.3 | 67 | `jiff-sqlx`, `sqlx`, `zip` | foreign function (14 files); processor intrinsic (3 files); assembly (2 files) |
| `hashbrown` | 0.16.1 | 1492 | `jiff-sqlx`, `sqlx` | processor intrinsic (3 files) |
| `hashbrown` | 0.17.1 | 1449 | `jiff-sqlx`, `sqlx`, `ureq`, `zip` | processor intrinsic (3 files) |
| `hashlink` | 0.11.1 | 783 | `jiff-sqlx`, `sqlx` | — |
| `html5ever` | 0.40.1 | 152 | `scraper` | processor intrinsic (1 files) |
| `http` | 1.5.0 | 181 | `ureq` | — |
| `httparse` | 1.10.1 | 129 | `ureq` | processor intrinsic (3 files) |
| `hybrid-array` | 0.4.15 | 136 | `jiff-sqlx`, `sha2`, `sqlx` | — |
| `icu_collections` | 2.4.0 | 38 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `icu_locale_core` | 2.4.0 | 11 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `icu_normalizer` | 2.4.0 | 23 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `icu_provider` | 2.4.0 | 45 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `idna` | 1.1.0 | 31 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `indexmap` | 2.14.2 | 80 | `jiff-sqlx`, `sqlx`, `ureq`, `zip` | — |
| `itoa` | 1.0.19 | 149 | `jiff-sqlx`, `rust_decimal`, `scraper`, `serde_json`, `sqlx`, `ureq` | — |
| `jiff` | 1.0.1 | 172 | `jiff`, `jiff-sqlx` | foreign function (2 files) |
| `jiff-core` | 1.0.1 | 6 | `jiff`, `jiff-sqlx` | — |
| `lazy_static` | 1.5.1 | 8 | `tracing-subscriber` | — |
| `libc` | 0.2.191 | 99 | `jiff-sqlx`, `rust_decimal`, `scraper`, `sqlx`, `tokio`, `ureq`, `zip` | foreign function (126 files) |
| `lock_api` | 0.4.14 | 739 | `jiff-sqlx`, `scraper`, `sqlx` | — |
| `log` | 0.4.35 | 21 | `jiff`, `jiff-sqlx`, `scraper`, `sqlx`, `tracing`, `tracing-subscriber`, `ureq` | — |
| `mail-parser` | 1.0.1 | 452 | `mail-parser` | processor intrinsic (2 files) |
| `markup5ever` | 0.40.0 | 3 | `scraper` | — |
| `matchers` | 0.2.0 | 3 | `tracing-subscriber` | — |
| `memchr` | 2.8.3 | 2138 | `jiff-sqlx`, `mail-parser`, `regex`, `rust_decimal`, `scraper`, `serde_json`, `sqlx`, `tracing-subscriber`, `ureq`, `zip` | processor intrinsic (9 files) |
| `mio` | 1.2.5 | 181 | `jiff-sqlx`, `sqlx`, `tokio` | — |
| `once_cell` | 1.21.4 | 89 | `jiff-sqlx`, `sqlx`, `tracing`, `tracing-subscriber`, `ureq` | — |
| `parking_lot` | 0.12.5 | 322 | `jiff-sqlx`, `scraper`, `sqlx` | processor intrinsic (1 files); assembly (1 files) |
| `parking_lot_core` | 0.9.12 | 936 | `jiff-sqlx`, `scraper`, `sqlx` | foreign function (2 files) |
| `percent-encoding` | 2.3.2 | 8 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `pin-project-lite` | 0.2.17 | 13 | `jiff-sqlx`, `sqlx`, `tokio`, `tracing`, `tracing-subscriber` | — |
| `potential_utf` | 0.1.6 | 20 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `rand` | 0.10.3 | 35 | `jiff-sqlx`, `sqlx` | processor intrinsic (2 files) |
| `regex-automata` | 0.4.19 | 657 | `regex`, `tracing-subscriber` | — |
| `ring` | 0.17.14 | 448 | `jiff-sqlx`, `sqlx`, `ureq` | foreign function (12 files); processor intrinsic (1 files); C or assembly sources (107 files) |
| `rustls-pki-types` | 1.16.0 | 2 | `jiff-sqlx`, `sqlx`, `ureq` | — |
| `scopeguard` | 1.2.0 | 17 | `jiff-sqlx`, `scraper`, `sqlx` | — |
| `selectors` | 0.41.0 | 19 | `scraper` | — |
| `serde_core` | 1.0.229 | 5 | `jiff`, `jiff-sqlx`, `rust_decimal`, `scraper`, `serde`, `serde_json`, `sqlx`, `tracing-subscriber`, `ureq`, `url` | — |
| `serde_json` | 1.0.154 | 81 | `jiff-sqlx`, `rust_decimal`, `serde_json`, `sqlx`, `ureq` | — |
| `servo_arc` | 0.5.0 | 238 | `scraper` | foreign function (1 files) |
| `sha2` | 0.11.1 | 27 | `jiff-sqlx`, `sha2`, `sqlx` | processor intrinsic (12 files); assembly (4 files) |
| `sharded-slab` | 0.1.7 | 165 | `tracing-subscriber` | — |
| `simdutf8` | 0.1.5 | 401 | `mail-parser` | processor intrinsic (4 files) |
| `siphasher` | 1.0.4 | 46 | `scraper` | — |
| `slab` | 0.4.12 | 32 | `jiff-sqlx`, `sqlx` | — |
| `smallvec` | 1.16.5 | 634 | `jiff-sqlx`, `scraper`, `sqlx`, `tracing-subscriber`, `ureq`, `url` | — |
| `socket2` | 0.6.5 | 674 | `jiff-sqlx`, `sqlx`, `tokio` | — |
| `stable_deref_trait` | 1.2.1 | 25 | `jiff-sqlx`, `scraper`, `sqlx`, `ureq`, `url` | — |
| `string_cache` | 0.11.0 | 82 | `scraper` | — |
| `subtle` | 2.6.1 | 8 | `jiff-sqlx`, `sqlx`, `ureq` | — |
| `tendril` | 0.5.1 | 865 | `scraper` | — |
| `thread_local` | 1.1.10 | 61 | `tracing-subscriber` | — |
| `tinystr` | 0.8.5 | 62 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `tokio` | 1.54.1 | 2575 | `jiff-sqlx`, `sqlx`, `tokio` | foreign function (3 files) |
| `tokio-stream` | 0.1.19 | 4 | `jiff-sqlx`, `sqlx` | — |
| `tracing` | 0.1.44 | 18 | `jiff-sqlx`, `sqlx`, `tracing`, `tracing-subscriber` | — |
| `tracing-core` | 0.1.36 | 80 | `jiff-sqlx`, `sqlx`, `tracing`, `tracing-subscriber` | — |
| `tracing-log` | 0.2.0 | 36 | `tracing-subscriber` | — |
| `tracing-subscriber` | 0.3.23 | 149 | `tracing-subscriber` | — |
| `typed-path` | 0.13.0 | 112 | `zip` | — |
| `unicode-bidi` | 0.3.18 | 5 | `jiff-sqlx`, `sqlx` | — |
| `unicode-normalization` | 0.1.26 | 25 | `jiff-sqlx`, `sqlx`, `unicode-normalization` | — |
| `url` | 2.5.9 | 4 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `utf8-zero` | 0.8.1 | 3 | `ureq` | — |
| `utf8_iter` | 1.1.0 | 47 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `utf_types` | 1.0.0 | 43 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `whoami` | 2.1.3 | 58 | `jiff-sqlx`, `sqlx` | foreign function (1 files) |
| `yoke` | 0.8.4 | 150 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `zeroize` | 1.9.1 | 28 | `jiff-sqlx`, `secrecy`, `sqlx`, `ureq`, `zip` | processor intrinsic (3 files); assembly (1 files) |
| `zerotrie` | 0.2.5 | 25 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `zerovec` | 0.11.9 | 541 | `jiff-sqlx`, `sqlx`, `ureq`, `url` | — |
| `zip` | 9.7.0 | 131 | `zip` | — |
| `zlib-rs` | 0.6.8 | 2932 | `zip` | foreign function (3 files); processor intrinsic (13 files); assembly (2 files) |
| `zmij` | 1.0.23 | 629 | `jiff-sqlx`, `rust_decimal`, `serde_json`, `sqlx`, `ureq` | processor intrinsic (2 files); assembly (1 files) |

## What was reviewed, and what was not

* No unsafe code of a dependency was reviewed. REAS-SEC-001 asks for a review with `cargo vet` or a recorded manual review before the first release that uses the dependency. The project has published no release. The list above is what such a review starts from.
* REAS-NATIVE-004 asks that a path that Miri cannot run is covered by a build with a sanitizer. `cargo fuzz` builds the seven fuzz targets with the address sanitizer, so the Rust code of the libraries that the readers call runs under it in each fuzz run, on x86_64 Linux ([ADR 0017](/decisions/0017-fuzz-targets-with-cargo-fuzz-and-a-budget-for-each-stage.md)). No record says which of the 32 packages with a mark a target reaches. No target opens a connection, so none reaches `ring`.
* `ring` is the one package of the table that carries C and assembly sources. Each byte from the Gmail API passes it, and the database connection when it uses TLS.
* The [self-audit](/self-audit.md) holds these as gaps.

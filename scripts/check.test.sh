#!/usr/bin/env bash
# Test of scripts/check.sh: its usage, and the `lists` and `layers` groups with planted
# violations (REAS-DOC-006). Each case of `lists` copies the two workflows and the Stop hook,
# plants one violation in a copy, and asserts that `lists` fails and names it. Each case of
# `layers` does the same with a copy of the two inner layers of `src/`. The files of the
# repository do not change.
set -u
cd "$(dirname "$0")/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failed=0
cases=0
mkdir "$work/wf" "$work/hook"
ci="$work/wf/ci.yml"
scan="$work/wf/scan.yml"
fuzzing="$work/wf/fuzz.yml"
hook="$work/hook/stop-check.sh"

fresh() {
  rm -f "$work"/wf/* "$work"/hook/*
  cp .github/workflows/ci.yml "$ci"
  cp .github/workflows/scan.yml "$scan"
  cp .github/workflows/fuzz.yml "$fuzzing"
  cp .claude/hooks/stop-check.sh "$hook"
}

# expect STATUS TEXT COMMAND...: the command exits with STATUS and prints TEXT.
expect() {
  local status=$1 text=$2 output actual
  shift 2
  cases=$((cases + 1))
  output=$("$@" 2>&1)
  actual=$?
  if [ "$actual" != "$status" ] || ! printf '%s' "$output" | grep -qF -- "$text"; then
    echo "FAIL: case $cases: $* gave status $actual, expected $status and the text: $text" >&2
    printf '%s\n' "$output" | tail -5 >&2
    failed=1
  fi
}
lists() { scripts/check.sh lists "$ci" "$hook" "$scan" "$fuzzing"; }

# Usage.
expect 2 "usage: scripts/check.sh {rust|layers|planted|toolchain|compile|database|sqlx|deny|audit|docs|privacy|rules|hooks|targets|fuzz|lists|all}" \
  scripts/check.sh no-such-group
expect 2 "needs DATABASE_URL" env -u DATABASE_URL scripts/check.sh database
expect 2 "needs DATABASE_URL" env -u DATABASE_URL scripts/check.sh sqlx
expect 1 "QM_CANARY_TERMS names a file that cannot be read" env QM_CANARY_TERMS="$work/absent.txt" scripts/check.sh privacy
# Each group that the head of the script describes is a group that it runs, and the reverse.
described=$(sed -n 's|^#   scripts/check\.sh \([a-z]*\) .*|\1|p' scripts/check.sh | tr '\n' ' ')
cases=$((cases + 1))
if [ "$described" != "rust layers planted toolchain compile database sqlx deny audit docs privacy rules hooks targets fuzz lists all " ]; then
  echo "FAIL: the head of scripts/check.sh describes: $described" >&2
  failed=1
fi
# The toolchain group: the file and rustc agree here; a workflow that installed another
# version fails, and one that installed the pinned version for a target passes.
# A stand-in for rustc gives the version, so that this test needs no toolchain: the job that
# runs it in CI installs none.
pinned=$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)
mkdir "$work/bin" "$work/other"
printf '#!/bin/sh\necho "rustc %s (stand-in)"\n' "$pinned" > "$work/bin/rustc"
printf '#!/bin/sh\necho "rustc 1.0.0 (stand-in)"\n' > "$work/other/rustc"
chmod +x "$work/bin/rustc" "$work/other/rustc"
expect 0 "PASS: toolchain" env PATH="$work/bin:$PATH" scripts/check.sh toolchain
expect 0 "PASS: toolchain" env PATH="$work/bin:$PATH" INSTALLED="$pinned-x86_64-unknown-linux-gnu" scripts/check.sh toolchain
for other in stable "" "${pinned}0" "1.0.0"; do
  expect 1 "the workflow installed another toolchain" env PATH="$work/bin:$PATH" INSTALLED="$other" scripts/check.sh toolchain
done
expect 1 "do not name the same toolchain" env PATH="$work/other:$PATH" scripts/check.sh toolchain

# The files of the repository pass, and so do their copies.
expect 0 "PASS: lists" scripts/check.sh lists
fresh
expect 0 "PASS: lists" lists

# A step of a workflow that runs a command of the script without the script.
for planted in \
  '      - run: cargo test --locked' \
  '        run: python scripts/check_docs.py' \
  '      - run: scripts/pii_canary.sh' \
  '      - run: bash scripts/pii_canary.test.sh' \
  '      - run: cargo +nightly-2026-11-15 fuzz run mime -- -max_total_time=30' \
  '        run: cargo fuzz run takeout' \
  '      - run: python3 scripts/fuzz_seeds.py --check' \
  '        run: cargo deny check' \
  '          cargo clippy --all-targets' \
  '          active=$(rustc --version | cut -d" " -f2)' \
  '      - run: bash scripts/check.test.sh'; do
  for target in "$ci" "$scan"; do
    fresh
    printf '%s\n' "$planted" >> "$target"
    expect 1 "names a check of its own" lists
  done
done
# A comment can name a command.
fresh
printf '%s\n' '      # cargo test runs through scripts/check.sh' >> "$ci"
expect 0 "PASS: lists" lists

# A group that the workflow no longer calls: the line removed, made a comment, or made a text.
for group in toolchain rust sqlx database audit docs privacy hooks lists; do
  fresh
  grep -v "run: scripts/check\.sh $group\$" .github/workflows/ci.yml > "$ci"
  expect 1 "does not run scripts/check.sh $group in one step" lists
  fresh
  sed "s|^\( *\)run: scripts/check\.sh $group\$|\1# run: scripts/check.sh $group|" .github/workflows/ci.yml > "$ci"
  expect 1 "does not run scripts/check.sh $group in one step" lists
  fresh
  sed "s|^\( *\)run: scripts/check\.sh $group\$|\1run: scripts/check.sh $group \|\| true|" .github/workflows/ci.yml > "$ci"
  expect 1 "does not run scripts/check.sh $group in one step" lists
done
# A group in a second step.
for group in rust lists; do
  fresh
  printf '      - run: scripts/check.sh %s\n' "$group" >> "$ci"
  expect 1 "does not run scripts/check.sh $group in one step" lists
done
# A line that makes a job or a step pass without its check.
for planted in \
  "    if: github.event_name == 'schedule'" \
  '        if: false' \
  '      - if: ${{ false }}' \
  '        continue-on-error: true' \
  '    continue-on-error: true' \
  '        shell: echo {0}' \
  '    needs: secrets' \
  '    needs: [rust, docs]' \
  "    if: github.event_name == 'pull_request'"; do
  for target in "$ci" "$scan"; do
    fresh
    printf '%s\n' "$planted" >> "$target"
    expect 1 "can make a job pass without its check" lists
  done
done
# The workflow of pull requests without one of its triggers.
for trigger in '  pull_request:' '  push:'; do
  fresh
  grep -vxF "$trigger" .github/workflows/ci.yml > "$ci"
  expect 1 "does not run on" lists
done
# The deny job without its action, with the action in a comment only, and with another command.
for target in ci scan; do
  fresh
  grep -v 'EmbarkStudios/cargo-deny-action@' ".github/workflows/$target.yml" > "$work/wf/$target.yml"
  expect 1 "does not run cargo deny check through its action" lists
  fresh
  sed 's|^\( *\)- uses: EmbarkStudios|\1# - uses: EmbarkStudios|' ".github/workflows/$target.yml" > "$work/wf/$target.yml"
  expect 1 "does not run cargo deny check through its action" lists
  fresh
  sed 's/^\( *\)command: check$/\1command: list/' ".github/workflows/$target.yml" > "$work/wf/$target.yml"
  expect 1 "does not run cargo deny check through its action" lists
done
# The daily workflow without its audit, and without its schedule.
fresh
grep -v 'run: scripts/check\.sh audit$' .github/workflows/scan.yml > "$scan"
expect 1 "does not run scripts/check.sh audit in one step" lists
fresh
grep -vxF '  schedule:' .github/workflows/scan.yml > "$scan"
expect 1 "does not run on a schedule" lists
# A third workflow, which no line of the script reads; also with a name that starts with a dot.
for name in other.yml .hidden.yml; do
  fresh
  cp "$scan" "$work/wf/$name"
  expect 1 "is a workflow that scripts/check.sh does not know" lists
  rm -f "$work/wf/$name"
done
# A job of the daily workflow with the name of a required check.
fresh
sed 's/^  daily-audit:$/  audit:/' .github/workflows/scan.yml > "$scan"
expect 1 "has no job named daily-audit" lists
fresh
sed 's/^  daily-deny:$/  daily-deny:\n    name: deny/' .github/workflows/scan.yml > "$scan"
expect 1 "has a job with the name of a required check" lists

# The Stop hook: a check of its own, and a group that it no longer calls.
fresh
printf '%s\n' 'cargo check --all-targets 1>&2 || failed=1' >> "$hook"
expect 1 "names a check of its own" lists
for group in compile docs privacy hooks lists; do
  fresh
  grep -v "scripts/check\.sh $group" .claude/hooks/stop-check.sh > "$hook"
  expect 1 "does not run scripts/check.sh $group" lists
  fresh
  sed "s|^\( *\)scripts/check\.sh $group 1>&2 .*|\1# scripts/check.sh $group 1>\&2 \|\| failed=1|" .claude/hooks/stop-check.sh > "$hook"
  expect 1 "does not run scripts/check.sh $group" lists
  fresh
  sed "s|^\( *\)scripts/check\.sh $group 1>&2 .*|\1scripts/check.sh $group 1>\&2 \|\| true|" .claude/hooks/stop-check.sh > "$hook"
  expect 1 "does not run scripts/check.sh $group" lists
done
# A file that is not there.
fresh
rm "$ci"
expect 1 "cannot read" lists

# The layers group. The two inner layers of the repository pass, and so do their copies.
expect 0 "PASS: layers" scripts/check.sh layers
inner="$work/inner"
manifest="$work/Cargo.toml"
layers() { scripts/check.sh layers "$inner/domain" "$inner/application" "$manifest"; }
fresh_layers() {
  rm -rf "$inner"
  mkdir "$inner"
  cp -r src/domain "$inner/domain"
  cp -r src/application "$inner/application"
  cp Cargo.toml "$manifest"
}
# plant LAYER LINE: one more line of code at the end of a file of that layer.
plant() { fresh_layers; printf '%s\n' "$2" >> "$inner/$1/mod.rs"; }
refused() { expect 1 "$inner/$1/mod.rs:" layers; }
fresh_layers
expect 0 "PASS: layers" layers

# Each dependency of the manifest, in each layer: refused, unless it is one that this test
# lists for the layer. The lists are written here a second time on purpose. A name that is
# added to a list of the script fails this test until somebody adds it here too.
domain_may_use=" hex jiff regex rust_decimal thiserror unicode_normalization proptest rstest "
application_may_use=" jiff thiserror tokio proptest rstest "
dependencies=$(awk '/^\[/{on=($0=="[dependencies]"||$0=="[dev-dependencies]")} on&&/^[A-Za-z0-9_-]+ *=/{sub(/ *=.*/,"");gsub(/-/,"_");print}' Cargo.toml)
# The floors are written here a second time too: a floor that is lowered fails this test.
for floor in DOMAIN_FLOOR=10 APPLICATION_FLOOR=8 DEPENDENCY_FLOOR=30; do
  cases=$((cases + 1))
  if ! grep -qx "$floor" scripts/check.sh; then
    echo "FAIL: scripts/check.sh does not hold the line $floor" >&2
    failed=1
  fi
done
cases=$((cases + 1))
if [ "$(printf '%s\n' "$dependencies" | wc -l)" -lt 30 ]; then
  echo "FAIL: the test read fewer than 30 dependencies from Cargo.toml" >&2
  failed=1
fi
for name in $dependencies; do
  for layer in domain application; do
    plant "$layer" "    let value = $name::make();"
    may_use=$domain_may_use
    if [ "$layer" = application ]; then may_use=$application_may_use; fi
    case "$may_use" in
      *" $name "*) expect 0 "PASS: layers" layers ;;
      *) refused "$layer" ;;
    esac
  done
done
# The forms in which a library is named. Each is one that the code had, or one that a change
# would write; serde and SQLx stand for every library that a layer may not use.
for planted in \
  'use serde::Serialize;' \
  'use serde;' \
  'use serde as wire;' \
  'use ::serde::Serialize;' \
  'use ::serde;' \
  'extern crate serde;' \
  'pub use sqlx;' \
  '#[derive(Debug, Clone, Serialize)]' \
  '#[derive(Deserialize, Debug)]' \
  '    #[serde(rename = "product_id")]' \
  '#![serde(default)]' \
  '    #[cfg_attr(feature = "json", serde(rename = "product_id"))]' \
  "impl<'de> serde::Deserialize<'de> for Platform {" \
  '    let value: serde_json::Value = serde_json::from_str(text)?;' \
  '    let text = ::serde_json::to_string(&value)?;' \
  '    rust_decimal::serde::str::serialize(&self.0, serializer)' \
  '    #[sqlx(rename = "id")]' \
  '#[derive(Debug, ::sqlx::FromRow)]' \
  '    let rows = sqlx::query("select 1").fetch_all(pool).await?;' \
  '    let stored: jiff_sqlx::Timestamp = at.to_sqlx();' \
  'use jiff_sqlx::ToSqlx;' \
  '    let page = (ureq::get(address).call()?);' \
  'use std::fs;' \
  '    let text = std::fs::read_to_string(path)?;' \
  '    let text = ::std::fs::read_to_string(path)?;' \
  'use std::{collections::HashMap, fs};' \
  'use std::{env, fmt};' \
  'use std::{collections::{BTreeMap, BTreeSet}, fs};' \
  'use std::{collections::{HashSet}, env};' \
  'use std::*;' \
  '    let peer = std::os::unix::net::UnixStream::connect(socket)?;' \
  '    let text = tokio::fs::read_to_string(path).await?;' \
  '    let stream = tokio::net::TcpStream::connect(address).await?;' \
  '    let child = tokio::process::Command::new("sh").spawn()?;' \
  '#[cfg_attr(all(), path = "../adapters/postgres/mod.rs")]' \
  '    let url = std::env::var("DATABASE_URL")?;' \
  '    let stream = std::net::TcpStream::connect(address)?;' \
  '    let status = std::process::Command::new("sh").status()?;' \
  '#[path = "../adapters/postgres/mod.rs"]' \
  '    #[ path = "rows.rs" ]' \
  'include!("rows.inc");'; do
  for layer in domain application; do
    plant "$layer" "$planted"
    expect 1 "names a library or a module that the layer may not use" layers
    refused "$layer"
  done
done
# A line that starts like a comment and closes a block comment is code after the close.
for layer in domain application; do
  fresh_layers
  printf '/*\n// */ use serde::Serialize;\n' >> "$inner/$layer/mod.rs"
  refused "$layer"
done
# What only the domain refuses: the runtime, input and output, threads, and a read of the
# clock, also when `now` is passed as a value.
for planted in \
  '    let handle = tokio::spawn(work);' \
  '    let line = std::io::stdin().lines().next();' \
  'use std::io;' \
  '    std::thread::sleep(pause);' \
  '    let started = std::time::Instant::now();' \
  'use std::time::SystemTime;' \
  '    let today = jiff::Zoned::now().date();' \
  '    let at = given.unwrap_or_else(jiff::Timestamp::now);' \
  '    let started = clock.now();' \
  '    let zone = jiff::tz::TimeZone::system();' \
  '    let there = std::path::Path::new(name).exists();' \
  '    println!("{value}");' \
  '    eprintln!("{value}");' \
  '    dbg!(&value);'; do
  plant domain "$planted"
  expect 1 "the domain names a library or a module that the layer may not use" layers
  plant application "$planted"
  expect 0 "PASS: layers" layers
done
# The controls: a line that looks like a finding and is none.
for harmless in \
  '/// The CLI adapter serializes the result with serde::Serialize.' \
  '    // sqlx::query is what the adapter calls.' \
  '    let pairs = names.iter().zip(sizes);' \
  '    let pairs = (1_usize..).zip(list);' \
  'pub struct Serializer;' \
  '    let known = self.now_known();' \
  '    let known_now = snow::flake();' \
  '    let path = crate::adapters::zip_name();' \
  '    let address = self.url;' \
  'use std::fmt;' \
  'use std::{collections::BTreeMap, iter};' \
  '    let text = include_str!("words.txt");' \
  '    let pathological = 1;' \
  '    let level = gzip::level();' \
  '    let count = my_url::count(&rows);' \
  '    let pairs = std::iter::zip(names, sizes);' \
  '    let printed = blueprint!(value);' \
  '// A comment with a star * and serde::Serialize in it.' \
  '    #[rstest::rstest]' \
  '    proptest::proptest!(|(raw in ".*")| { check(&raw)?; });'; do
  fresh_layers
  printf '%s\n' "$harmless" >> "$inner/domain/mod.rs"
  printf '%s\n' "$harmless" >> "$inner/application/mod.rs"
  expect 0 "PASS: layers" layers
done
# The channel of the runtime is what the application layer may use of it.
plant application '    let stream = tokio::sync::mpsc::channel(8);'
expect 0 "PASS: layers" layers
# A file that is no text hides nothing: a byte of zero before the finding.
fresh_layers
printf '// \0\nuse sqlx::Row;\n' >> "$inner/application/ports.rs"
expect 1 "$inner/application/ports.rs:" layers
# A file that cannot be read is a finding, not a file without findings. A superuser reads
# every file, so the case is left out for one.
if [ "$(id -u)" != 0 ]; then
  fresh_layers
  chmod 000 "$inner/domain/money.rs"
  expect 1 "the search failed" layers
  chmod 600 "$inner/domain/money.rs"
fi
# A symbolic link is refused, because the search does not follow it.
fresh_layers
printf 'use serde::Serialize;\n' > "$work/outside.rs"
ln -s "$work/outside.rs" "$inner/domain/extra.rs"
expect 1 "holds a symbolic link" layers
# The floors. A layer that is not there, one file fewer than the floor, and files without
# content are not a layer that passed; a layer with exactly its floor is.
fresh_layers
rm -rf "$inner/domain"
expect 1 "fewer than the floor" layers
for layer in domain application; do
  floor=$(sed -n "s/^$(printf '%s' "$layer" | tr a-z A-Z)_FLOOR=//p" scripts/check.sh)
  fresh_layers
  find "$inner/$layer" -name '*.rs' | sort | tail -n +"$((floor + 1))" | xargs -r rm
  expect 0 "PASS: layers" layers
  find "$inner/$layer" -name '*.rs' | sort | tail -n 1 | xargs rm
  expect 1 "$inner/$layer holds $((floor - 1)) Rust files with content, fewer than the floor of $floor" layers
  fresh_layers
  find "$inner/$layer" -name '*.rs' ! -name mod.rs -exec sh -c ': > "$1"' sh {} \;
  expect 1 "$inner/$layer holds 1 Rust files with content" layers
done
# The manifest: one that names too few dependencies, and a table that the check does not read.
fresh_layers
sed -i '/^serde/d; /^sqlx/d; /^clap/d; /^ureq/d; /^zip/d' "$manifest"
expect 1 "fewer than the floor" layers
for table in '[build-dependencies]' '[target.x86_64-unknown-linux-gnu.dependencies]' '[dependencies.serde]' '[dev-dependencies.newdep]'; do
  fresh_layers
  printf '\n%s\nversion = "1"\n' "$table" >> "$manifest"
  expect 1 "a dependency in a form that scripts/check.sh layers does not read" layers
done
# A new dependency in the plain form is refused in both layers; in a form that the reader of
# the manifest does not take as a name, the manifest itself is refused.
fresh_layers
sed -i 's/^\[dependencies\]$/[dependencies]\nnewdep = "1"/' "$manifest"
expect 0 "PASS: layers" layers
printf '%s\n' '    let value = newdep::make();' >> "$inner/domain/mod.rs"
refused domain
for key in 'newdep.version = "1"' 'newdep.workspace = true' '"newdep" = "1"' '  newdep = "1"'; do
  fresh_layers
  sed -i "s/^\[dependencies\]\$/[dependencies]\n$key/" "$manifest"
  expect 1 "a dependency in a form that scripts/check.sh layers does not read" layers
done
fresh_layers
sed -i '/^serde = /d' "$manifest"
sed -i '/^\[dependencies\]$/,/^\[dev-dependencies\]$/ { /^sqlx = /d }' "$manifest"
sed -i '/^clap = /d; /^ureq = /d' "$manifest"
expect 0 "PASS: layers" layers
sed -i '/^zip = /d' "$manifest"
expect 1 "names 29 dependencies, fewer than the floor of 30" layers
# The `rust` group, which CI runs, holds the two groups that have no step of their own.
for part in layers planted; do
  cases=$((cases + 1))
  if ! sed -n '/^rust() {$/,/^}$/p' scripts/check.sh | grep -qx "  $part"; then
    echo "FAIL: the rust group of scripts/check.sh does not run $part" >&2
    failed=1
  fi
done

# The fuzz workflow, which `lists` reads like the two others.
fresh
grep -v 'run: scripts/check.sh fuzz' .github/workflows/fuzz.yml > "$fuzzing"
expect 1 "does not run scripts/check.sh fuzz in one step" lists
fresh
printf '%s\n' '      - run: scripts/check.sh fuzz' >> "$fuzzing"
expect 1 "does not run scripts/check.sh fuzz in one step" lists
fresh
printf '%s\n' '      - run: cargo fuzz run mime' >> "$fuzzing"
expect 1 "names a check of its own" lists
for key in "    if: github.event_name == 'schedule'" '    continue-on-error: true' '        continue-on-error: true' '    needs: other'; do
  fresh
  printf '%s\n' "$key" >> "$fuzzing"
  expect 1 "can make a job pass without its check" lists
done
fresh
grep -v '^  pull_request:$' .github/workflows/fuzz.yml > "$fuzzing"
expect 1 "does not run on pull_request:" lists
fresh
grep -v '^  schedule:$' .github/workflows/fuzz.yml > "$fuzzing"
expect 1 "does not run on schedule:" lists
# The budget of each stage is in the file, and the toolchain is the one of the script.
fresh
grep -v '^ *QM_FUZZ_SECONDS:' .github/workflows/fuzz.yml > "$fuzzing"
expect 1 "does not set QM_FUZZ_SECONDS once" lists
for budget in '1' "\${{ github.event_name == 'pull_request' \&\& '1' || '1' }}" "\${{ github.event_name == 'pull_request' \&\& '30' || '300' }} # or less" "\${{ github.event_name == 'pull_request' \&\& '300' || '30' }}"; do
  fresh
  sed "s/^\( *\)QM_FUZZ_SECONDS: .*/\1QM_FUZZ_SECONDS: $budget/" .github/workflows/fuzz.yml > "$fuzzing"
  expect 1 "does not set QM_FUZZ_SECONDS once, to 30 on a pull request and 300 in the nightly run" lists
done
# A second line, in a step, that sets another budget.
fresh
printf '%s\n' '        env:' '          QM_FUZZ_SECONDS: 1' >> "$fuzzing"
expect 1 "does not set QM_FUZZ_SECONDS once" lists
# A filter on a trigger, a schedule that is not daily, and a token that can write.
for filter in "    paths: ['src/**']" "    branches: [main]" "    paths-ignore: ['**']" "    types: [labeled]"; do
  fresh
  sed "s#^  pull_request:\$#  pull_request:\n$filter#" .github/workflows/fuzz.yml > "$fuzzing"
  expect 1 "has a filter on a trigger" lists
done
for cron in '0 0 30 2 *' '41 6 * * 1' '41 6 1 * *'; do
  fresh
  sed "s/^    - cron: .*/    - cron: \"$cron\"/" .github/workflows/fuzz.yml > "$fuzzing"
  expect 1 "does not run once a day" lists
done
fresh
sed 's/^  contents: read$/  contents: write/' .github/workflows/fuzz.yml > "$fuzzing"
expect 1 "a token that can only read" lists
fresh
grep -v '^  contents: read$' .github/workflows/fuzz.yml > "$fuzzing"
expect 1 "a token that can only read" lists
fresh
sed 's/^  contents: read$/  contents: read\n  actions: write/' .github/workflows/fuzz.yml > "$fuzzing"
expect 1 "a token that can only read" lists
fresh
sed 's/tool: cargo-fuzz@.*/tool: cargo-fuzz/' .github/workflows/fuzz.yml > "$fuzzing"
expect 1 "does not install cargo-fuzz at a version" lists
# A trigger that gives the code of a pull request a token of the repository, in each workflow.
for target_file in "$ci" "$scan" "$fuzzing"; do
  fresh
  sed -i 's/^  workflow_dispatch:$/  workflow_dispatch:\n  pull_request_target:/' "$target_file"
  expect 1 "can make a job pass without its check" lists
done
fresh
sed 's/^permissions:$/permissions: write-all/; /^  contents: read$/d' .github/workflows/fuzz.yml > "$fuzzing"
expect 1 "a token that can only read" lists
for line in 'FUZZ_PR_SECONDS=30' 'FUZZ_NIGHTLY_SECONDS=300'; do
  cases=$((cases + 1))
  if ! grep -qxF "$line" scripts/check.sh; then
    echo "FAIL: scripts/check.sh does not hold the line $line" >&2
    failed=1
  fi
done
fresh
sed 's/toolchain: nightly-.*/toolchain: nightly/' .github/workflows/fuzz.yml > "$fuzzing"
expect 1 "does not install the toolchain" lists
fresh
sed 's/^  fuzz:$/  rust:/' .github/workflows/fuzz.yml > "$fuzzing"
expect 1 "has a job with the name of a required check" lists
fresh
rm "$fuzzing"
expect 1 "cannot read" lists

# The targets group. The fuzz package of the repository passes, and so does its copy.
expect 0 "PASS: targets" scripts/check.sh targets
package="$work/fuzz"
targets() { scripts/check.sh targets "$package"; }
fresh_targets() {
  rm -rf "$package"
  mkdir "$package"
  cp fuzz/Cargo.toml "$package/Cargo.toml"
  cp -r fuzz/fuzz_targets "$package/fuzz_targets"
  cp -r fuzz/seeds "$package/seeds"
}
fresh_targets
expect 0 "PASS: targets" targets
# A target in one of the four places only.
fresh_targets
printf '#![no_main]\n' > "$package/fuzz_targets/new_reader.rs"
expect 1 "the files of $package/fuzz_targets (>) differ" targets
fresh_targets
rm "$package/fuzz_targets/pack.rs"
expect 1 "the files of $package/fuzz_targets (>) differ" targets
fresh_targets
printf '\n[[bin]]\nname = "new_reader"\npath = "fuzz_targets/new_reader.rs"\n' >> "$package/Cargo.toml"
expect 1 "the [[bin]] entries of $package/Cargo.toml (>) differ" targets
fresh_targets
sed -i 's/^name = "mime"$/name = "mail"/' "$package/Cargo.toml"
expect 1 "the [[bin]] entries of $package/Cargo.toml (>) differ" targets
# A target without a seed, with a seed that is empty, and a seed for no target.
fresh_targets
rm -r "$package/seeds/takeout"
expect 1 "the targets with a seed in $package/seeds (>) differ" targets
fresh_targets
for seed in "$package"/seeds/gmail_json/*; do : > "$seed"; done
expect 1 "the targets with a seed in $package/seeds (>) differ" targets
fresh_targets
mkdir "$package/seeds/other"
printf 'x\n' > "$package/seeds/other/seed"
expect 1 "the targets with a seed in $package/seeds (>) differ" targets
fresh_targets
rm -rf "$package"
expect 1 "differ" targets
# The list is written here a second time, with its floor and the toolchain.
for line in 'FUZZ_TARGETS=(takeout mime classify redact receipt pack gmail_json)' 'FUZZ_FLOOR=7' 'FUZZ_TOOLCHAIN=nightly-2026-11-15'; do
  cases=$((cases + 1))
  if ! grep -qxF "$line" scripts/check.sh; then
    echo "FAIL: scripts/check.sh does not hold the line $line" >&2
    failed=1
  fi
done
# The fuzz group runs the targets group first, `lists` runs it too, and the audit reads both
# lockfiles.
for pair in "fuzz:  targets" "audit:  run cargo audit --file fuzz/Cargo.lock" "audit:  run cargo audit --file Cargo.lock"; do
  cases=$((cases + 1))
  if ! sed -n "/^${pair%%:*}() {\$/,/^}\$/p" scripts/check.sh | grep -qxF -- "${pair#*:}"; then
    echo "FAIL: the ${pair%%:*} group of scripts/check.sh does not hold the line:${pair#*:}" >&2
    failed=1
  fi
done
cases=$((cases + 1))
if ! sed -n '/^lists() {$/,/^}$/p' scripts/check.sh | grep -qxF '    targets || failed=1'; then
  echo "FAIL: the lists group of scripts/check.sh does not run the targets group" >&2
  failed=1
fi
# The fuzz group itself, with a stand-in for cargo that writes down what it is asked to run:
# no compiler is needed, and the job that runs this test installs none.
mkdir "$work/cargo"
cat > "$work/cargo/cargo" <<'STAND_IN'
#!/bin/sh
printf '%s\n' "$*" >> "$QM_STAND_IN_LOG"
case "$*" in
  *" fuzz --version") exit "${QM_STAND_IN_VERSION:-0}" ;;
  *" -vV") printf 'cargo 1.0.0 (stand-in)\nhost: %s\n' "${QM_STAND_IN_HOST-a-host-platform}"; exit "${QM_STAND_IN_VV:-0}" ;;
  *" fuzz build "*) exit "${QM_STAND_IN_BUILD:-0}" ;;
  *" fuzz run --target a-host-platform ${QM_STAND_IN_FAILS:-no-target} "*) exit 1 ;;
esac
exit 0
STAND_IN
chmod +x "$work/cargo/cargo"
asked="$work/asked.log"
with_stand_in() { env PATH="$work/cargo:$PATH" QM_STAND_IN_LOG="$asked" "$@" scripts/check.sh fuzz; }
# A budget that is no number is refused before a target runs. The stand-in is on the path
# here too: with the real tool, a budget that is no number would be no limit.
: > "$asked"
expect 2 "is not a number of seconds" with_stand_in env QM_FUZZ_SECONDS=soon
cases=$((cases + 1))
if grep -q ' fuzz run ' "$asked"; then
  echo "FAIL: case $cases: a target ran with a budget that is no number" >&2
  failed=1
fi
# Each of the seven targets is run, from the found inputs and from the seeds, for the budget.
: > "$asked"
expect 0 "PASS: 7 fuzz targets ran for 30 seconds each without a failure" with_stand_in env -u QM_FUZZ_SECONDS
for target in takeout mime classify redact receipt pack gmail_json; do
  cases=$((cases + 1))
  if ! grep -qxF -- "+nightly-2026-11-15 fuzz run --target a-host-platform $target fuzz/corpus/$target fuzz/seeds/$target -- -max_total_time=30 -rss_limit_mb=2048 -print_final_stats=1" "$asked"; then
    echo "FAIL: case $cases: the fuzz group did not run $target as expected; it asked for:" >&2
    cat "$asked" >&2
    failed=1
  fi
done
: > "$asked"
expect 0 "PASS: 7 fuzz targets ran for 7 seconds each" with_stand_in env QM_FUZZ_SECONDS=7
cases=$((cases + 1))
if [ "$(grep -c -- ' -max_total_time=7 ' "$asked")" != 7 ]; then
  echo "FAIL: case $cases: not seven runs with the budget of 7 seconds" >&2
  failed=1
fi
# The targets are built first, for the platform that cargo names; a build that fails is said to
# be no finding, and no target runs.
cases=$((cases + 1))
if ! grep -qxF -- "+nightly-2026-11-15 fuzz build --target a-host-platform" "$asked"; then
  echo "FAIL: case $cases: the fuzz group did not build the targets for the host platform" >&2
  failed=1
fi
: > "$asked"
expect 1 "the fuzz targets do not build; this is no finding of a target" with_stand_in env QM_STAND_IN_BUILD=1
cases=$((cases + 1))
if grep -q ' fuzz run ' "$asked"; then
  echo "FAIL: case $cases: a target ran although the build failed" >&2
  failed=1
fi
: > "$asked"
expect 1 "does not name its host platform" with_stand_in env QM_STAND_IN_HOST=
: > "$asked"
expect 1 "does not name its host platform" with_stand_in env QM_STAND_IN_HOST= QM_STAND_IN_VV=101
# Without the tool or the toolchain the group does not pass.
: > "$asked"
expect 2 "needs cargo-fuzz and the toolchain nightly-2026-11-15" with_stand_in env QM_STAND_IN_VERSION=1
cases=$((cases + 1))
if grep -q ' fuzz run ' "$asked"; then
  echo "FAIL: case $cases: a target ran although the tool is missing" >&2
  failed=1
fi
# A target that fails: the group fails, prints the input, and still runs the other targets.
mkdir -p fuzz/artifacts/mime
printf 'the input of a planted failure\n' > fuzz/artifacts/mime/crash-planted-by-the-test
: > "$asked"
expect 1 "FAIL: mime failed on the input fuzz/artifacts/mime/crash-planted-by-the-test, in base64:" with_stand_in env QM_STAND_IN_FAILS=mime
: > "$asked"
expect 1 "$(base64 -w 76 fuzz/artifacts/mime/crash-planted-by-the-test)" with_stand_in env QM_STAND_IN_FAILS=mime
cases=$((cases + 1))
if [ "$(grep -c ' fuzz run ' "$asked")" != 7 ]; then
  echo "FAIL: case $cases: the targets after the failed one did not run" >&2
  failed=1
fi
rm -f fuzz/artifacts/mime/crash-planted-by-the-test
: > "$asked"
expect 1 "A failure is a defect of the reader or a property that is not true" with_stand_in env QM_STAND_IN_FAILS=gmail_json

# The rules group: the Read deny rules of the settings and the rules that the privacy page
# lists. The two files of the repository agree, and so do their copies.
expect 0 "PASS: rules" scripts/check.sh rules
settings="$work/settings.json"
page="$work/privacy.md"
rules() { scripts/check.sh rules "$settings" "$page"; }
fresh_rules() {
  cp .claude/settings.json "$settings"
  cp docs/concepts/privacy.md "$page"
}
fresh_rules
expect 0 "PASS: rules" rules
# A rule that one of the two files does not hold.
one_rule=$(sed -n '/"deny"/,/\]/p' .claude/settings.json | grep -m1 -oE '"Read\(~/[^"]*\)"' | tr -d '"')
cases=$((cases + 1))
if [ -z "$one_rule" ]; then
  echo "FAIL: the settings hold no Read deny rule that starts at the home directory" >&2
  failed=1
fi
fresh_rules
grep -vF "\"$one_rule\"" .claude/settings.json > "$settings"
expect 1 "differ" rules
fresh_rules
grep -vF "\`$one_rule\`" docs/concepts/privacy.md > "$page"
expect 1 "differ" rules
fresh_rules
sed -i 's|^\( *\)"Read(\./\.env)",$|\1"Read(./.env)",\n\1"Read(~/another-place/**)",|' "$settings"
expect 1 "differ" rules
fresh_rules
printf '\nThe page also names `Read(~/another-place/**)`.\n' >> "$page"
expect 1 "differ" rules
# A rule with the home directory of one person, in both files. The path is put together from
# two pieces: the privacy canary reads this file.
for home in "//ho""me/somebody" "//Us""ers/somebody" "/ho""me/somebody" "//mnt/c/Us""ers/somebody" '$HO''ME' '${HO''ME}'; do
  fresh_rules
  sed -i "s|^\\( *\\)\"Read(\\./\\.env)\",\$|\\1\"Read(./.env)\",\\n\\1\"Read($home/qm-data/**)\",|" "$settings"
  printf '\nThe page also names `Read(%s/qm-data/**)`.\n' "$home" >> "$page"
  expect 1 "names a home directory; write it with ~/" rules
done
# A Read rule that is not in the deny list is no deny rule: the two lists still agree.
fresh_rules
sed -i 's|^\( *\)"allow": \[$|\1"allow": [\n\1  "Read(./docs/**)",|' "$settings"
expect 0 "PASS: rules" rules
# Settings that a session would not read as these rules: a second deny list that replaces the
# first, the list under another name, and a file that is no JSON.
fresh_rules
sed -i 's|^\( *\)"hooks": {$|\1"permissions2": {},\n\1"hooks": {|' "$settings"
expect 0 "PASS: rules" rules
fresh_rules
python3 - "$settings" <<'PY'
import sys
path = sys.argv[1]
text = open(path).read()
marker = '"deny": ['
assert marker in text
open(path, "w").write(text.replace(marker, '"deny": [], "deny_was": [', 1))
PY
expect 1 "fewer than the floor" rules
fresh_rules
sed -i 's|"permissions": {|"permission": {|' "$settings"
expect 1 "fewer than the floor" rules
fresh_rules
printf ',\n' >> "$settings"
expect 1 "fewer than the floor" rules
# One rule out of both files: the two lists agree, and the count does not.
fresh_rules
grep -vF "\"$one_rule\"" .claude/settings.json > "$settings"
grep -vF "\`$one_rule\`" docs/concepts/privacy.md > "$page"
expect 1 "fewer than the floor of 10" rules
cases=$((cases + 1))
if ! grep -qx 'RULE_FLOOR=10' scripts/check.sh; then
  echo "FAIL: scripts/check.sh does not hold the line RULE_FLOOR=10" >&2
  failed=1
fi
# The groups that CI and the Stop hook call run the parts that have no step of their own.
for pair in "privacy:  rules || status=1" "hooks:  run bash scripts/pii_canary.test.sh" "hooks:  run bash scripts/check.test.sh" "hooks:  run bash .claude/hooks/guard-bash.test.sh"; do
  cases=$((cases + 1))
  if ! sed -n "/^${pair%%:*}() {\$/,/^}\$/p" scripts/check.sh | grep -qxF -- "${pair#*:}"; then
    echo "FAIL: the ${pair%%:*} group of scripts/check.sh does not hold the line:${pair#*:}" >&2
    failed=1
  fi
done
# The floor: a deny list without its Read rules, and no settings at all.
fresh_rules
grep -v '"Read(' .claude/settings.json > "$settings"
expect 1 "fewer than the floor" rules
fresh_rules
rm "$settings"
expect 1 "fewer than the floor" rules

if [ "$failed" != 0 ]; then
  echo "FAIL: scripts/check.sh does not do what its test expects" >&2
  exit 1
fi
if [ "$cases" -lt 520 ]; then
  echo "FAIL: only $cases cases ran" >&2
  exit 1
fi
echo "PASS: scripts/check.sh, $cases cases"

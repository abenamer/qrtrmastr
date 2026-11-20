# REAS, vendored

A copy of the REAS engineering standard, laid out as its repository is, so a session can read the rules from disk instead of a link it cannot open, and so the standard's own links between its files hold.

| | |
|---|---|
| Source | <https://github.com/abenamer/reas> |
| Version | 1.6.0-draft |
| Commit | `9df4053affe592687db1e54b03493965f45bfe1c` (2026-11-20) |
| Branch | `claude/zen-newton-74jyn8`, the source's pull request #4. On the day of the copy that pull request was open and its five jobs had passed; the source's default branch was at 1.5.0-draft, commit `5fe1f535`. |
| Copied | 2026-11-20 |
| Copied here | `standard/` (the documents and their decision record), `examples/` (three small Cargo packages the documents refer to, and the fuzz package and seed corpus of the trust-boundary example), `LICENSE-DOCS` (CC BY 4.0, the documents) and `LICENSE-CODE` (MIT, the examples) |
| Not copied | The source's README, agent files, changelog, checker, check script, templates, tests and workflows; this file stands in for its README |
| Copy before this one | 1.4.0-draft, commit `8a2d64f1cecb6baf040b2bb5325cb0a454a987a6` (2026-10-23), copied 2026-11-08 |

Start at [standard/index.md](standard/index.md). The files are copied as they are. To update them, copy the same directories and files from the source at a newer commit and change this table. The example packages are not part of the project's Cargo build, and nothing in the project's CI compiles them or runs the fuzz target. Two tools read their lockfiles as data: the `dependency-review` job, and GitHub's dependency graph behind Dependabot alerts. When an advisory names a package in a vendored lockfile, copy the examples again from the source at a commit that fixed it; do not change a copied file here, and do not merge a Dependabot change to one. The project's own [self-audit](../../docs/self-audit.md) says which rules were checked and against which version.

The copied tree was compared with `git archive` of the commit on the day of the copy, and the two were equal. If the source's pull request merges with a different commit, or changes before it merges, copy again from the merged commit and change this table.

# REAS, vendored

A copy of the REAS engineering standard, laid out as its repository is, so a session can read the rules from disk instead of a link it cannot open, and so the standard's own links between its files hold.

| | |
|---|---|
| Source | <https://github.com/abenamer/reas> |
| Version | 1.9.0-draft |
| Commit | `bfb99b72b99ea1c021fd83ceb675a0afb3d7c2ff` (2026-11-22), on the source's default branch: the merge of its pull request #6 |
| Copied | 2026-11-22 |
| Copied here | `standard/` (the documents and their decision record), `examples/` (three small Cargo packages the documents refer to, and the fuzz package and seed corpus of the trust-boundary example), `LICENSE-DOCS` (CC BY 4.0, the documents) and `LICENSE-CODE` (MIT, the examples) |
| Not copied | The source's README, agent files, changelog, checker, check script, templates, tests and workflows; this file stands in for its README |
| Copies before this one | 1.4.0-draft, commit `8a2d64f1cecb6baf040b2bb5325cb0a454a987a6` (2026-10-23), copied 2026-11-08. 1.6.0-draft, commit `9df4053affe592687db1e54b03493965f45bfe1c`, copied 2026-11-20 from the branch of that pull request while it was open; the copy of 1.7.0-draft replaced it the same day, before it reached the project's `main`. 1.7.0-draft, commit `a550838797e1e13dd00bde238b932ac1a9990157` (2026-11-20), the merge of the source's pull request #4, copied 2026-11-20. |

Start at [standard/index.md](standard/index.md). The files are copied as they are. To update them, copy the same directories and files from the source at a newer commit and change this table. The example packages are not part of the project's Cargo build, and nothing in the project's CI compiles them or runs the fuzz target. Two tools read their lockfiles as data: the `dependency-review` job, and GitHub's dependency graph behind Dependabot alerts. When an advisory names a package in a vendored lockfile, copy the examples again from the source at a commit that fixed it; do not change a copied file here, and do not merge a Dependabot change to one. The project's own [self-audit](../../docs/self-audit.md) says which rules were checked and against which version.

The copied tree was compared with `git archive` of the commit on the day of the copy, and the two were equal.

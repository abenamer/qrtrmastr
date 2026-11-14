---
paths:
  - "migrations/**"
---

# Migrations

- Migrations only add. Never drop a table or a column that holds data, and never rewrite an existing migration file: SQLx embeds the directory at compile time and checks the applied ones by checksum.
- A new file is numbered after the last one (`0003_...sql`) with a snake_case name that says what it adds. Start it with a comment that says why, in the voice of the other migrations.
- A check constraint that lists values is named `<table>_<column>_check` and lists exactly the texts of the matching `text_enum!` in `src/domain/states.rs`; to admit a value, drop that constraint and add it again with the full list.
- Every index has a comment naming the query it serves (REAS-DATA-001). No index only because a query filters a column.
- Update [docs/reference/schema.md](../../docs/reference/schema.md): the table's row and a `sources` entry for the migration. Note the migration in the changelog and the log.

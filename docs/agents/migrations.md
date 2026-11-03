# Migrations

ADR 0003 and `CODING_STANDARDS.md` set the rules for migrations. This page covers running the generator and the hand edits Toasty 0.11 needs.

## Generate

1. Change the models in `domain`.
2. Run `mise exec -- cargo run --bin toasty -- migration generate --name <name>`. It writes the SQL, a snapshot of the schema and an entry in `history.toml` under `infrastructure/persistence/toasty/`.
3. Read the SQL and make the hand edits below.
4. Run the generator again. It must print "No migration needed".

The generator asks questions in the terminal, so it fails in a shell without a TTY. Drive it with `expect` there. When a model loses a field, it prints that the column "is missing" and offers Drop, the first choice, or Rename:

- Drop: `expect "is missing"`, then `send "\r"`.
- Rename: `expect "is missing"`, `send "\033\[B"`, then `send "\r"`.

## Hand edits

Toasty 0.11 cannot express these. Each one is a hand edit to the generated SQL.

- For case-insensitive text, add `COLLATE "case_insensitive"` to the generated column, as `users.name`, `users.email` and `invites.email` have it. The first migration creates the collation. Toasty's `filter_by_*`, `upsert_by_*` and unique index on that column then ignore case.
- When a migration removes the last column of a PostgreSQL enum type, Toasty drops the column but not the type. Add `DROP TYPE "<type>";` after it.
- Renaming an enum variant makes the generator panic with "reordering or renaming variants is not supported". Generate while the Rust variant keeps its old name and only its fields are renamed, and answer Rename at the column prompt. Add `ALTER TYPE "<type>" RENAME VALUE '<old>' TO '<new>';` to the SQL and rename the variant in the new snapshot. Then rename the Rust variant. See `20261103_015518_invite_email_refused.sql`.

A model field with `#[column(type = "custom")]` panics when the PostgreSQL driver binds it. Use a type Toasty maps itself, or a `toasty::Embed`.

## Scratch databases

Use a scratch database to try a migration or to run a server of your own. The shared `selfless` database belongs to `mise run dev` and every other worktree.

- Create one: `docker exec selfless-postgres-1 psql -U selfless -c 'CREATE DATABASE selfless_<ticket>'`.
- Point a command at it with `mise exec -- env DATABASE__URL=postgres://selfless:selfless@localhost:5432/selfless_<ticket> <command>`. Put `env` after `mise exec --`, so the command gets the scratch URL whatever the shell exported. A plain `mise exec -- <command>` can run on the shared database.
- A migration that adds a `NOT NULL` column without a default fails on a table that has rows. selfless has no released database, so recreate the scratch database, or delete the rows first.
- Drop it when you are done: `docker exec selfless-postgres-1 psql -U selfless -c 'DROP DATABASE selfless_<ticket> WITH (FORCE)'`.

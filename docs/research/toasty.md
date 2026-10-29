# Toasty 0.11 for a media catalog

Research for [SELFLESS-8](https://app.plane.so/reilley/browse/SELFLESS-8/). Written 2026-10-29 against Toasty 0.11.0, released to crates.io on 2026-09-25 ([crates.io API](https://crates.io/api/v1/crates/toasty)). Source links point at the `toasty-v0.11.0` tag unless they say `main`.

## Summary

Toasty 0.11's SQLite and PostgreSQL drivers cover what a media catalog needs for everyday reads and writes. Both support relations with preloading, interactive transactions with savepoints, and single-row upserts on a primary key or unique field. Both also ship a migration generator. The gaps that matter for selfless are these:

- There is no text search API. Search has to be raw SQL over SQLite FTS5 or PostgreSQL `tsvector`, and raw queries return untyped rows.
- Upsert works on one row per call and cannot go in a `toasty::batch()`, so a library rescan runs one statement per item or uses raw SQL.
- The SQLite migration generator has a bug. When a column change forces a table rebuild, it does not recreate the table's existing indexes, so a `#[unique]` index disappears without an error. I confirmed this against 0.11.0 and current `main`.
- Toasty creates no database foreign keys, so cascades run in the application.
- The query events use the older OpenTelemetry attribute names.

Toasty is pre-1.0 and changes often. Ten minor versions, 0.2.0 through 0.11.0, shipped between 2026-03-30 and 2026-09-25, and each of the last five has breaking changes ([CHANGELOG](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/CHANGELOG.md)).

My recommendation is to use Toasty on one backend only, SQLite, for the catalog's plain CRUD and relations. Put search and bulk sync behind repository ports in the infrastructure layer, written in raw SQL. Review every generated SQLite migration by hand. The grilling ticket makes the decision.

## Project state

- Toasty is MIT licensed ([LICENSE](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/LICENSE)), so it fits the permissive-license rule.
- crates.io lists 0.2.0 on 2026-03-30 and 0.11.0 on 2026-09-25. Version 0.11.0 needs Rust 1.95 ([crates.io API](https://crates.io/api/v1/crates/toasty)).
- An open release PR for 0.12.0 already flags breaking API changes in `toasty-core` and `toasty-driver-sqlite` ([#1263](https://github.com/tokio-rs/toasty/pull/1263)).
- On 2026-10-29 the repository had 91 open issues, 24 of them labelled `C-bug` ([GitHub search](https://github.com/tokio-rs/toasty/issues?q=is%3Aissue+is%3Aopen+label%3AC-bug)).
- CI runs the shared driver integration suite against `postgres:18` ([ci.yml](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/.github/workflows/ci.yml#L235)). SQLite uses `rusqlite` 0.40 with the `bundled` feature ([Cargo.toml](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/Cargo.toml)), which ships SQLite 3.53.2 ([sqlite3.h in rusqlite v0.40.2](https://github.com/rusqlite/rusqlite/blob/v0.40.2/libsqlite3-sys/sqlite3/sqlite3.h)).
- The roadmap is partly out of date. It still lists upsert and many-to-many as future work ([roadmap.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/dev/roadmap.md#L86)), but both shipped: upsert in 0.9.0 ([CHANGELOG](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/CHANGELOG.md)) and many-to-many as a guide chapter ([many-to-many.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/many-to-many.md)). Read the guide and source before the roadmap.
- Both drivers still reach `todo!()` for operation types they do not expect ([SQLite](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-driver-sqlite/src/lib.rs#L297), [PostgreSQL](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-driver-postgresql/src/lib.rs#L529)), and 16 open issues mention a panic ([GitHub search](https://github.com/tokio-rs/toasty/issues?q=is%3Aissue+is%3Aopen+panic)). One example is a panic when a query-based delete touches a model with a `has_many` relation ([#1200](https://github.com/tokio-rs/toasty/issues/1200)).

## Driver comparison

| Topic | SQLite driver | PostgreSQL driver |
| --- | --- | --- |
| Client library | `rusqlite`, blocking calls | `tokio-postgres`, async |
| Upsert by key or unique field | Yes, `INSERT ... ON CONFLICT` | Yes, `INSERT ... ON CONFLICT` |
| Isolation levels | `Serializable` only | All four |
| Lock modes | `BEGIN IMMEDIATE` and `EXCLUSIVE` | Default and deferred only |
| Row locks | None | `SELECT ... FOR UPDATE` |
| Case-insensitive match | `LIKE`, ASCII only | `ILIKE` |
| `Vec<T>` fields | JSON text, predicates scan | Native arrays |
| Enums | `TEXT` with `CHECK` | Native `ENUM` type |
| Timestamps | ISO 8601 text | `TIMESTAMPTZ` |
| Error classes | Almost everything is `DriverOperationFailed` | Serialization failure, read-only, and connection lost are typed |

Sources: [sqlite.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/sqlite.md), [postgresql.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/postgresql.md), [transactions.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/transactions.md), and the capability tables for [SQLite](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-core/src/driver/capability.rs#L660) and [PostgreSQL](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-core/src/driver/capability.rs#L761).

### SQLite details

- Each pooled connection has its own worker task, started with `tokio::spawn` ([connection_task.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/src/db/connection_task.rs#L110)). The SQLite driver calls `rusqlite` directly inside that task ([lib.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-driver-sqlite/src/lib.rs#L176)), so every statement blocks a Tokio worker thread while it runs. For one household this is unlikely to matter, but a long scan stalls other tasks on that thread. Toasty also has an async Turso driver that uses the same SQL dialect ([turso.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/turso.md)).
- In 0.11 you cannot set pragmas through the driver. The guide says to set `journal_mode`, `synchronous`, and `foreign_keys` in your own setup code ([sqlite.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/sqlite.md)). `rusqlite` sets a 5 second busy timeout on every connection it opens ([inner_connection.rs](https://github.com/rusqlite/rusqlite/blob/v0.40.2/src/inner_connection.rs)). WAL mode is stored in the database file once set ([SQLite WAL](https://www.sqlite.org/wal.html)), so one raw `PRAGMA journal_mode=WAL` at startup is enough. On 2026-10-08, `main` gained typed pragma setters on the driver, with `foreign_keys` on by default, but no release includes them yet ([#1285](https://github.com/tokio-rs/toasty/pull/1285), [sqlite.md on main](https://github.com/tokio-rs/toasty/blob/main/docs/guide/src/sqlite.md)).
- An in-memory database caps the pool at one connection ([sqlite.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/sqlite.md)). Use a file database outside tests.
- The driver only accepts `Serializable` isolation and rejects other levels with `UnsupportedFeature` ([lib.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-driver-sqlite/src/lib.rs#L286)).

### PostgreSQL details

- TLS uses `rustls` and is on by default. The URL accepts libpq-style `sslmode`, `application_name`, and `options` for `search_path` ([postgresql.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/postgresql.md)).
- The driver caches up to 100 prepared statements per connection ([lib.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-driver-postgresql/src/lib.rs#L337)). It maps SQLSTATE `40001` and `25006` to typed errors. Any other server error, unique violations included, becomes `DriverOperationFailed` ([lib.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-driver-postgresql/src/lib.rs#L60)).
- The pool runs a health check every 60 seconds by default, and after a lost connection it checks the idle connections and drops the broken ones ([database-setup.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/database-setup.md)). Automatic retry of safe statements is still open ([#863](https://github.com/tokio-rs/toasty/issues/863)).

## Migrations

What exists:

- `push_schema()` creates every table and index, and the guide recommends it only for prototypes and tests ([schema-management.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/schema-management.md)).
- You build a small CLI binary on the `toasty-cli` crate. It has `migration generate`, `apply`, `snapshot`, `drop`, and `reset` ([schema-management.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/schema-management.md)). `generate` diffs the models against the last TOML snapshot and writes a SQL file. It asks interactively whether a dropped and added table, column, or index is a rename.
- `toasty::embed_migrations!()` compiles migrations into the binary, and `MigrationSet::apply(&db)` runs the pending ones at startup ([schema-management.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/schema-management.md)). This suits a self-hosted server that migrates itself on boot.
- Both drivers run each migration inside `BEGIN`/`COMMIT` and record it in `__toasty_migrations` ([SQLite](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-driver-sqlite/src/lib.rs#L370), [PostgreSQL](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-driver-postgresql/src/lib.rs#L607)).

Gaps:

- There are no down migrations. `drop` only removes files, and `reset` drops every table ([schema-management.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/schema-management.md)).
- The generated SQL depends on the driver the CLI connects with ([generate.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-cli/src/migration/generate.rs#L260)). Supporting both SQLite and PostgreSQL means keeping two migration directories and two snapshots, one per backend.
- You cannot create a migration with no schema change. `generate` stops with "No migration needed" ([generate.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-cli/src/migration/generate.rs#L264)). Custom SQL, such as an FTS5 table or a GIN index, has to be added by hand to a generated file. Checksums are off by default, so edits go unnoticed. For SQLite, separate each hand-written statement with `-- #[toasty::breakpoint]`, because the driver runs each chunk with `rusqlite`'s `execute`, and that call rejects a string with more than one statement ([rusqlite lib.rs](https://github.com/rusqlite/rusqlite/blob/v0.40.2/src/lib.rs#L795)).
- SQLite cannot change a column's type or nullability in place, so Toasty rebuilds the table in six steps: create a copy, insert, drop, and rename ([sqlite.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/sqlite.md), [migration.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-sql/src/migration.rs#L241)). The generator drops and recreates only the indexes that changed in the diff ([migration.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-sql/src/migration.rs#L176)). `DROP TABLE` deletes every index and trigger on the old table ([SQLite DROP TABLE](https://www.sqlite.org/lang_droptable.html)). I wrote a scratch test in a clone of `toasty-sql`'s own test file, with a table that had a unique index on `email` and a nullable column made `NOT NULL`. The generated SQL rebuilt the table and never recreated the unique index. The result was the same on `toasty-v0.11.0` and on `main` at `41f569a`. Any FTS5 sync triggers on a rebuilt table would also be lost. I found no upstream issue for this, so a later ticket should report it.
- PostgreSQL alters columns in place, one statement per property change, and the migration assumes exclusive access to the schema. There is no support for zero-downtime migrations such as `CREATE INDEX CONCURRENTLY` ([postgresql.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/postgresql.md)).

## Transactions

- `db.transaction()` opens an interactive transaction. Calling `.transaction()` on a transaction opens a nested one backed by a savepoint. A dropped transaction that was not committed rolls back ([transactions.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/transactions.md)).
- `transaction_builder()` sets the isolation level, read-only mode, and lock mode. SQLite accepts only `Serializable`, but `TransactionMode::Immediate` takes the write lock at `BEGIN`, which avoids `SQLITE_BUSY` on a later write ([transactions.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/transactions.md)). For the webhook writer this is the mode to use.
- A transaction borrows `&mut Db`, so a statement cannot leak onto another pooled connection by mistake. Clone the `Db` to get a second handle ([transactions.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/transactions.md)).
- `toasty::batch()` sends several queries and creates as one atomic statement, and the guide suggests it before interactive transactions ([batch-operations.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/batch-operations.md)).
- The roadmap still lists "rollback on cancel" under runtime work ([roadmap.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/dev/roadmap.md#L152)). I did not test what happens to an open transaction when a request future is cancelled mid-statement.

## Relations

- `#[belongs_to]`, `#[has_many]`, and `#[has_one]` are supported, plus multi-step `via` relations. Many-to-many uses an explicit join model with `has_many(via = ...)`, and the `via` side is read-only ([relationships.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/relationships.md), [many-to-many.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/many-to-many.md)). Composite foreign keys work.
- `.include()` preloads relations in a fixed number of queries. That covers `via` relations on SQL backends. Includes can filter and order, both added in 0.9.0 ([preloading-associations.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/preloading-associations.md), [CHANGELOG](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/CHANGELOG.md)). An `.await` marks every query, which makes N+1 queries easy to spot.
- Multi-level `.include()` is still on the roadmap ([roadmap.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/dev/roadmap.md#L88)). For a series, season, and episode tree, the options are eager relation fields, which "can load other eager relations" ([relationships.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/relationships.md)), a `via` relation from series to episodes, or a second query.
- `.any()` and `.all()` filter parents by their children, and a path filter can cross `has_one` and `belongs_to` ([filtering-with-expressions.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/filtering-with-expressions.md)).
- Toasty emits no `FOREIGN KEY` constraints. Cascade deletes and null-setting happen in the query engine ([relationships.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/relationships.md#L266)), and the design for real constraints is still open ([#366](https://github.com/tokio-rs/toasty/issues/366)). A raw SQL write, or a second process writing to the database, can leave orphan rows.
- Aggregates such as `GROUP BY` and `SUM` are not built in ([#421](https://github.com/tokio-rs/toasty/issues/421)). Counting is supported ([query_count.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-driver-integration-suite/src/tests/query_count.rs)).

## Gaps that matter for a media catalog

### Text search

Toasty has no text search API. The full-text search issue is open with no design ([#423](https://github.com/tokio-rs/toasty/issues/423)). The roadmap also lists string `contains` and `ends_with` as future work ([roadmap.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/dev/roadmap.md#L51)). What 0.11 has:

- `.starts_with()` on every backend, which lowers to `GLOB 'x*'` on SQLite and `^@` on PostgreSQL. Both can use an index ([filtering-with-expressions.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/filtering-with-expressions.md)).
- `.like()`, which passes straight through to the database. SQLite's `LIKE` ignores case for ASCII only, so `"Amélie"` will not match `"AMÉLIE"`.
- `.ilike()`, on PostgreSQL only.
- A `%term%` pattern scans the whole table.

For real search, use raw SQL:

- **SQLite.** The bundled build compiles FTS5 in ([libsqlite3-sys build.rs](https://github.com/rusqlite/rusqlite/blob/v0.40.2/libsqlite3-sys/build.rs)). Create an FTS5 table ([SQLite FTS5](https://www.sqlite.org/fts5.html)) in a hand-edited migration and keep it in sync from the repository code or with triggers. Triggers have the rebuild risk from the migrations section.
- **PostgreSQL.** Use a generated `tsvector` column with a GIN index ([PostgreSQL text search tables](https://www.postgresql.org/docs/18/textsearch-tables.html)), or `pg_trgm` for fuzzy title matches ([pg_trgm](https://www.postgresql.org/docs/18/pgtrgm.html)). Toasty cannot declare GIN indexes yet ([#673](https://github.com/tokio-rs/toasty/issues/673)), so these also go in hand-edited migrations.
- `toasty::sql::query` returns rows as `Value::Record`, not models ([raw-sql.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/raw-sql.md)). A search therefore takes two steps. The raw query returns ranked IDs, then `Model::filter(id.in_list(ids))` loads the models, and the Rust code puts them back in rank order. Placeholders differ by backend, `?1` on SQLite and `$1` on PostgreSQL, which is one more reason to pick a single backend.

### Bulk upserts

- `upsert_by_<field>()` is generated for primary keys and unique fields, composite ones included. It supports `on_create`, `on_update`, `or_ignore`, and `incoming()` ([upserting-records.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/upserting-records.md)). This maps well to "insert or update this movie by its TMDB or TVDB id" from a Radarr or Sonarr webhook.
- Each call handles one record ([upserting-records.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/upserting-records.md#L239)). The generated upsert builder has only `exec` ([upsert.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty-macros/src/model/expand/upsert.rs#L262)) and does not implement `IntoStatement` ([into_statement.rs](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/src/stmt/into_statement.rs#L25)), so it cannot go in a `toasty::batch()`. Upsert also cannot set relations or create nested records.
- `create_many()` inserts many rows at once, but it fails on any conflict ([batch-operations.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/batch-operations.md)). "Bulk insert / update" is still on the roadmap ([roadmap.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/dev/roadmap.md#L95)).
- For a full library rescan, the options are a loop of `upsert_by_*` calls inside one `Immediate` transaction, or raw multi-row `INSERT ... ON CONFLICT`. Webhooks carry one item each, so only rescans and the first import have this problem. I did not measure the loop's speed.

### Observability

- Each statement runs in a `tracing` span named `query` with `stmt.kind` and `model`, and emits a `toasty::query` event with `db.system`, `db.statement`, `duration_ms`, and `rows` ([tracing.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/tracing.md)). The caller's span is carried to the connection task, so query events land under the request span.
- OpenTelemetry semantic conventions v1.44.0 mark `db.statement` and `db.system` as deprecated, replaced by `db.query.text` and `db.system.name` ([db attribute registry](https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/registry/attributes/db.md)). The stable span name is `{db.query.summary}` or `{db.operation.name} {target}` ([database spans](https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/db/database-spans.md)). Toasty's span is always named `query`, and the statement goes on an event, not on the span. "OpenTelemetry span-per-statement export helpers" is a roadmap item ([roadmap.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/dev/roadmap.md#L160)).
- To follow semantic conventions from day one, selfless needs its own client spans at the repository layer, or a `tracing` layer that renames Toasty's fields.

### Other points

- `uuid::Uuid` keys with `#[auto]` generate UUID v7 ([keys-and-auto-generation.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/keys-and-auto-generation.md)). SQLite stores them as a 16-byte `BLOB`, which is harder to read in a SQL shell ([sqlite.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/sqlite.md)).
- `jiff` timestamps are ISO 8601 text on SQLite and `TIMESTAMPTZ` on PostgreSQL ([sqlite.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/sqlite.md), [postgresql.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/postgresql.md)).
- `Vec<String>` fields for genres or tags are JSON text on SQLite, and their predicates cannot use an index ([sqlite.md](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/docs/guide/src/sqlite.md)). A join model is better for genre filters.
- Since 0.8.0, `#[version]` adds optimistic concurrency on SQL drivers, which helps when two clients update the same row ([CHANGELOG](https://github.com/tokio-rs/toasty/blob/toasty-v0.11.0/crates/toasty/CHANGELOG.md)).

## Recommendations

1. Pick one backend. SQLite fits a single-household homelab server and needs no extra service. Supporting both backends doubles the migrations and the raw SQL.
2. Use Toasty for entities, relations, single-row upserts, and transactions. Put search, bulk sync, and anything else that needs raw SQL behind repository ports in the infrastructure layer, so the ORM stays a detail of that layer.
3. Treat generated SQLite migrations as drafts. After any rebuild, check that the indexes are still there, add custom SQL with breakpoints, and test migrations against a copy of a real database.
4. Enable WAL with a raw pragma at startup until a release with the typed pragma setters ships.
5. Pin an exact Toasty version and plan an upgrade for every minor release.

## Open questions for the decision

- Is SQLite the only target, or does PostgreSQL need to stay possible? This decides how many migration sets and raw SQL variants selfless keeps.
- Is a blocking SQLite driver on Tokio worker threads acceptable, or should selfless use the async Turso driver, which has its own maturity questions not covered here?
- Should selfless report the index-loss bug and wait for a fix, or patch generated migrations by hand?
- Which search does the catalog need: title prefix only, accent-insensitive search, or ranked full text over overviews and cast? Prefix search works in Toasty today. The others need FTS5 or `tsvector`.
- How large is a first import or rescan? Is a loop of single-row upserts fast enough there, or should it use raw multi-row SQL?
- Are app-level cascades without database foreign keys acceptable, given that only selfless writes to the database?
- Should database spans come from selfless's repository layer or from a layer that maps Toasty's events?

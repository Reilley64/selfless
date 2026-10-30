# Coding standards

The `code-review` step applies these rules to each diff. Each rule is a judgement call. `cargo fmt`, Clippy with `-D warnings`, `cargo deny`, TypeScript, ESLint, vacuum, Weaver, actionlint and `mise run lint:versions` own the mechanical rules. The ADRs and the Telemetry conventions page in Plane own the architecture decisions. Cite them when a diff breaks one.

## Code

- New code has no comments or doc comments. Names, types and constants carry the meaning. Upstream lines that appear as context in `patches/` are not ours.
- Keep single-use production logic inline. Extract only for real reuse, a layer boundary, or behavior that means something on its own. Do not extract code to make a test easier.
- Merge two copies only when every change to one must also change the other. Code that serves different actors, such as users and admins, stays separate even when it looks the same.

## Domain model

- A business rule that holds whichever screen or flow runs it lives in `domain`, on the type whose data it uses. This includes any value the glossary names or that comes from a domain constant, such as an expiry. The use case passes the result to the Toasty builder.
- A rule that needs several models but no database, clock or port is a function in `domain` that takes them. The use case loads them and passes them in.
- A domain value type that must satisfy a rule has a private field and one `new` that returns `Result<Self, Marker>`, with the marker in `domain`. It has no setters. A change builds a new value.
- A fixed set of kinds is a domain enum. `domain` and `application` derive no wire format. Presentation owns the JSON names and labels and maps them to the domain type.
- A model refers to another root model by its ID. A use case changes a root only through that root's own `DbSet` or a loaded instance of it.

## Layers and settings

- Dependencies point inward: domain, application, infrastructure, presentation, then the binary. `deny.toml` lists each crate's allowed dependents. A new edge needs a reason the diff makes clear.
- A port is a newtype over `Arc<dyn Fn>` with a `call` method that takes named parameters. Application owns the port, and an infrastructure crate builds it.
- Use case Outputs carry plain data, never Toasty models. Each use case defines its own Output. Outputs are never shared between use cases.
- Settings come only from environment variables, with no prefix and `__` for nesting. Each adapter crate owns its settings type, and `AppSettings::load()` in `infrastructure/settings` composes them.
- Every database call goes through the `DbContext`, including hand-written SQL through `toasty::sql`. Hand-written SQL runs on PostgreSQL 14, the homelab's version.
- An insert that must not fail on a unique clash uses Toasty's upsert, `Model::upsert_by_*()`, with `.or_ignore()` when the existing row must stay, because Toasty's `DbSet` has no upsert. Every other insert starts from a `DbSet` root, such as `db_context.users().create()`. Bulk upserts stay in `toasty::sql`, as ADR 0001 says.
- Rustls is the only TLS stack. A dependency with a choice of TLS backend sets `default-features = false` and picks Rustls by name.

## Files

- Application groups use cases by capability. Each use case is `<use_case>.rs` with `call`, and its `Dependencies`, `Output` and `Error` sit in `<use_case>/dependencies.rs`, `output.rs` and `error.rs`. A marker error that only one use case returns goes in its `error.rs`. See `application/src/users/create_user.rs`.
- Ports live in `application/src/ports/`. `persistence.rs` holds the `DbContext`, and `mod.rs` holds the callable ports in alphabetical order.
- Domain has one file per type, re-exported from `lib.rs`.
- API routes live in `presentation/api/src/routes/`, one file per resource and version, such as `routes/api/sessions/v1.rs`. The admin app splits its views into `pages/`, `templates/` and `atoms/`.

## Logging and telemetry

- Log an error once, where it is handled, as `error = &error as &dyn Error` with a constant message.
- Use OpenTelemetry semantic convention names. A name they lack gets a `selfless.*` entry in `telemetry/registry`, and code uses the generated constant.
- User and media IDs go on spans and events, never on metric attributes. Usernames, passwords, session tokens and signed URLs never appear in telemetry.

## Use cases and adapters

- A use case loads models, calls domain methods and saves them. It owns the transaction, the order of steps and the ownership or authorization check.
- A use case takes domain value types for input it stores or acts on. Presentation builds them and maps their marker errors. Input the use case only checks, such as a sign-in password, stays a plain type.
- A use case's `Dependencies` holds only the ports it calls.
- A port is named for what the use case needs. Its parameters, results and errors are application or domain types, never a vendor's request or response shapes.
- Radarr, Sonarr, NFO and SMTP shapes stay in the crate that talks to that system. That crate translates them into domain types before they cross a port, and keeps only the IDs and fields selfless uses.
- A use case that must reach Radarr, Sonarr or the SMTP server after a change writes an outbox row with IDs in the same transaction, and a worker makes the call. No call to those systems runs inside a transaction or a request.
- An inbox or outbox handler gives the same result when it runs twice or out of order. It re-reads current state, or checks a marker committed in the same transaction as its change.
- A route, admin page, webhook handler or worker loop calls one use case and only maps its input and output.
- A screen that combines several models, such as Continue watching, gets one use case whose query builds its Output.
- The `selfless` crate builds every port, `AppState`, router and worker. Other crates expose constructors for it to call.
- A diff that adds a framework or runtime crate to `domain` or `application` cites the ADR that accepts it. Plain libraries such as `jiff` or `uuid` need none.

## API

- `openapi.yaml` is written by hand and describes every endpoint the server serves. Each resource's version is at the end of its path.

## Tests

- Test each behaviour once, at the smallest component that owns it, in a `#[cfg(test)] mod tests` at the bottom of that component's file: a domain rule in the domain type; a use case on the mocked database (Toasty's in-memory SQLite from `test_fixtures::mock_database`) with fake ports; a route or admin page over HTTP through `TestServer` or `Browser`, which check every JSON response against `openapi.yaml`; a worker or adapter at the bottom of its own file. These unit tests never touch PostgreSQL.
- Unit tests check only our own logic. Never test what a dependency does, such as Toasty's id generation or serde ignoring unknown fields.
- A behaviour that relies on PostgreSQL is an integration test. The case-insensitive collation, unique-index races, `FOR UPDATE` and other hand-written SQL, and the migrations go in `infrastructure/persistence/tests/`. Startup, health and build info through `selfless::start` go in the root `tests/`. A test that sets a global telemetry provider, or compares timings, gets its own file in its crate's `tests/`.
- An HTTP test checks wiring: one step past each time limit, with the problem type. The exact limit is the domain unit test.
- Tests read data through the `DbContext`. Use raw SQL only where no model exposes the data.
- Fixtures live in `test/`. `test-fixtures` depends on no workspace crate. `test-app` builds an `AppState` for presentation tests. Each crate's glue is a file in `test/` included with `#[path]`.
- An app module or component is tested with Jest in a `*.test.ts` file next to it, with a fake storage or a mocked fetch.
- Each test checks behaviour a spec asks for, and its name says which.

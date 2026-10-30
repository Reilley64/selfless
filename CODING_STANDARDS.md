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
- Settings come only from environment variables, with no prefix and `__` for nesting. Each adapter crate owns its settings type, and `AppSettings::load()` in `infrastructure/settings` composes them.
- Every database call goes through the `DbContext`, including hand-written SQL through `toasty::sql`. Hand-written SQL runs on PostgreSQL 14, the homelab's version.
- Rustls is the only TLS stack. A dependency with a choice of TLS backend sets `default-features = false` and picks Rustls by name.

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

- Tests sit at the seams the spec's Testing Decisions name: the whole server in process against PostgreSQL over HTTP, with every JSON response checked against `openapi.yaml`, and the shared app components with a mocked fetch. Each test checks behavior a spec asks for.
- Do not test a dependency's behavior on its own.

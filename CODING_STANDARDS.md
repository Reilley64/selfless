# Coding standards

The `code-review` step applies these rules to each diff. Each rule is a judgement call. `cargo fmt`, Clippy with `-D warnings`, `cargo deny`, TypeScript, ESLint, vacuum, Weaver, actionlint and `mise run lint:versions` own the mechanical rules. The ADRs and the Telemetry conventions page in Plane own the architecture decisions. Cite them when a diff breaks one.

## Code

- New code has no comments or doc comments. Names, types and constants carry the meaning. Upstream lines that appear as context in `patches/` are not ours.
- Keep single-use production logic inline. Extract only for real reuse, a layer boundary, or behavior that means something on its own. Do not extract code to make a test easier.

## Layers and settings

- Dependencies point inward: domain, application, infrastructure, presentation, then the binary. `deny.toml` lists each crate's allowed dependents. A new edge needs a reason the diff makes clear.
- A port is a newtype over `Arc<dyn Fn>` with a `call` method that takes named parameters. Application owns the port, and an infrastructure crate builds it.
- Use case Outputs carry plain data, never Toasty models.
- Settings come only from environment variables, with no prefix and `__` for nesting. Each adapter crate owns its settings type, and `AppSettings::load()` in `infrastructure/settings` composes them.
- Every database call goes through the `DbContext`, including hand-written SQL through `toasty::sql`. Hand-written SQL runs on PostgreSQL 14, the homelab's version.
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

## API

- `openapi.yaml` is written by hand and describes every endpoint the server serves. Each resource's version is at the end of its path.

## Tests

- Tests sit at the seams the spec's Testing Decisions name: the whole server in process against PostgreSQL over HTTP, with every JSON response checked against `openapi.yaml`, and the shared app components with a mocked fetch. Each test checks behavior a spec asks for.
- Do not test a dependency's behavior on its own.

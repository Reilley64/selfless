# Writing and running tests

`CODING_STANDARDS.md` ("Tests") says which tests to write and where each one goes. This page says how to build them with the fixtures in `test/`, and how to run them.

## Fixtures

- `test/fixtures/src/lib.rs` lists the shared fakes and helpers: the fake clock and mailer, `TestServer` and `Browser` for HTTP, `TestDatabase` for PostgreSQL, `Logs`, `ScriptedSmtpServer` and `set_default_subscriber`.
- `TestApp` runs on the mocked database. Its `AcquireLock` holds nothing, its `TryAcquireLock` always succeeds, and its `ExportTelemetry` always fails. A relay test that needs a 200 replaces `app.state.export_telemetry` with its own port.
- The glue files are `test/application.rs`, `api.rs`, `admin.rs`, `persistence.rs`, `selfless.rs` and `webhooks.rs`. A helper that one crate needs goes in that crate's glue file. It goes in `test/fixtures` only when several crates need it. `test/application.rs` has `RecordingLock`, which records the lock keys a use case asks for, and mailers that fail or refuse.
- `test/selfless.rs` starts the server for root tests. It also has the stub Radarr and Sonarr servers, a temporary media folder, `fixture()`, `send_webhook` and `eventually`.
- Data fixtures, such as Radarr and Sonarr API responses, webhook bodies, NFO files and ffprobe output, live in `test/fixtures/radarr/`, `sonarr/` and `ffprobe/`.
- A root test includes the glue as `#[path = "../test/selfless.rs"] pub mod selfless_test;`. Naming the module `selfless` makes `selfless::start` ambiguous with the crate. The module is `pub` because each root test uses only some of the helpers, and Rust warns about unused items in a private module.
- Seed the mocked database through the `DbSet` roots, such as `db_context.users().create()`. A session row needs a `client_id`. When a test needs a real password hash, seed through `create_user::call` or `create_admin::call` instead.
- A domain test can build a Toasty model as a struct literal. Use `Deferred::default()` for a `#[belongs_to]` field and `Uuid::nil()` for IDs, as the tests in `domain/src/session.rs` do.

## HTTP tests

- `TestServer::request`, `request_with_headers` and `Browser` check each JSON response against the one response `openapi.yaml` lists for its status. If a status can carry two problem types, that response's `type` is an `enum` of both, like `BadRequestOrInvalidIdempotencyKey`.
- `request_with_response_headers` also returns the response headers, such as the `WWW-Authenticate` of a webhook's 401.
- `get_text` follows redirects and checks nothing against `openapi.yaml`, so `get_text("/admin")` returns the sign-in page with 200.
- `post_protobuf` panics with `openapi.yaml has nothing at` when the file lists no protobuf body for that path and status, such as a 401.
- `#[route(GET "/x")]` works inside a `#[cfg(test)] mod tests`, so a test can serve its own routes with `TestServer::serve(Router::builder().route(x).layer(ProblemLayer).build())`. The macro turns the function into a unit struct, so a route named like a local variable in the same test fails to compile.
- To make a use case fail with an unexpected error, rename the table it reads with `toasty::sql::statement("ALTER TABLE users RENAME TO users_gone")`, as `presentation/api/src/problem.rs` does.

## Time

- Domain time comes from `FakeClock`. Session and invite expiry includes the limit itself, so a web session idle for exactly 30 days has expired. "One step past" a limit is the limit.
- A worker loop test runs with `#[tokio::test(start_paused = true)]` and needs `tokio` with `test-util` in the crate's dev-dependencies. Follow `serving_deletes_expired_sessions_once_a_day` in `presentation/workers/src/session_cleanup.rs`. Call `tokio::task::yield_now()` before every read, not only before the first `advance`. Without it, the read runs before a delete that a woken tick has just started.

## Tracing, logs and metrics

- Install a test's subscriber with `test_fixtures::set_default_subscriber`. Clippy bans `tracing::subscriber::set_default`. tracing-core caches whether a span or event is enabled the first time any thread reaches it. A test without a subscriber can then cache a callsite as disabled for a test that has one. The fixture keeps a second dispatcher registered, so tracing-core asks every subscriber.
- Install the subscriber after `TestApp::new()` or `mock_db_context()`, because Toasty logs a warning about the SQLite pool while it builds the mocked database. `TestServer::serve` runs the server on the test's current-thread runtime, so the subscriber also sees the server task's spans and logs.
- Capture log text with `test_fixtures::Logs`: pass `move || logs.clone()` to a fmt layer's `with_writer`, then read `logs.text()`. Filter with `EnvFilter::new("info")`, because at `trace` Toasty logs the values it binds.
- An `error!` event inside a span already sets that span's status to Error, so a test of a failure span checks `error.type`, not only the status.
- The sign-in counter is a static built on first use. A test binary that checks it sets its global meter provider before the first sign-in, as `application/tests/sign_in_metrics.rs` does.

## PostgreSQL integration tests

- They need the compose PostgreSQL (`mise run services`) and `DATABASE__URL`. In a shell where mise is not active, run them as `mise exec -- cargo test -p persistence` or `mise exec -- cargo test --test <file>`. Without `DATABASE__URL`, `TestDatabase::create` panics.
- Each test creates its own database with `TestDatabase::create()` and ends with `database.drop().await`. `db_context(&database)` from `test/persistence.rs` connects and runs the migrations.
- Every worktree shares the compose containers (project `selfless`). If another worktree stops them, `mise run services` starts them again. When the disk fills, OrbStack stops and PostgreSQL with it. Free space, run `orb start`, then `mise run services`.
- Under heavy machine load, server tests fail with `Timeout occurred while creating a new object`, and the sign-in timing tests can fail. Re-run them with `-- --test-threads=2` once the load drops.

## Flaky and unproven tests

- Reproduce a flaky test by running its test binary in a loop. `cargo test -p <crate> --no-run` prints the binary's path.
- To prove that a test of a lock, race or loop can fail, break what it guards by hand, run the test, and revert. For example, drop the lock guard early, replace an upsert with a create, or halve a worker's period.

## Jest

- `presentation/ui/jest.resolver.js` removes the `node` export condition, so Jest resolves packages the way Metro does. Without it, the OTLP exporters load their Node transport and never call the mocked `fetch`.
- With a cold Jest cache, `ServerInfo.test.tsx` can exceed the 5-second test timeout while Babel transforms React Native. A second run passes.

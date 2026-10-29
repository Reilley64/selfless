<img src="assets/icon.svg" width="64" height="64" alt="">

# selfless

Created in [T3 Code](https://t3.codes).

## Development

Install [mise](https://mise.jdx.dev) and Docker, then run `mise install` in the repository root.

- `mise run services` starts PostgreSQL and grafana/otel-lgtm from `compose.yaml`. Grafana runs at http://localhost:3000.
- `cargo run` starts the server. It reads `DATABASE__URL` (set in `mise.toml`) and `HTTP__PORT` (default 8080). It sends traces, metrics and logs over OTLP gRPC to `localhost:4317`. The standard `OTEL_*` variables change this, and `mise.toml` sets `OTEL_SERVICE_NAME`.
- A cargo build runs Weaver to generate Rust constants from `telemetry/registry`. `mise run generate:telemetry` writes the TypeScript constants. `mise run lint:telemetry` checks the registry.
- `mise run test` runs the server tests against that PostgreSQL. Each test creates its own database.
- `mise run check` checks formatting, Clippy lints and crate direction.
- `mise run lint:openapi` lints `openapi.yaml`.
- `cargo run --bin toasty -- migration generate` writes a migration for changed models to `infrastructure/persistence/toasty`. The server applies pending migrations when it starts.

## License

selfless is licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.

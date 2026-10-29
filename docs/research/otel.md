# OpenTelemetry semantic conventions for selfless

Research for [SELFLESS-9](https://app.plane.so/reilley/browse/SELFLESS-9/). Sources were read on 2026-10-29. The newest semantic conventions release on that date is v1.44.0 (2026-08-04) ([releases][sc-releases]), and every convention link below points at that tag.

## Summary

- **Server HTTP.** The HTTP server span and the `http.server.request.duration` histogram are Stable. Use them for the API and for Radarr and Sonarr webhooks ([HTTP spans][sc-http-spans], [HTTP metrics][sc-http-metrics]).
- **Server database.** The database client span and `db.client.operation.duration` are Stable, and so is `db.system.name=postgresql`. The value `sqlite` is still Development ([DB spans][sc-db-spans], [DB metrics][sc-db-metrics], [db registry][sc-reg-db]). Toasty 0.11.0 opens a `tracing` span named `query` with `stmt.kind` and `model` fields, not a span that follows the database conventions, so selfless must add the `db.*` attributes itself ([Toasty instrument.rs][toasty-instrument]).
- **Server process.** The process metrics and process resource attributes became Release Candidate in v1.44.0 ([process metrics][sc-process-metrics], [changelog][sc-changelog]).
- **Server runtime.** No convention covers Rust or Tokio. The runtime page asks each runtime to use its own `{environment}.*` prefix, so Tokio metrics would be `tokio.*` and named by selfless ([runtime][sc-runtime]).
- **Rust SDK.** In `opentelemetry` 0.33.1 (2026-10-09), logs and metrics are Stable, traces are Beta, and context and propagators are Beta ([Rust README][rust-readme]). The Tower HTTP middleware that follows the HTTP conventions is alpha ([Tower README][tower-readme]).
- **Browser.** The core JS API and SDK are Stable for traces, metrics and logs (JS SDK 3.0.0, 2026-10-20) ([JS README][js-readme]). Browser instrumentation is still experimental, and the old browser packages are moving to the new `opentelemetry-browser` repository ([JS README][js-readme], [browser README][browser-readme]). Browser semantic conventions are Development. The only browser event in the main registry is `browser.web_vital` ([browser events][sc-browser-events]).
- **React Native.** OpenTelemetry has no React Native SDK. Since JS SDK 3.0.0, React Native and Expo officially use the browser builds of the JS packages, which need Metro 0.81.3 or later ([JS README][js-readme]). Vendor wrappers exist, but none of their READMEs mention tvOS or Android TV (see [React Native SDK maturity](#react-native-sdk-maturity)).
- **Playback and other domain names.** OpenTelemetry has no media or playback conventions. The naming rules tell application developers to prefix their own names with a company reverse domain or a unique application name, and never to use an existing OpenTelemetry namespace ([naming][sc-naming]). The recommended option is a `selfless.*` prefix, defined in a small Weaver registry that depends on the OpenTelemetry registry ([OTEP 4815][otep-4815]).

## Versions checked

| Component | Version | Date | Source |
| --- | --- | --- | --- |
| Semantic conventions | v1.44.0 | 2026-08-04 | [releases][sc-releases] |
| `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `opentelemetry-appender-tracing` | 0.33.1 | 2026-10-09 | [crates.io][crate-otel], [Rust README][rust-readme] |
| `opentelemetry-semantic-conventions` (Rust) | 0.33.1, generated from semconv v1.42.0 | 2026-10-08 | [crate changelog][rust-semconv-changelog], [lib.rs][rust-semconv-lib] |
| `tracing-opentelemetry` | 0.34.0 | 2026-09-20 | [changelog][to-changelog] |
| `opentelemetry-instrumentation-tower` | 0.19.0 | 2026-09-21 | [changelog][tower-changelog] |
| `opentelemetry-instrumentation-process` | 0.3.0 | 2026-10-09 | [crates.io][crate-process] |
| `toasty` | 0.11.0 | 2026-09-25 | [crates.io][crate-toasty] |
| OpenTelemetry JS stable packages | 3.0.0 | 2026-10-20 | [v3.0.0 release][js-v3] |
| OpenTelemetry JS experimental packages | 0.300.0 | 2026-10-20 | [v3.0.0 release][js-v3] |
| `@opentelemetry/semantic-conventions` | 1.44.0 | 2026-10-28 | [npm][npm-semconv] |
| `@opentelemetry/browser-sdk` | 0.6.0 | 2026-10-22 | [npm][npm-browser-sdk] |
| `@opentelemetry/browser-instrumentation` | 0.9.0 | 2026-10-22 | [npm][npm-browser-instr] |
| Weaver | v0.28.0 | 2026-10-14 | [releases][weaver-releases] |

## How stability levels read

The specification defines the levels. Development components "SHOULD NOT be used in production" and may be removed without notice. Release Candidate means feature-complete, with breaking changes "only allowed under special circumstances". Stable has the same promise and is ready for general use ([maturity levels][spec-maturity]). A convention page can be Mixed, which means sections have different levels ([HTTP README][sc-http-readme]).

The opt-in variable `OTEL_SEMCONV_STABILITY_OPT_IN` exists so that old instrumentations can move from the experimental to the stable HTTP and database conventions ([HTTP README][sc-http-readme], [DB README][sc-db-readme]). selfless starts fresh, so it can emit the stable conventions only and needs no opt-in handling.

## Server conventions

### HTTP

The HTTP server span is Stable. Its span kind is `SERVER`. The name is `{method} {http.route}`, for example `GET /items/{id}`, and instrumentation "MUST NOT default to using URI path" as the target ([HTTP spans][sc-http-spans]).

Required attributes are `http.request.method`, `url.path` and `url.scheme`. Conditionally required ones are `error.type`, `http.response.status_code`, `http.route`, `url.query`, `server.port` and a few more. Recommended ones include `client.address`, `server.address`, `network.protocol.version` and `user_agent.original` ([HTTP spans][sc-http-spans]).

A 4xx response leaves the server span status unset, and a 5xx sets it to `Error` ([HTTP spans][sc-http-spans]).

Behind a reverse proxy, the server reads `server.address` and `server.port` first from `Forwarded#host` or `X-Forwarded-Host`, then from `:authority`, then from `Host` ([HTTP spans][sc-http-spans]). This applies when Traefik or another proxy sits in front of selfless.

Metrics ([HTTP metrics][sc-http-metrics]):

| Metric | Type | Unit | Stability |
| --- | --- | --- | --- |
| `http.server.request.duration` | Histogram | `s` | Stable |
| `http.server.active_requests` | UpDownCounter | `{request}` | Development |
| `http.server.request.body.size` | Histogram | `By` | Development |
| `http.server.response.body.size` | Histogram | `By` | Development |
| `http.client.request.duration` | Histogram | `s` | Stable |

`http.client.*` applies if the server calls Radarr or Sonarr APIs. The webhooks themselves are inbound requests, so they get server spans.

### Database

The database client span is Stable. Its name is `{db.query.summary}` when a summary exists, otherwise `{db.operation.name} {target}`. The kind is `CLIENT`, or `INTERNAL` for an in-memory database ([DB spans][sc-db-spans]).

`db.system.name` is required. `db.collection.name`, `db.namespace`, `db.operation.name`, `db.response.status_code` and `error.type` are conditionally required. `db.query.summary`, `db.query.text` and `server.address` are recommended ([DB spans][sc-db-spans]).

The value of `db.system.name` depends on the database selfless picks. `postgresql` and `mysql` are Stable, while `sqlite` is Development ([db registry][sc-reg-db]). The SQL and PostgreSQL pages are Stable, and there is no SQLite page ([DB README][sc-db-readme], [SQL][sc-db-sql]).

Metrics ([DB metrics][sc-db-metrics]):

| Metric | Type | Unit | Stability |
| --- | --- | --- | --- |
| `db.client.operation.duration` | Histogram | `s` | Stable |
| `db.client.connection.count` | UpDownCounter | `{connection}` | Development |
| `db.client.connection.wait_time` | Histogram | `s` | Development |
| `db.client.response.returned_rows` | Histogram | `{row}` | Development |

The DB span page recommends instrumenting at the level of the caller's logical operation ([DB spans][sc-db-spans]). Toasty 0.11.0 instruments with `tracing`. For each statement it opens a debug-level span named `query` (target `toasty::query`) with the fields `stmt.kind` and `model`, and carries it to the connection's worker task ([Toasty instrument.rs][toasty-instrument]). The drivers log events such as `tracing::trace!(driver = "sqlite", op = %op.name(), "driver exec")` ([Toasty SQLite driver][toasty-sqlite]). None of this sets `db.system.name` or any other `db.*` attribute. Through `tracing-opentelemetry`, the `query` span would arrive as an internal span named `query`. selfless therefore needs its own database client spans, or a change to Toasty. A natural place for its own spans is the infrastructure adapter that implements each repository port.

### Process

The process resource (`process.pid`, `process.creation.time`, `process.command`, `process.runtime.*` and others) and most process metrics are Release Candidate in v1.44.0 ([process resource][sc-process-resource], [process metrics][sc-process-metrics]). The changelog notes that the namespace became Release Candidate in this release ([changelog][sc-changelog]).

| Metric | Type | Unit | Stability |
| --- | --- | --- | --- |
| `process.cpu.time` | Counter | `s` | Release Candidate |
| `process.cpu.utilization` | Gauge | `1` | Release Candidate |
| `process.memory.usage` | UpDownCounter | `By` | Release Candidate |
| `process.memory.virtual` | UpDownCounter | `By` | Release Candidate |
| `process.disk.io` | Counter | `By` | Release Candidate |
| `process.network.io` | Counter | `By` | Release Candidate |
| `process.thread.count` | UpDownCounter | `{thread}` | Release Candidate |
| `process.unix.file_descriptor.count` | UpDownCounter | `{file_descriptor}` | Release Candidate |
| `process.context_switches` | Counter | `{context_switch}` | Release Candidate |
| `process.uptime` | Gauge | `s` | Release Candidate |

The same metrics can describe ffmpeg child processes if selfless runs them for transcoding. The process resource lists runtimes for Erlang, Go, Java, JavaScript, .NET, Python and Ruby, but not Rust ([process resource][sc-process-resource]).

The Rust crate `opentelemetry-instrumentation-process` 0.3.0 emits seven of these metrics on Linux and does nothing on other systems ([README][process-readme]). Its source records `process.memory.usage` with a gauge and `process.context_switches` with a gauge, while the conventions call for an UpDownCounter and a Counter ([source][process-src], [process metrics][sc-process-metrics]). The Rust semantic conventions crate hides the process metric names behind its `semconv_experimental` feature ([metric.rs][rust-semconv-metric]).

### Runtime

The runtime page is Development. It defines no shared runtime metrics, because heap and GC numbers do not compare across runtimes. Runtime-specific metrics should use the runtime's own prefix, such as `jvm.*`, and the instrumentation author chooses that prefix. Pages exist for CPython, Go, JVM, Node.js, V8 and .NET only ([runtime][sc-runtime]). No issue in the semantic conventions repository proposes Tokio or Rust runtime metrics (GitHub issue search for "tokio" returned 0 results on 2026-10-29).

For Tokio, the `tokio-metrics` crate reads runtime metrics. Some of them need the `tokio_unstable` compiler flag ([tokio-metrics README][tokio-metrics]). Exporting them under `tokio.*` follows the runtime page's rule.

### Resource attributes

`service.name`, `service.version`, `service.namespace` and `service.instance.id` are Stable ([service][sc-service]). `deployment.environment.name` is Stable ([deployment environment][sc-deployment]). `host.*` and `os.*` are Development ([host][sc-host], [os][sc-os]).

### Errors

Exceptions recorded as span events are Deprecated. The conventions now record exceptions as logs ([exceptions on spans][sc-exceptions-spans]). For any failed operation, instrumentation should set span status `Error` and `error.type`, and duration histograms should carry `error.type` only on failures ([recording errors][sc-recording-errors]).

### Rust SDK and crates maturity

The `opentelemetry-rust` README for 0.33.1 lists these levels ([Rust README][rust-readme]):

| Component | Status |
| --- | --- |
| Context | Beta |
| Baggage | RC |
| Propagators | Beta |
| Logs API, Logs SDK, Logs OTLP exporter, Logs appender for `tracing` | Stable |
| Metrics API, Metrics SDK, Metrics OTLP exporter | Stable |
| Metrics Prometheus exporter | Beta |
| Traces API, Traces SDK, Traces OTLP exporter | Beta |

The README recommends `tracing` as the logging API for new projects ([Rust README][rust-readme]). `tracing-opentelemetry` 0.34.0 sends `tracing` spans to an OpenTelemetry tracer. It reads the special fields `otel.name`, `otel.kind`, `otel.status_code` and `otel.status_description`, and passes other fields through as attributes, so a field such as `"server.port" = 80` becomes a semantic convention attribute. It does not export logs. Its docs point to `opentelemetry-appender-tracing` for that ([tracing-opentelemetry lib.rs][to-lib]).

`opentelemetry-instrumentation-tower` 0.19.0 is alpha. It is a Tower layer for Axum and Hyper that emits the `http.server.*` metrics and a server span per request. It aims to set every Required, Conditionally Required and Recommended attribute, and it redacts credential query parameters in `url.query` ([Tower README][tower-readme]). Setting `error.type` on 5xx responses is in the unreleased vNext section of its changelog, not in 0.19.0 ([Tower changelog][tower-changelog]). Its README suggests `myapp.error.type` for domain errors, because the layer overwrites `error.type` on 5xx responses ([Tower README][tower-readme]).

The Rust semantic conventions crate 0.33.1 is generated from semconv v1.42.0, two minor versions behind v1.44.0 ([lib.rs][rust-semconv-lib], [crate changelog][rust-semconv-changelog]).

## Client conventions

### Conventions for browser and app clients

All client-side conventions in v1.44.0 are Development:

- Browser resource (`browser.brands`, `browser.language`, `browser.mobile`, `browser.platform`) plus Stable `user_agent.original` ([browser resource][sc-browser-resource]).
- The `browser.web_vital` event for CLS, FCP, INP, LCP and TTFB ([browser events][sc-browser-events]).
- `session.id`, `session.previous_id`, and the `session.start` and `session.end` events ([session][sc-session]).
- App events `app.screen.click`, `app.widget.click`, `app.crash` and `app.jank` ([app events][sc-app-events]).
- The `device.app.lifecycle` event with `android.app.state` and `ios.app.state` ([mobile events][sc-mobile-events]).
- Device resource (`device.id`, `device.manufacturer`, `device.model.identifier`, `device.model.name`) and OS resource ([device][sc-device], [os][sc-os]).

The browser SIG's own catalog says that only `browser.web_vital` and `exception` exist in the main registry. Earlier proposals for navigation and timing events were closed as stale, and the attributes in `@opentelemetry/browser-instrumentation` serve as working definitions ([browser events catalog][browser-bom]). A new federated registry, `semantic-conventions-client-side`, was created for client conventions. Its only release is `v0.1.0-dev`, and it currently holds browser and enduser models ([client-side registry][cs-readme], [community issue 3594][community-3594]).

### Browser SDK maturity

OpenTelemetry JS 3.0.0 lists tracing, metrics and logs as Stable for both API and SDK. Logs became Stable in this release, and `@opentelemetry/api-logs` folded into `@opentelemetry/api` ([JS README][js-readme], [migration guide][js-migration]). The same README says client instrumentation for the browser "is **experimental** and mostly **unspecified**" ([JS README][js-readme]).

JS 3.0.0 removed `@opentelemetry/sdk-trace-web` and `@opentelemetry/sdk-trace-base` in favour of `@opentelemetry/sdk-trace` ([v3.0.0 release][js-v3]). The classic `instrumentation-fetch` and `instrumentation-xml-http-request` packages are deprecated ([browser README][browser-readme]).

The new `opentelemetry-browser` repository (created 2025-09-11) holds the replacements. Both of its packages are experimental ([browser README][browser-readme]):

- `@opentelemetry/browser-instrumentation` 0.9.0 has event-based instrumentations (console, errors, navigation, navigation timing, resource timing, user actions, web vitals) and span-based ones (fetch, XHR). The fetch instrumentation creates a span per request and propagates trace context to the URLs listed in `propagateTraceHeaderCorsUrls` ([instrumentation README][browser-instr-readme]).
- `@opentelemetry/browser-sdk` 0.6.0 sets up logs and traces. Metrics are out of scope for now ([browser SDK README][browser-sdk-readme]).

The browser SIG prefers logs (events) over long spans in the browser, and says to "prefer clearly reported logs" when it is hard to say when a span starts or ends ([browser events catalog][browser-bom]).

### React Native SDK maturity

No OpenTelemetry SIG ships a React Native SDK. The JS README says "React Native and Expo apps use the browser implementations". This needs Metro 0.81.3 or later, which comes with React Native 0.79 and later ([JS README][js-readme]). The section arrived in 3.0.0, with the switch from the `browser` field to `#platform` imports ([migration guide][js-migration]).

Vendor packages wrap OpenTelemetry for React Native. They are listed with their own maturity claims:

| Package | Version (date) | Notes |
| --- | --- | --- |
| `@honeycombio/opentelemetry-react-native` | 0.8.0 (2026-03-19) | "this library is experimental", needs Metro package exports ([README][honeycomb-readme]) |
| `@embrace-io/react-native` | 7.2.0 (2026-10-08) | Its OTLP add-on `@embrace-io/react-native-otlp` is deprecated in favour of OTLP export from the native SDKs ([npm][npm-embrace-otlp]) |
| `@splunk/otel-react-native` | 1.4.0 (2026-10-26) | Splunk distribution ([npm][npm-splunk]) |
| `@grafana/faro-react-native` | 1.5.0 (2026-10-28) | Faro RUM SDK ([npm][npm-faro-rn]) |

None of these four npm READMEs mention tvOS, Apple TV or Android TV (text search of each README on 2026-10-29). Embrace now tells users to start its SDK in native code, and Faro depends on the PLCrashReporter pod, so both include native iOS code ([npm][npm-embrace-otlp], [npm][npm-faro-rn]). Whether they build for react-native-tvos is unknown until tested.

## Names that no convention covers

### What the conventions say

The naming page gives these rules for application developers ([naming][sc-naming]):

- Check the existing conventions first.
- For company names that may be used outside the company, prefix with the company's reverse domain, for example `com.acme.shopname`.
- For names used inside one application, prefix with the application name if it is unique, for example `myuniquemapapp.longitude`, and make sure it does not clash with an existing namespace.
- Do not use an existing OpenTelemetry namespace as the prefix for your own names.
- Names are lowercase, dot-separated namespaces with snake_case words, such as `http.response.status_code`.
- `otel.*` is reserved.

The metric rules on the same page also apply to custom metrics. Durations are `{operation}.duration` histograms. Do not use `_total`. Do not pluralize UpDownCounter names (`cicd.pipeline.run.active`, not `active_runs`). Pluralize counters only when the unit is a count, such as `{fault}` ([naming][sc-naming]).

Any project may publish its own registry that depends on the OpenTelemetry registry. A `manifest.yaml` names the dependency, and Weaver resolves and packages it ([OTEP 4815][otep-4815]). Weaver checks registries, generates code and docs from templates, and runs a "live-check" that compares emitted telemetry against the schema ([Weaver README][weaver-readme]). The client-side registry above uses the same model ([client-side registry][cs-readme]).

### Reuse before inventing

These existing attributes cover parts of playback and library work:

| Need | Existing name | Stability | Source |
| --- | --- | --- | --- |
| Which user | `user.id` | Development | [user registry][sc-reg-user] |
| Client session | `session.id` | Development | [session][sc-session] |
| Media file on disk | `file.path`, `file.size`, `file.extension` | Development | [file registry][sc-reg-file] |
| Quick Sync load | `hw.gpu.utilization` with `hw.gpu.task` = `encoder` or `decoder` | Development | [GPU][sc-gpu], [hardware registry][sc-reg-hw] |
| ffmpeg child process | `process.*` metrics and resource | Release Candidate | [process metrics][sc-process-metrics] |
| Failure class | `error.type` | Stable | [recording errors][sc-recording-errors] |
| Segment and API requests | HTTP span and metrics | Stable | [HTTP spans][sc-http-spans] |

### Recommended option

Use `selfless.*` as the prefix for every name that no convention covers. "selfless" is not an OpenTelemetry namespace in v1.44.0 ([registry index][sc-registry]). It follows the application-name rule. A reverse-domain prefix is the rule for names that other companies' software may also emit, which does not fit names that only selfless emits.

Define those names in a Weaver registry in the monorepo with a dependency on semconv v1.44.0. Generate Rust and TypeScript constants from it, and run live-check in CI. This keeps one source of truth for server and clients.

The names below are only an illustration of the shape. A later decision picks the real list.

| Kind | Example name | Notes |
| --- | --- | --- |
| Attribute | `selfless.media.type` | `movie` or `episode` |
| Attribute | `selfless.media.id` | Spans and events only, too many values for metrics |
| Attribute | `selfless.playback.method` | for example `direct_play` or `transcode` |
| Event | `selfless.playback.start`, `selfless.playback.stop` | Log records, not one span per viewing |
| Metric | `selfless.playback.session.active` | UpDownCounter, `{session}` |
| Metric | `selfless.transcode.duration` | Histogram, `s` |
| Metric | `selfless.library.import.duration` | Histogram, `s`, for webhook-driven imports |

A viewing can last hours. The browser SIG's advice to prefer events over long spans fits this case on every client ([browser events catalog][browser-bom]).

## Open questions

1. **Database.** If SQLite is chosen, `db.system.name=sqlite` is Development. Is that acceptable, or does the database choice weigh this?
2. **Database spans.** Where do they come from? Options are spans in each repository adapter, or a contribution to Toasty that adds `db.*` attributes to its `query` span.
3. **Tracing API.** Use `tracing` with `tracing-opentelemetry`, the OpenTelemetry API directly, or both? A spike must confirm that `tracing` spans nest under the server span that `opentelemetry-instrumentation-tower` starts. No source checked here answers that.
4. **Alpha Tower layer.** Is the alpha `opentelemetry-instrumentation-tower` acceptable, or should selfless write its own HTTP layer against the stable HTTP conventions?
5. **Process metrics.** Use `opentelemetry-instrumentation-process` (Linux only, two instrument types differ from the conventions), write a small collector, or let an OpenTelemetry Collector scrape host and process metrics?
6. **Tokio metrics.** Export Tokio runtime metrics under `tokio.*`, and accept the `tokio_unstable` flag for some of them?
7. **Browser stack.** Use the experimental `@opentelemetry/browser-sdk` and `@opentelemetry/browser-instrumentation`, or the stable JS 3.0.0 core with hand-written fetch spans?
8. **TV clients.** Does the OpenTelemetry JS browser build run on react-native-tvos for Apple TV and Google TV with the Metro version Expo ships? This needs a prototype. Are vendor SDKs out of scope, given that they target their own backends?
9. **Custom prefix.** Confirm `selfless.*` over a reverse-domain prefix, and decide whether the Weaver registry is part of the foundation.
10. **Playback signal.** Events, spans, metrics, or a mix? What is the cardinality budget, for example user and media IDs on events only?
11. **Personal data.** Which user attributes (`user.id` only, or names) may leave the server, and does client telemetry go to the server or straight to a collector?
12. **Backend.** Which collector and backend receive OTLP on the homelab? That decides whether Prometheus-style limits shape metric names and labels.

## Sources

[sc-releases]: https://github.com/open-telemetry/semantic-conventions/releases
[sc-changelog]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/CHANGELOG.md
[sc-http-readme]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/http/README.md
[sc-http-spans]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/http/http-spans.md
[sc-http-metrics]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/http/http-metrics.md
[sc-db-readme]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/db/README.md
[sc-db-spans]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/db/database-spans.md
[sc-db-metrics]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/db/database-metrics.md
[sc-db-sql]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/db/sql.md
[sc-reg-db]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/registry/attributes/db.md
[sc-process-metrics]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/system/process-metrics.md
[sc-process-resource]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/resource/process.md
[sc-runtime]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/runtime/README.md
[sc-service]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/resource/service.md
[sc-deployment]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/resource/deployment-environment.md
[sc-host]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/resource/host.md
[sc-os]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/resource/os.md
[sc-device]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/resource/device.md
[sc-exceptions-spans]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/exceptions/exceptions-spans.md
[sc-recording-errors]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/general/recording-errors.md
[sc-naming]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/general/naming.md
[sc-browser-resource]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/resource/browser.md
[sc-browser-events]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/browser/browser-events.md
[sc-session]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/general/session.md
[sc-app-events]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/app/app-events.md
[sc-mobile-events]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/mobile/mobile-events.md
[sc-gpu]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/hardware/gpu.md
[sc-registry]: https://github.com/open-telemetry/semantic-conventions/tree/v1.44.0/docs/registry/attributes
[sc-reg-user]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/registry/attributes/user.md
[sc-reg-file]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/registry/attributes/file.md
[sc-reg-hw]: https://github.com/open-telemetry/semantic-conventions/blob/v1.44.0/docs/registry/attributes/hardware.md
[spec-maturity]: https://github.com/open-telemetry/opentelemetry-specification/blob/36ccfcfb33aca1e9c70a2cfd26e031ed9180949a/specification/maturity-levels.md
[otep-4815]: https://github.com/open-telemetry/opentelemetry-specification/blob/36ccfcfb33aca1e9c70a2cfd26e031ed9180949a/oteps/4815-semantic-conventions-schema-v2.md
[rust-readme]: https://github.com/open-telemetry/opentelemetry-rust/blob/opentelemetry-0.33.1/README.md
[rust-semconv-changelog]: https://github.com/open-telemetry/opentelemetry-rust/blob/opentelemetry-0.33.1/opentelemetry-semantic-conventions/CHANGELOG.md
[rust-semconv-lib]: https://github.com/open-telemetry/opentelemetry-rust/blob/opentelemetry-0.33.1/opentelemetry-semantic-conventions/src/lib.rs
[rust-semconv-metric]: https://github.com/open-telemetry/opentelemetry-rust/blob/opentelemetry-0.33.1/opentelemetry-semantic-conventions/src/metric.rs
[crate-otel]: https://crates.io/crates/opentelemetry
[crate-process]: https://crates.io/crates/opentelemetry-instrumentation-process
[crate-toasty]: https://crates.io/crates/toasty
[to-lib]: https://github.com/tokio-rs/tracing-opentelemetry/blob/8c1a02d68034de317607ada0b1dd0d98abefd6cd/src/lib.rs
[to-changelog]: https://github.com/tokio-rs/tracing-opentelemetry/blob/8c1a02d68034de317607ada0b1dd0d98abefd6cd/CHANGELOG.md
[tower-readme]: https://github.com/open-telemetry/opentelemetry-rust-contrib/blob/e3568092ae76cf62641f3c6c78d057adb916561c/opentelemetry-instrumentation-tower/README.md
[tower-changelog]: https://github.com/open-telemetry/opentelemetry-rust-contrib/blob/e3568092ae76cf62641f3c6c78d057adb916561c/opentelemetry-instrumentation-tower/CHANGELOG.md
[process-readme]: https://github.com/sandhose/opentelemetry-instrumentation-process/blob/c4139aa87d929e0193bfaa1d0de516e676a3d31b/README.md
[process-src]: https://github.com/sandhose/opentelemetry-instrumentation-process/blob/c4139aa87d929e0193bfaa1d0de516e676a3d31b/src/linux.rs
[tokio-metrics]: https://github.com/tokio-rs/tokio-metrics/blob/a504ad8447aba8c2e7b3055921c34e86cebd7ae9/README.md
[toasty-sqlite]: https://github.com/tokio-rs/toasty/blob/41f569a8107df2d7e24f8e8e044061a316a13d24/crates/toasty-driver-sqlite/src/lib.rs
[toasty-instrument]: https://github.com/tokio-rs/toasty/blob/41f569a8107df2d7e24f8e8e044061a316a13d24/crates/toasty/src/instrument.rs
[js-readme]: https://github.com/open-telemetry/opentelemetry-js/blob/v3.0.0/README.md
[js-v3]: https://github.com/open-telemetry/opentelemetry-js/releases/tag/v3.0.0
[js-migration]: https://github.com/open-telemetry/opentelemetry-js/blob/v3.0.0/doc/3.x/migration-guide.md
[npm-semconv]: https://www.npmjs.com/package/@opentelemetry/semantic-conventions
[npm-browser-sdk]: https://www.npmjs.com/package/@opentelemetry/browser-sdk
[npm-browser-instr]: https://www.npmjs.com/package/@opentelemetry/browser-instrumentation
[browser-readme]: https://github.com/open-telemetry/opentelemetry-browser/blob/4807f3fe2a6379b67393a04afec88c6d2a49595d/README.md
[browser-bom]: https://github.com/open-telemetry/opentelemetry-browser/blob/4807f3fe2a6379b67393a04afec88c6d2a49595d/docs/browser-observability-model.md
[browser-instr-readme]: https://github.com/open-telemetry/opentelemetry-browser/blob/4807f3fe2a6379b67393a04afec88c6d2a49595d/packages/instrumentation/README.md
[browser-sdk-readme]: https://github.com/open-telemetry/opentelemetry-browser/blob/4807f3fe2a6379b67393a04afec88c6d2a49595d/packages/sdk/README.md
[cs-readme]: https://github.com/open-telemetry/semantic-conventions-client-side/blob/ce50ec3287a32cc3ed781a3177497981a81020ff/README.md
[community-3594]: https://github.com/open-telemetry/community/issues/3594
[honeycomb-readme]: https://github.com/honeycombio/honeycomb-opentelemetry-react-native/blob/f385b366d3b24912aac5e09d273c5db0487b51d2/README.md
[npm-embrace-otlp]: https://www.npmjs.com/package/@embrace-io/react-native-otlp
[npm-splunk]: https://www.npmjs.com/package/@splunk/otel-react-native
[npm-faro-rn]: https://www.npmjs.com/package/@grafana/faro-react-native
[weaver-readme]: https://github.com/open-telemetry/weaver/blob/08408062893ad70392f5f98dcc18fdf38dd2b6e2/README.md
[weaver-releases]: https://github.com/open-telemetry/weaver/releases

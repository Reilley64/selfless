# Clean Architecture reference

Quotes are verified against Robert Martin, "The Clean Architecture" (2012, blog.cleancoder.com). Cite them; do not paraphrase them into new rules.

| Rule | Martin | In this repo |
| --- | --- | --- |
| Dependency Rule | "Source code dependencies always point inwards." The circles are schematic; this is the only hard rule. | `domain` <- `application` <- infrastructure and presentation crates <- `selfless` (`src/`). `deny.toml` wrappers enforce crate edges. The web and TV apps are presentation too (ADR 0001). |
| Entities | Enterprise-wide critical business rules. | `domain`: values and Toasty models (ADR 0001), one file per type. |
| Use cases | "contain application specific business rules ... orchestrate the flow of data to and from the entities, and direct those entities to use their enterprise wide business rules". "changes to the operation of the application will affect the use-cases". | `application/src/<capability>/<use_case>.rs` with `Dependencies`, `Output`, `Error`. |
| Interface adapters | "convert data from the format most convenient for the use cases and entities, to the format most convenient for some external agency such as the Database or the Web." | Presentation crates (controllers, views, workers, jobs) and infrastructure adapters (gateways). |
| Main | The Main component builds factories and strategies and hands control inward. | `selfless` `main` and `selfless::start`, which build `AppState` (`infrastructure/dependencies`). |
| Data crossing boundaries | "isolated, simple, data structures ... We don't want to cheat and pass Entities or Database rows." | Use case `Output` types. |

Common guidance, not a Martin quote; label it as such when you cite it:

- A controller calls one use case.
- A use case does not call another public use case.

## Placement tests

- **Entity rule**: a business fact the glossary or a living spec names, true regardless of which use case runs it (allowed state changes, money math, limits, pricing). Belongs in `domain`.
- **Use case rule**: the operation of the application (which merchants a batch covers, what happens after a failure, idempotent redelivery flow, locks, transactions, notifications, ownership and authorization checks, provider choice). Belongs in `application`.
- **Adapter work**: format conversion and transport. Ask "would this change if the delivery mechanism changed?" If yes, it belongs in the adapter; if no, it has leaked out of `application` or `domain`.

## Sanctioned deviations

- ADR 0001: Toasty 0.11 models are the domain types; application owns `DbContext`, `DbSet<T>` roots and `DbTransaction`, and holds the hand-written SQL Toasty cannot express, so selfless supports one database.
- ADR 0006: database spans and metrics come from the application-owned `DbContext`, and HTTP spans from a selfless topcoat middleware. The Telemetry conventions page also lets application use OpenTelemetry trace context directly.

Report friction these cause with evidence; do not propose reversing them without it. A deviation that only `CODING_STANDARDS.md`, a `deny.toml` exception or nothing at all allows, with no ADR, is unrecorded: list it as such. `#[fundle::deps]` on `Dependencies` with one `AppState` has no selfless ADR; check whether one covers it before calling it sanctioned.

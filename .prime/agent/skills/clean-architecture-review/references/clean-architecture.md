# Clean Architecture reference

Quotes are verbatim from Robert C. Martin, *Clean Architecture* (2017). The EPUB has no page numbers, so cite the chapter. Cite a quote as evidence for a finding. Do not turn a paraphrase into a new rule.

The rules a reviewer checks against one diff are in `CODING_STANDARDS.md`. This file holds the book's grounding for them and the checks that need the whole codebase.

| Rule | Martin | In this repo |
| --- | --- | --- |
| Dependency Rule | "Nothing in an inner circle can know anything at all about something in an outer circle." (Ch. 22). This covers data formats as well as names. | `domain` <- `application` <- infrastructure and presentation crates <- `selfless` (`src/`). `deny.toml` wrappers enforce crate edges. The web and TV apps are presentation too (ADR 0001). `domain` and `application` derive no wire format. |
| Entities | "No operational change to any particular application should affect the entity layer." (Ch. 22) | `domain`: values and Toasty models (ADR 0001), one file per type. |
| Use cases | "Use cases control the dance of the Entities." (Ch. 20) | `application/src/<capability>/<use_case>.rs` with `Dependencies`, `Output`, `Error`. |
| Interface adapters | "convert data from the format most convenient for the use cases and entities, to the format most convenient for some external agency such as the database or the web." (Ch. 22) | Presentation crates (routes, admin pages, workers, webhooks) and infrastructure adapters. |
| Humble Object | "One of those modules is humble; it contains all the hard-to-test behaviors stripped down to their barest essence." (Ch. 23) | A route, page, webhook handler or worker loop calls one use case and only maps input and output. |
| Ports owned by the caller | "The API is defined and owned by the user, rather than by the implementer." (Ch. 25) | Application owns each port, named for what the use case needs, and an infrastructure crate builds it. |
| Data crossing boundaries | "We don't want to cheat and pass Entity objects or database rows." (Ch. 22) | Use case `Output` types carry plain data. |
| Main | "Think of Main as the dirtiest of all the dirty components." (Ch. 26). "It is in this Main component that dependencies should be injected by a Dependency Injection framework." (Ch. 26) | `selfless` `main` and `selfless::start`, which build `AppState` (`infrastructure/dependencies`). |
| Frameworks are details | A framework in the inner circles is a marriage: "That's normal—but it should still be a decision." (Ch. 32) | Toasty (ADR 0001) and OpenTelemetry (ADR 0006) are recorded. Any other framework or runtime crate in `domain` or `application` needs an ADR. |
| Partial boundaries | "Without reciprocal interfaces, nothing prevents this kind of backchannel other than the diligence and discipline of the developers and architects." (Ch. 24) | Presentation calls concrete use case `call` functions with no boundary traits. A backchannel is presentation reading `AppState` fields or `application::ports` outside a use case. |
| Accidental duplication | "Resist the temptation to commit the sin of knee-jerk elimination of duplication. Make sure the duplication is real." (Ch. 16) | User and admin flows stay separate while they change for different reasons. |
| Testable | "You shouldn't need the database connected to run your tests." (Ch. 21) | Use case tests take a mocked database. Behaviour only PostgreSQL has gets integration tests. |

A use case does not call another use case's `call`. Vernon grounds this rule (see [domain-driven-design.md](domain-driven-design.md)). A function that two use cases share is fine.

## Placement tests

- **Entity rule**: a business fact the glossary or a living spec names, true whichever screen, flow or delivery runs it (allowed state changes, expiries, limits). It belongs in `domain`, on the type whose data it uses. A rule over several models with no I/O is a domain function (Vernon, p. 267).
- **Use case rule**: how and when the application runs entity rules, such as idempotent redelivery, locks, transactions, notifications, ownership and authorization checks, or the choice of provider. It belongs in `application`.
- **Adapter work**: format conversion, transport, schedules and retry pauses. Martin defines level as "the distance from the inputs and outputs." (Ch. 19). Ask "would this change if the delivery mechanism changed?" If yes, it belongs in the adapter. If no, it has leaked out of `application` or `domain`.

## Crate metrics

Martin's component principles (Ch. 13 and 14) apply to Cargo crates. Measure them in step 1 from the normal dependencies between workspace crates.

- **Instability** I = fan-out / (fan-in + fan-out). "The SDP says that the I metric of a component should be larger than the I metrics of the components that it depends on." (Ch. 14). Check every edge.
- **Abstractness** A = abstract public items / all public items. Count a `pub trait` or a port newtype over `Arc<dyn Fn>` as abstract. "A component should be as abstract as it is stable." (Ch. 14)
- **Distance** D = |A + I − 1|. Flag a volatile crate far from the main sequence. `domain` sits in the Zone of Pain (stable and concrete) because Toasty models are the schema. ADR 0001 sanctions that, so report it without flagging it.
- **Cohesion**: an adapter crate that depends on all of `application` but uses only its ports is a CRP strain. Raise a ports crate only with measured build-time friction, because the Common Closure Principle outweighs reuse early in a project (Ch. 13).
- Cargo rejects cycles between normal dependencies, so the Acyclic Dependencies Principle needs no check. Every crate has `publish = false`, so the Reuse/Release Equivalence Principle does not apply.

## Sanctioned deviations

- ADR 0001: Toasty 0.11 models are the domain types. Application owns `DbContext`, `DbSet<T>` roots and `DbTransaction`, and holds the hand-written SQL Toasty cannot express, so selfless supports one database. Martin puts SQL and the data mapper in the database adapter (Ch. 23, Ch. 30). ADR 0003 records the cost he predicts: SQLite later means rewriting every hand-written query.
- ADR 0006: database spans and metrics come from the application-owned `DbContext`, and HTTP spans from a selfless topcoat middleware. The Telemetry conventions page also lets application use OpenTelemetry trace context directly.

Report friction these cause with evidence, and propose reversing them only when the evidence supports it. A deviation that only `CODING_STANDARDS.md`, a `deny.toml` exception or nothing at all allows, with no ADR, is unrecorded: list it as such. `#[fundle::deps]` on every use case's `Dependencies` puts a dependency-injection framework in `application`, where Ch. 26 and Ch. 32 put it in Main ("you should not sprinkle @autowired annotations all throughout your business objects"). No ADR covers it, so list it as unrecorded until one does. Check `domain/Cargo.toml` and `application/Cargo.toml` for any other framework or runtime crate the same way.

## Vocabulary

- **Policy and detail**: policy is the business rules; details (I/O, database, web, frameworks) let the world reach the policy without changing it (Ch. 15).
- **Actor**: the group a module answers to. One actor per module (Ch. 7).
- **Humble Object**: the hard-to-test half of a split, kept thin (Ch. 23).
- **Partial boundary** and **backchannel**: a boundary built only in part, and a dependency that skips it (Ch. 24).
- **Main component**: builds factories, strategies and configuration, then hands control inward (Ch. 26).
- **True and accidental duplication**: true copies change together; accidental ones only look alike (Ch. 16).
- **Fan-in, fan-out, I, A, D, main sequence, Zone of Pain, Zone of Uselessness**: Ch. 14.
- **Screaming architecture**: a top level that shows the use cases, not the frameworks (Ch. 21).
- **Testing API**: crates with special powers that keep tests off the application's structure (Ch. 28).

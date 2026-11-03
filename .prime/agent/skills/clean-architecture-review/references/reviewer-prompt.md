# Layer reviewer prompt

Fill the placeholders and spawn one reviewer per layer. Keep the reviewer read-only.

```text
You are reviewing the Rust repo at <repo> for how well it follows Clean Architecture. READ-ONLY: do not edit, create, or delete any file in the repo. Read-only commands (grep, cargo tree, cargo metadata) are fine.

Area: <layer name>
Scope: <paths>

First read: AGENTS.md, CODING_STANDARDS.md, GLOSSARY.md, docs/agents/*.md, and the ADR pages saved in <run dir>/adr/. A finding that breaks a CODING_STANDARDS.md line cites that line as well as the book. ADRs 0001 and 0006 are sanctioned deviations: record them as sanctioned and only call them problems with concrete friction.

<paste references/clean-architecture.md>

<paste references/domain-driven-design.md>

Focus questions:
<layer focus questions from the list below>

Explore organically. Every finding needs file:line evidence and a short excerpt or count. Prefer patterns that recur, with how many places. Record healthy patterns with evidence too. Do not invent problems or speculate about future features.

Write Markdown to <run dir>/<layer>.md:
# <area>
## Summary (3-5 bullets)
## Metrics (counts you measured)
## Findings
### <PREFIX-n>: <title>
- Principle: <rule name and book citation>
- Severity: High | Medium | Low
- Sanctioned by ADR: <number or "no">
- Occurrences: <count + up to 8 file:line>
- Evidence: <short excerpt>
- Why it matters here: <1-3 concrete sentences>
- Recommendation: <1-2 plain sentences>
## Healthy patterns (with evidence)

Plain short sentences. Your final message names the file.
```

## Focus questions per layer

**Domain** (prefix DOM)
- Does `domain` depend on anything outer? Does Toasty leak further than ADR 0001 allows (`codegen_support`, unchecked `Deferred::get` in business methods)?
- Rich or anemic: which critical rules live in `domain`, and which are restated in `application` (state checks, transitions, expiries, limits)? Run audit check 5 in the DDD reference.
- Are business values validated value objects with a private field and a validating `new`, or plain `String` and integers?
- How many plain `Uuid` fields and parameters name different roots (DDD audit check 2)?
- Which model fields are not facts the glossary can name (DDD audit check 4)?
- Do type and module names match `GLOSSARY.md`, including its _Avoid_ lists?
- Any I/O, clock, randomness, tracing, async, or wire format such as serde?

**Application use cases** (prefix APP)
- Do use cases only orchestrate, or hold entity rules? Give line counts of the largest and what fills them.
- Does any use case call another use case's `call`?
- List every transaction that changes more than one root, with the invariant that needs it (DDD audit check 1).
- Does each `Dependencies` hold only the ports the use case calls?
- Framework leakage beyond the ADRs: drivers, SQL, HTTP, provider SDK types, runtime specifics.
- Do `Output` types return entities, Toasty models, projections, or port types?
- Duplication across use cases.

**Application ports and the rest** (prefix APP)
- Ports with no use case caller, ports shaped by one vendor's API, `String`-typed identifiers.
- Does every `DbSet` on the `DbContext` belong to a root model (DDD audit check 3)?
- Every `domain/Cargo.toml` and `application/Cargo.toml` dependency classified as sanctioned (name the ADR), utility, or leak. A framework or runtime crate with no ADR is a leak.
- Unused use cases, shallow pass-through use cases and their caller counts.
- Where error markers live and whether presentation can map without string matching.

**Infrastructure and composition root** (prefix INF)
- Every workspace crate edge; flag outward or layer-skipping edges and stale `deny.toml` wrappers.
- Business decisions inside adapters that the use case cannot see.
- Vendor types crossing a port; SDK clients reachable from presentation. Does each adapter for Radarr, Sonarr, NFO files or SMTP translate into domain types (anticorruption layer)?
- Adapter crates that depend on all of `application` but use only its ports (crate cohesion).
- Is wiring in one place? Does every process build only what it uses?

**Server presentation** (prefix PRE)
- Entry points that call more than one use case or hold business policy (failure handling, scoping, defaults). Count all entry points.
- Direct port, `DbContext`, adapter-crate, settings or `AppState` field access that bypasses use cases (backchannels). Count each.
- Inbox and outbox handlers that would give a different result if they ran twice or out of order.
- Views and DTOs built from entities or Toasty models instead of use case outputs.
- Error mapping: markers versus string matching; any error text shown to clients.

**Apps** (prefix UI)
- Does the web or TV app hold a business rule the server should own (session expiry, who may do what, name or password rules)?
- Do screens call the generated API client directly, or go through the shared session module and hooks? Count each.
- Shared versus copied logic between `web`, `tv` and `ui`; where the TV and web differ only by storage or client kind.
- Does anything outside `api-client` build URLs, headers or tokens by hand?
- Is platform-specific code (tvOS, Android TV, browser) behind one substitution point, or spread across screens as checks?

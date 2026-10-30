# Layer reviewer prompt

Fill the placeholders and spawn one reviewer per layer. Keep the reviewer read-only.

```text
You are reviewing the Rust repo at <repo> for how well it follows Clean Architecture. READ-ONLY: do not edit, create, or delete any file in the repo. Read-only commands (grep, cargo tree, cargo metadata) are fine.

Area: <layer name>
Scope: <paths>

First read: AGENTS.md, CODING_STANDARDS.md, GLOSSARY.md, docs/agents/*.md, and the ADR pages saved in <run dir>/adr/. ADRs 0001 and 0006 are sanctioned deviations: record them as sanctioned and only call them problems with concrete friction.

<paste references/clean-architecture.md>

Focus questions:
<layer focus questions from the list below>

Explore organically. Every finding needs file:line evidence and a short excerpt or count. Prefer patterns that recur, with how many places. Record healthy patterns with evidence too. Do not invent problems or speculate about future features.

Write Markdown to <run dir>/<layer>.md:
# <area>
## Summary (3-5 bullets)
## Metrics (counts you measured)
## Findings
### <PREFIX-n>: <title>
- Principle:
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
- Rich or anemic: which critical rules live in `domain`, and which are restated in `application` (state checks, transitions, money math, limits, pricing)?
- Are business values validated value objects, or plain `String` and integers?
- Any I/O, clock, randomness, tracing, or async?

**Application use cases** (prefix APP)
- Do use cases only orchestrate, or hold entity rules? Give line counts of the largest and what fills them.
- Does any use case call another public use case?
- Framework leakage beyond the ADRs: drivers, SQL, HTTP, provider SDK types, runtime specifics.
- Do `Output` types return entities, Toasty models, projections, or port types?
- Duplication across use cases.

**Application ports and the rest** (prefix APP)
- Ports with no use case caller, ports shaped by one vendor's API, `String`-typed identifiers.
- Every `application/Cargo.toml` dependency classified as sanctioned, utility, or leak.
- Unused use cases, shallow pass-through use cases and their caller counts.
- Where error markers live and whether presentation can map without string matching.

**Infrastructure and composition root** (prefix INF)
- Every workspace crate edge; flag outward or layer-skipping edges and stale `deny.toml` wrappers.
- Business decisions inside adapters that the use case cannot see.
- Vendor types crossing a port; SDK clients reachable from presentation.
- Is wiring in one place? Does every process build only what it uses?

**Server presentation** (prefix PRE)
- Entry points that call more than one use case or hold business policy (failure handling, scoping, defaults). Count all entry points.
- Direct port, `DbContext`, adapter-crate, or settings access that bypasses use cases.
- Views and DTOs built from entities or Toasty models instead of use case outputs.
- Error mapping: markers versus string matching; any error text shown to clients.

**Apps** (prefix UI)
- Does the web or TV app hold a business rule the server should own (session expiry, who may do what, name or password rules)?
- Do screens call the generated API client directly, or go through the shared session module and hooks? Count each.
- Shared versus copied logic between `web`, `tv` and `ui`; where the TV and web differ only by storage or client kind.
- Does anything outside `api-client` build URLs, headers or tokens by hand?

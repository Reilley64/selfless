# Domain-Driven Design reference

Quotes are verbatim from Vaughn Vernon, *Implementing Domain-Driven Design* (2013). Pages are the printed page numbers. Cite a quote as evidence for a finding. Do not turn a paraphrase into a new rule.

The rules a reviewer checks against one diff are in `CODING_STANDARDS.md` ("Domain model", "Use cases and adapters"). The rules about language are in `docs/agents/domain.md`. This file holds the book's grounding for them and the checks that need the whole codebase.

ADR 0001 replaces repositories with the application-owned `DbContext` and its `DbSet` roots, and makes Toasty models the domain types. Read every rule below through that ADR.

| Rule | Vernon | In this repo |
| --- | --- | --- |
| Ubiquitous Language | "Both the software and the tests that verify the model's adherence to the tenets of the domain capture and adhere to this Language" (p. 24) | `GLOSSARY.md`. Code names follow it (`docs/agents/domain.md`). |
| One bounded context | "Just keep in mind that technical components don't define the Context." (p. 71) | Crates are layers. The context map is in `docs/agents/domain.md`. |
| Proportion | "Never use DDD to make your solution more complex." (p. 10) | Core and generic parts are named in `docs/agents/domain.md`. |
| Anticorruption layer | "As a downstream client, create an isolating layer to provide your system with functionality of the upstream system in terms of your own domain model." (p. 93) | The crate that talks to Radarr, Sonarr, NFO files or SMTP translates their shapes. ADR 0007 keeps Radarr and Sonarr IDs only for API calls. |
| Thin use cases | "Keep Application Services thin, using them only to coordinate tasks on the model." (p. 521) | A use case loads, calls domain methods and saves. It owns the transaction and the authorization check. |
| Domain functions | "We don't want to house business logic in an Application Service, but we do want business logic housed in a Domain Service." (p. 267) | A rule over several models with no I/O is a function in `domain`. |
| Tell, Don't Ask | "modification of the Aggregate state belongs to the Aggregate, not the client." (p. 384) | Toasty builders set fields from outside the type, so a domain method returns the value to set and the use case passes it to the builder (ADR 0001). |
| Value objects | "You must not allow the attributes of a Value instance to be populated after construction" (p. 225) | A value type with a rule has a private field and a validating `new`. `toasty::Embed` rebuilds stored values without `new`. That is sanctioned friction, because every write went through `new`. |
| Domain values as inputs | "If you eliminate types from the model, you avoid dependency and coupling, but you lose out on strong type checking and basic validations (guards)" (p. 523) | Use cases take domain values for input they store. Presentation builds them. |
| Standard types | "it is best to be biased toward enum for Standard Types" (p. 238) | A fixed set of kinds is a domain enum, and presentation owns its JSON names. |
| One aggregate per transaction | "And a properly designed Bounded Context modifies only one Aggregate instance per transaction in all cases." (p. 354) | ADR 0001 lets one `DbTransaction` span several `DbSet`s. See the audit checks below. |
| Reference by identity | "Prefer references to external Aggregates only by their globally unique identity, not by holding a direct object reference" (p. 361) | A model holds the other root's ID beside its `Deferred<T>` relation, and changes it only through that root's `DbSet`. |
| Only roots get sets | "Strictly speaking, only Aggregates have Repositories." (p. 402) | A `DbSet` root exists only for a model that code loads and changes on its own. |
| Outbox | "This has the advantage that your model and your Events are guaranteed to be consistent within a single, local transaction." (p. 304) | An outbox row in the same transaction, sent by a worker. The webhook inbox (ADR 0002) is the same rule on the way in. |
| Idempotent handlers | "it is important that the tracking of handled message identity be committed along with any changes to the local domain model state." (p. 331) | ADR 0007 re-syncs one movie or show per webhook instead of applying the payload. |
| Pending states | "Working around eventual consistency is in no way a kludge. It's just another valid state that should be modeled." (p. 108) | States a user or admin can see get glossary terms (`docs/agents/domain.md`). |
| Use cases stay independent | "Do not allow any one handler to depend on (make use of) any others." (p. 144) | A use case never calls another use case's `call`. A function both call is fine. |
| Screen-shaped reads | "you might instead use what is called a use case optimal query" (p. 517) | One use case and one `DbContext` query per screen that combines models, such as Continue watching. |
| Data model serves the domain | "Design your data model for the sake of your domain model, not your domain model for the sake of your data model." (p. 250) | A field that exists only for persistence or request replay is ADR 0001 friction. |
| Concurrent writers | "objects carry a version number that is incremented when changes are made and checked before they are saved to the database." (p. 350) | When two clients can write one model at once, such as a resume position from web and TV, the spec says last write wins or names a version check. |

## Audit checks

Run these across the whole scope. Each one reports evidence. None reports a defect on its own.

1. **Transactions over several roots.** List every use case that changes more than one root in a transaction, with the invariant that needs them together. Report named invariants as justified. A household has one user changing their own data at a time, which Vernon calls user-aggregate affinity (p. 369), so most are safe. Question only those with no invariant, and suggest a worker when keeping the other root in step is the system's job rather than the acting user's (p. 366).
2. **ID types.** Count the plain `Uuid` fields and parameters that name different roots. A distinct ID type per root stops one being passed for another. Vernon warns that plain IDs "would also have opened up the potential for bugs in code where each Identity type could not be distinguished from the others." (p. 343). Raise it as a deepening candidate with the count. No decision covers it yet.
3. **Sets for roots only.** Check that every `DbSet` on the `DbContext` belongs to a root model.
4. **Data model friction.** List domain model fields that are not facts the glossary can name, such as columns kept for request replay. Report them as ADR 0001 friction.
5. **Anemic models.** For each domain type, list the rules about its data that live in use cases instead. Use the placement tests in [clean-architecture.md](clean-architecture.md).
6. **Proportion.** Every deepening candidate names the risk it removes and whether it touches core or generic code (`docs/agents/domain.md`). Mark Speculative a candidate that adds patterns to generic code.

## Book rules selfless does not follow

- Bounded retries and time-outs for long-running processes (p. 493). selfless's invite outbox keeps retrying with no attempt limit and treats only a mailbox refusal as permanent.
- Validation that collects every error before failing (p. 212). ADR 0004 answers with one problem per response, with a stable `type`.
- Two-phase commit, message brokers, event sourcing and REST notification logs (Ch. 4, 8, 13). selfless is one process with one PostgreSQL database (ADR 0002, ADR 0003).

## Vocabulary

- **Bounded context**: the boundary inside which each term has one meaning (p. 62).
- **Core, supporting and generic subdomains**: the reason for the product, the parts specific to it, and the ordinary parts (p. 52).
- **Upstream and downstream; Conformist; Anticorruption layer**: pp. 88-93.
- **Entity; value object; standard type; guard**: Ch. 5 and 6.
- **Domain service; application service**: p. 267, p. 521. In selfless an application service is a use case's `call`.
- **Aggregate and root; true invariant; consistency boundary; eventual consistency**: Ch. 10.
- **Anemic domain model**: domain types that only hold data, with their rules in callers (p. 13).
- **Domain event; idempotent receiver**: Ch. 8. In selfless an outbox item is a queued request, not a domain event.
- **Use case optimal query**: p. 517.

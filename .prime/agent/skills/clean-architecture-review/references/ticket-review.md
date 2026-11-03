# Ticket review

Spawn a fresh read-only reviewer with the ticket bodies (saved as files in the run directory), `decisions.md`, the layer notes, any fact-finding output, `CODING_STANDARDS.md`, `references/clean-architecture.md` and `references/domain-driven-design.md`. Ask it to write `<run dir>/ticket-review.md` with a verdict per ticket (Ready, Ready with edits, Needs rework) and numbered findings with severity, evidence, and a concrete edit.

## Checklist for the reviewer

1. **Placement**: each moved rule lands in the right layer by the placement tests. Ownership checks, provider choice, locks, transactions, and notifications stay in `application`.
2. **Dependency Rule**: no new outward dependency; `domain` gains no clock, ID generation, logging, or I/O.
3. **Seams**: domain inputs and outputs are projections and plain values; no Toasty load state, port types, or provider details cross them.
4. **Depth and proportion**: each new domain method has real behavior behind a small interface; flag one-line pass-throughs and seams with a single hypothetical adapter. Each new pattern, port or layer names the risk it removes, and generic code stays plain (`docs/agents/domain.md`).
5. **Completeness**: every decision a use case makes today has a home, including checks that run before an external call; blocking edges order every pair of tickets that edit the same file; each ticket says which use case tests stay.
6. **Hygiene**: each ticket uses the structure in `docs/agents/issue-tracker.md`; a refactor ticket really changes no behavior; a behavior-changing ticket edits the spec that states the rule; a ticket that sets a lasting rule adds it to `CODING_STANDARDS.md`, `docs/agents/domain.md` or an ADR, as `decisions.md` says.
7. **Accuracy**: states, markers, and type shapes match the code and the specs.

After edits, re-run the reviewer with the earlier review and the new bodies, asking it to confirm each High and Medium finding is resolved and that the edited tickets agree with each other.

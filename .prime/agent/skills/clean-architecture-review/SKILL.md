---
name: clean-architecture-review
description: Review this repo against Clean Architecture and Domain-Driven Design, render an HTML report, grill a chosen candidate, then publish and review tickets for it.
disable-model-invocation: true
---

# Clean Architecture review

Audit where rules live in this workspace, measured against Robert C. Martin's *Clean Architecture*, Vaughn Vernon's *Implementing Domain-Driven Design*, `CODING_STANDARDS.md` and this repo's ADRs. The output is a self-contained HTML report, then, for the candidate the user picks, a grilled design, published tickets, a ticket review, and an implementation handoff.

Use the vocabulary in [references/clean-architecture.md](references/clean-architecture.md) and [references/domain-driven-design.md](references/domain-driven-design.md) for every finding, and cite their quotes. Use the `codebase-design` vocabulary (module, interface, depth, seam, adapter, leverage, locality) for candidate cards.

## 1. Scope and context

- Take the area the user names; otherwise review the whole workspace.
- Read `AGENTS.md`, `CODING_STANDARDS.md`, `GLOSSARY.md`, `docs/agents/*.md`, and every ADR. ADRs are Plane pages, not files: list them with the Plane MCP `page` tool as `docs/agents/domain.md` describes, and save each page as Markdown in the run directory so reviewers can read it. ADRs 0001 and 0006 are **sanctioned deviations**: report them as sanctioned, and raise one only with concrete friction it causes.
- Map the crate graph with `cargo metadata --format-version 1 --no-deps` (normal dependencies between workspace crates) and read the wrapper rules in `deny.toml`.
- Compute fan-in, fan-out and instability for every workspace crate, and estimate abstractness, as "Crate metrics" in [references/clean-architecture.md](references/clean-architecture.md) describes. List every edge that points to a higher instability.

Done when you hold the crate edge list with each crate's metrics, the sanctioned deviations, and the scoped guides for the area.

## 2. Fan out layer reviewers

Spawn one read-only reviewer per layer in parallel, using the template in [references/reviewer-prompt.md](references/reviewer-prompt.md). For a whole-workspace run the layers are: domain; application use cases (`application/src/<capability>/`); application ports and the rest of application; infrastructure with the composition root (`src/`, `infrastructure/`); the server presentation crates (`presentation/api`, `admin`, `workers`, `webhooks`); and the apps (`presentation/web`, `tv`, `ui`, `api-client`), which ADR 0001 also counts as presentation. Each writes a Markdown file to a run directory under `$TMPDIR` and replies with its path.

Done when every reviewer file exists and each finding carries an ID, severity, sanction status, occurrences, and file:line evidence.

## 3. Verify before you report

Re-check by hand every High finding and every finding that could lose money, data, or security (for example an unvalidated value rendered into a page, a queue row that retries forever, an event the domain later rejects). Reconcile counts that disagree between reviewers. Drop or soften any claim you cannot reproduce.

Done when every claim the report states as fact is one you or a reviewer showed with code.

## 4. Render the report

Build the HTML report described in [references/report.md](references/report.md), write it to `$TMPDIR/architecture-review-<timestamp>.html`, check its layout with a headless screenshot, open it, and give the user its path.

Then ask: "Which of these would you like to explore?"

## 5. Grill the chosen candidate

Run the `grilling` skill on the candidate, in rounds over the **frontier**.

- Dispatch a fact-finding subagent before any question whose answer depends on code facts (for example a full transition matrix: every state change the domain accepts, every site that writes it, guards, no-op states, markers, and covering tests). Ask the rest of the frontier while it runs.
- Before recommending any change that narrows or widens behavior, read the affected spec and tickets in Plane (`docs/agents/issue-tracker.md`). A rule a spec states is a spec change, never "no behavior change".
- Record every settled decision in a `decisions.md` file in the run directory as it lands.
- Update `GLOSSARY.md` the moment a term is resolved, following the `domain-modeling` skill.
- Give each decision that sets a lasting rule a home, and name it in `decisions.md`. A rule a reviewer can check against one diff goes in `CODING_STANDARDS.md`. A rule about domain language goes in `docs/agents/domain.md`. A hard-to-reverse trade-off goes in an ADR. The ticket that lands the code also lands the line.

Done when the frontier is empty and the user confirms the summarised plan.

## 6. Publish tickets

Run the `to-tickets` skill, following `docs/agents/issue-tracker.md`. Every ticket restates the shared rules agreed in step 5 so it stands alone, uses the ticket structure that doc defines, and has native blocking edges. Bugs found during the review become their own tickets; a refactor ticket changes no behavior. When a ticket changes behavior a spec states, edit that spec as the doc says.

## 7. Review the tickets

Spawn a fresh read-only reviewer with the checklist in [references/ticket-review.md](references/ticket-review.md). Present its verdicts, put each decision it raises to the user with your recommendation, apply the accepted edits to the issues and edges, then re-run the reviewer on the changed tickets.

Done when every ticket is Ready and no High or Medium finding is open.

## 8. Hand off

On request, write an implementation handoff with [references/handoff.md](references/handoff.md) and save it in the run directory.

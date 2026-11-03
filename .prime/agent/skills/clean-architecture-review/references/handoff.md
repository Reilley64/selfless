# Implementation handoff

A handoff is a prompt another agent can run cold. Write it to `<run dir>/handoff-<topic>.md` and show it to the user.

Include, in order:

1. **Goal**: the tracker, the ticket set, and what is out of scope.
2. **Tickets and order**: a table of work item, title, and blockers; the `implement-spec` flow with an integration branch, as `docs/agents/implementation.md` describes; name the ticket that sets the pattern for the others.
3. **Workflow per ticket**: guides to read, the Plane spec and tickets, recording `git status` before editing, the `tdd` skill, the repo's mise checks, and what the PR body references.
4. **Decisions already made**: the shared rules from `decisions.md`, stated positively, so the agent does not reopen them.
5. **Pending artefacts**: anything decided but not yet on `main`, such as a glossary entry, with its exact text and the ticket that lands it.
6. **Conventions reviews check**: the project rules from `AGENTS.md`, `CODING_STANDARDS.md`, and standing user preferences.
7. **Background**: the run directory files, marked optional.
8. **Report back**: PR link, checks run and results, spec changes, and anything unsettled.

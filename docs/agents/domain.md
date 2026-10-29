# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`GLOSSARY.md`** at the repo root, or
- **`GLOSSARY-MAP.md`** at the repo root if it exists: it points at one `GLOSSARY.md` per context. Read each one relevant to the topic.
- **ADRs**: these live as Plane pages, not in the repo. They are child pages of the **Architecture decisions** page in the `selfless` project, named `ADR NNNN: <title>`. List them with the Plane MCP `page list` action (`project_id` from `docs/agents/issue-tracker.md`) and read the ones that touch the area you're about to work in.

If `GLOSSARY.md` doesn't exist or there are no ADR pages yet, **proceed silently**. Don't flag their absence; don't suggest creating them upfront. The `/domain-modeling` skill (reached via `/grill-with-docs` and `/improve-codebase-architecture`) creates them lazily when terms or decisions actually get resolved.

## Layout

This repo is single-context: one `GLOSSARY.md` at the repo root. ADRs are Plane pages, numbered in order. To write one, create a child page of **Architecture decisions** with the Plane MCP `page create` action and `parent_id`, using the next free number, then attach it to the work item that decided it with `attach_to_workitem`.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a test name), use the term as defined in `GLOSSARY.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal: either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR page, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0007 (event-sourced orders), but worth reopening because…_

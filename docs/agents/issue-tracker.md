# Issue tracker: Plane

Issues and specs for this repo live in Plane Cloud, not on GitHub. Use the Plane MCP server for all operations. Its tools take an `action` argument, for example `workitem` with `action: "create"`.

- Workspace slug: `reilley`
- Project: `selfless`, identifier `SELFLESS`, id `00ca7d3a-c854-4422-b586-9d46503cda0a`
- Work item URL: `https://app.plane.so/reilley/browse/SELFLESS-<n>/`

Plane calls issues "work items". Tools that take a UUID (`state`, `labels`, `assignees`, `parent`) need the id, not the name. Look ids up with `state list`, `label list` and `member me` first.

## States and the board

The board columns are the project's states. Move a work item by updating its `state`. The triage roles the skills talk about are states here, not labels; see `docs/agents/triage-labels.md`.

| State | Group | Meaning |
| --- | --- | --- |
| Needs triage | backlog | New bug or request, waiting for `/triage`. The default for new work items. |
| Needs info | backlog | Waiting on the reporter for more information. |
| Ready for agent | unstarted | Fully specified; an agent can take it. Published specs and tickets start here. |
| Ready for human | unstarted | Only the owner can do it, such as granting access or approving an apply. |
| In progress | started | Claimed (assigned) and being worked on. A spec is In progress while `/implement-spec` builds it. |
| In review | started | A spec whose integration branch is in `/code-review`. |
| Done | completed | Resolved. |
| Won't fix | cancelled | Will not be done, or ruled out of scope. |

A work item is **open** while its state group is `backlog`, `unstarted` or `started`, and **closed** once it is `completed` or `cancelled`. **Ready** means Ready for agent with no assignee and no open blocker.

## Conventions

- **Create an issue**: `workitem create` with `project_id`, `name` and `description_html`. Descriptions and comments are HTML, so convert markdown before sending it.
- **Read an issue**: `workitem retrieve_by_identifier` with `workitem_identifier: "SELFLESS-<n>"`. Read its comments with `workitem_comment list`.
- **List issues**: `workitem list` with `project_id` and a PQL filter, for example `stateGroup IN openStates() AND label = "<label id>"`. Pass `fields` (such as `id,name,parent,assignees,labels,sequence_id,state`) to keep the result small. Call `get_pql_reference` for the syntax. PQL cannot filter on `parent`, so filter children in code.
- **Make an issue a sub-issue of a parent**: pass `parent: <parent id>` to `workitem create`, or set it later with `workitem update`.
- **Blocking**: Plane's native dependencies. `workitem_relation create` with `workitem_id: <blocked id>`, `workitem_ids: [<blocker id>, ...]` and `relation_type: "blocked_by"`. A work item is unblocked when every item in its `blocked_by` list is closed.
- **Count open blockers**: `workitem_relation list` on the work item returns `dependencies.blocked_by` with each blocker's `state_id`. Count the blockers whose state is in an open group.
- **Comment on an issue**: `workitem_comment create` with `comment_html`.
- **Apply / remove labels**: `workitem manage_label` with `add_label_id` or `remove_label_id`. Create a missing label first with `label create`.
- **Close**: post the explanation with `workitem_comment create`, then `workitem update` with the Done state. Use the Won't fix state instead when the issue will not be done.

## Specs and tickets

A **spec** is a work item labelled `spec`, published by `/to-spec`. Its **tickets** are work items published by `/to-tickets` with the spec as their `parent`. Filter on the `spec` label to see only specs. Plane's work item types and templates are paid features, so the label and the structures below stand in for them.

Every spec description uses these h2 sections, in this order:

1. **Problem Statement**: the user's problem, in their words.
2. **Solution**: what the user gets.
3. **User Stories**: one numbered list of "As a ..., I want ..., so that ...". Group with h3 headings only past about 40 stories, and keep the numbering going across groups.
4. **Implementation Decisions**: h3 subsections. Start with **Scope line with other specs** when the spec reads or writes a model another spec owns, and name the owner with its number. Then Domain, Application, API, Settings, each app, Telemetry and Homelab, leaving out the ones that don't apply.
5. **Testing Decisions**: what a good test is, the seams, what each seam covers, and prior art.
6. **Out of Scope**.
7. **Further Notes**: facts and measurements first, then a **Builds on** bullet listing ADRs and specs with their numbers. The last bullet is **Decided in** with the wayfinder ticket the spec came from.

Every ticket description uses two h2 sections:

1. **What to build**: one or two paragraphs on the behaviour, from the user's side.
2. **Acceptance criteria**: a Plane task list. Behaviour first, then one criterion for tests naming the seam and the cases, then any manual checks.

Before publishing a spec or its tickets, trace each test in Testing Decisions and each manual check through the request path the spec describes. Fix the spec when a check cannot pass under its own rules, for example a test that expects a span on a route the spec says creates none.

Descriptions never repeat what Plane fields already hold: no Parent or Blocked by sections. When a spec or ticket changes, edit its description and leave a short comment saying what changed. Don't leave amendments only in comments.

Specs and tickets start in Ready for agent, including blocked ones: the blocking links decide what is ready, so don't park blocked tickets in another state. Steps only the owner can do, such as granting access or approving an apply, go in their own ticket in Ready for human that blocks the agent tickets that need them.

## When a skill says "publish to the issue tracker"

Create a work item in the `selfless` project.

## When a skill says "fetch the relevant ticket"

Read it as in **Read an issue** above.

## Wayfinding operations

Used by `/wayfinder`. The **map** is a single work item with **child** work items as tickets.

- **Map**: a work item labelled `wayfinder:map`, holding the Destination / Notes / Decisions so far / Not yet specified / Out of scope body. It stays In progress while the effort runs and moves to Done when the destination is reached.
- **Child ticket**: a work item whose `parent` is the map. Labels: `wayfinder:<type>` (`research`/`prototype`/`grilling`/`task`). New tickets start in Ready for agent. Once claimed, the ticket is assigned to the driving dev and moves to In progress.
- **Blocking**: see **Blocking** under Conventions. Create every ticket first, then add the relations in a second pass, because relations need ids.
- **Frontier query**: list the project's open work items with `fields` including `parent` and `assignees`, keep those whose `parent` is the map, drop any with an assignee, then drop any with an open blocker (see **Count open blockers**). The lowest `sequence_id` wins.
- **Claim**: `workitem update` with `assignees: [<your member id>]` and the In progress state, as the session's first write. Get the member id from `member me`.
- **Resolve**: post the answer with `workitem_comment create`, move the ticket to Done, then append a context pointer (gist + link) to the map's Decisions so far with `workitem update` on the map's `description_html`.
- **Out of scope**: move the ticket to Won't fix and add one line to the map's Out of scope section.

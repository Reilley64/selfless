# Triage roles

The skills speak in terms of five triage roles and say "apply the label". In this tracker each role is a **state**, not a label: set the work item's `state` to the matching state instead of applying a label. There are no triage labels to apply.

| Role in mattpocock/skills | State in our tracker | Meaning |
| --- | --- | --- |
| `needs-triage` | Needs triage | Maintainer needs to evaluate this issue |
| `needs-info` | Needs info | Waiting on reporter for more information |
| `ready-for-agent` | Ready for agent | Fully specified, ready for an AFK agent |
| `ready-for-human` | Ready for human | Requires human implementation |
| `wontfix` | Won't fix | Will not be actioned |

The only labels are `spec` (on every spec) and the `wayfinder:*` labels used by `/wayfinder`.

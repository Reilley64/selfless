# Project instructions

## Agent skills

### Issue tracker

Issues live in Plane Cloud (workspace `reilley`, project `selfless`), reached through the Plane MCP server. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default triage labels. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` at the repo root. ADRs are Plane pages under Architecture decisions. See `docs/agents/domain.md`.

## Workflow

Follow the default flow from Matt Pocock's skills ([AI Hero](https://www.aihero.dev/skills/skills-changelog-v1-1-wayfinder-to-spec-to-tickets-grilling-improvements)). Pick the entry point by what you were given:

1. **Work too big or foggy for one session**: `/wayfinder`. Its map ends in specs.
2. **An idea or feature to shape**: `/grill-with-docs`, then `/to-spec`.
3. **A spec**: `/to-tickets` breaks it into tickets with blocking links.
4. **A ticket**: `/implement` in its own session. It runs `/tdd` at the seams agreed in the spec and `/code-review` before committing.
5. **A whole spec and its ticket graph at once**: `/implement-spec`.

Incoming bugs and requests go through `/triage`. Hard bugs go through `/diagnosing-bugs`.

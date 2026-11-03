# HTML report

One self-contained HTML file. Tailwind from its CDN for layout. Diagrams are Mermaid pre-rendered to inline SVG, plus hand-built divs for editorial visuals. Prose is sparse; diagrams carry the weight. Follow the editorial style of the `improve-codebase-architecture` skill's `HTML-REPORT.md` when it is available.

## Sections, in order

1. **Header**: repo, worktree, date, a legend (module, seam or sanctioned edge, leakage, deep module), and a two-sentence verdict.
2. **Scorecard**: one row per rule. The Clean Architecture rows are Dependency Rule, entities hold critical rules, use cases orchestrate, adapters only convert, simple data crosses seams, Main composes, and frameworks at the edge. The Domain-Driven Design rows are glossary terms in code, value objects, consistency boundaries, reference by identity, and anticorruption layers. Each row has a badge (Holds, Mostly, Gaps, Sanctioned), what holds, and what does not, with counts. Below it, one metric card per layer.
3. **Crate dependencies**: the crate graph. Dashed edges for ADR-sanctioned ones, red for edges with no ADR. Beside it, a table of fan-in, fan-out, I, A and D per crate, and a plot of each crate on the A/I plane with the main sequence drawn in.
4. **Where business rules live**: a grid of rules by layer (domain, application, infrastructure, presentation). Red cell where a rule lives now, dark cell where it belongs, dashed cell for an unused copy, with finding IDs.
5. **Correctness risks found on the way**: verified bugs and security issues, each with evidence. Fix them separately from refactors.
6. **Deepening candidates**: one card per candidate with title, strength badge (Strong, Worth exploring, Speculative), dependency category, files, before and after diagrams side by side, one-sentence problem, one-sentence solution, short wins, and an amber callout when it touches an ADR.
7. **Recorded and unrecorded deviations**: each ADR with its friction, plus deviations that only a guide allows or that nothing records.
8. **Deletion test**: modules to remove outright, with the reason and finding ID.
9. **Healthy patterns** to keep.
10. **Top recommendation**: one card linking to the candidate to tackle first, and why.
11. Footer naming the run directory that holds the reviewer notes.

## Rendering

Runtime Mermaid can draw side-by-side diagrams on top of each other, so pre-render each one:

```sh
npx -y @mermaid-js/mermaid-cli -i <name>.mmd -o <name>.svg --svgId <unique-id> -b transparent -t neutral
```

mermaid-cli pins its own chrome-headless-shell version. If it cannot find one, pass `-p <config>.json` with `{"executablePath": "<installed chrome-headless-shell>", "headless": "shell"}`.

Inline each SVG and set its root style to `max-width: 100%; max-height: 100%` inside a fixed-height container. Keep diagram text short; long sequence-diagram notes overflow, so break them with `<br/>`.

Check the layout before opening the report. The `--virtual-time-budget` flag gives the Tailwind CDN time to load. Install a headless browser once with `npx puppeteer browsers install chrome-headless-shell`, then:

```sh
<printed chrome-headless-shell path> --disable-gpu --virtual-time-budget=8000 --window-size=1280,2400 --screenshot=<png> file://<report>
```

View the screenshot. To inspect a lower section, write a copy of the report with the earlier sections removed and screenshot that.

Open the report with `open <path>` and give the user its absolute path.

# GitHub automation

This guide owns CI/CD terminology and YAML layout under `.github/`. The root
`AGENTS.md` and `CODING_STANDARDS.md` still apply.

## CDF naming and grouping

Use the CD Foundation's
[Pipeline Terminology](https://github.com/cdfoundation/sig-interoperability/blob/main/docs/pipelines-terminology.md)
as reference terminology. It is unfinished guidance, not a normative standard.
The prefixes and formatting below are project conventions.

Workflow and job display names use `Stage: Description`. Choose stages by the
work performed:

| Stage | Meaning |
| --- | --- |
| Build | Compile or assemble software, documentation or generated files. |
| Test | Test, scan or verify software and project rules. |
| Release | Package, version or publish artifacts and release records. |

Workflows here stop at publishing the container image. This repository has no
Deploy or Operations stage; consult the CDF definitions if one is added.

Step display names use `Type: Description`. Use these distinctions:

- Source retrieves repository contents.
- Setup prepares tools, dependencies, caches, credentials or workspace inputs,
  including installing the tools from `mise.toml`.
- Build compiles software or generates files, such as the API client, the
  telemetry constants, the theme and the admin stylesheet.
- Test runs a test suite. Scan performs non-test verification, including
  formatting, Clippy, `cargo deny`, typechecking, linting and registry checks.
- Policy enforces project rules, including pull request title requirements.
- Package creates a distributable artifact. Tag applies a version label;
  computing a tag string alone is Setup. Publish uploads artifacts or records.
- Run covers automation spanning several types, such as release-please's
  release requests, tags and publication. Release is a stage, not a step type.

Use combined prefixes when one action spans categories, such as
`Build, test and release` for a job or `Tag and publish` for a step.
Separate jobs when execution gates or permissions differ. Put Setup commands
in their own steps, apart from the commands they prepare for.
Keep dependency order driven by execution needs, not the stage list above.
For additional step types, consult the linked CDF definitions.

## YAML layout

These rules cover workflows and the composite actions under `.github/actions/`.

- Use two-space indentation and double-quote display names containing `:`.
- Order workflow blocks as name, triggers (`on`), permissions, concurrency,
  environment and jobs. Separate top-level blocks and jobs with one blank line.
- Within a job, group name, dependencies (`needs`), condition (`if`), runner
  and timeout first. Follow with permissions, outputs, environment/defaults,
  then steps. Separate these groups with one blank line; omit absent groups.
- Begin steps with name, then optional id and condition. Keep action `uses`
  with its `with` inputs. Put shell, working directory and environment before
  a shell step's `run`. Preserve other keys beside the behavior they configure.
- Separate steps with one blank line. Keep an explanatory comment directly
  above its step or setting, without a blank line between them.
- Expand multiple job dependencies as a block sequence. Short trigger filters
  may use inline lists, such as `branches: [main]`.
- Use `|` for multi-command scripts and `>-` for a single folded command.
  Keep commands in execution order and preserve expression and quoting semantics.

## Changes and verification

Before renaming a workflow file, search the repository for its filename.
`.github/actionlint.yaml` ignores diagnostics by workflow path.

Before changing a job display name, search the docs for it and check the `main`
ruleset with `gh api repos/Reilley64/selfless/rulesets`. The ruleset requires
"Test: Pull request policy" from GitHub Actions. Renaming that job without
updating the ruleset blocks every pull request from merging.

Keep `packages: write` and the release app token scoped to the jobs that need
them. Publish the versioned image by adding tags to the tested commit image's
digest, not by rebuilding it.

For formatting-only edits, compare parsed YAML before and after with a parser
that preserves GitHub's `on` key and expression strings. Completion requires
identical values and sequence order. For structural edits, verify every `needs`
and output reference, event condition, permission change and release gate.

Run `git diff --check` and `mise run lint:workflows`, which runs the Actionlint
version pinned in `mise.toml`. If runner labels or action metadata produce
diagnostics, verify them against the runner configuration or upstream action
definition before adding a narrow filter to `.github/actionlint.yaml`. Report
filtered diagnostics and any disabled shell checks, and report commands that
failed or were unavailable.

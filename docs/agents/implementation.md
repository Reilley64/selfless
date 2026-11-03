# Building tickets in worktrees

- A fresh worktree needs `mise trust` and `bun install` before `bun run generate`.
- Each ticket worktree builds into its own Cargo target directory. Set `CARGO_TARGET_DIR` to a directory for the ticket, outside the repository, before the first `cargo` command. The `.cargo/config.toml` in the folder that holds the worktrees points them all at one shared directory. Worktrees that build into one directory overwrite each other's crates without Cargo noticing. A test binary then links another branch's `domain` or `application`, and the Weaver constants generated in `application` come from whichever worktree built last.
- A target directory grows to 5 to 11 GB. Start a new one as an APFS clone of a built one with `cp -cR <built> <new>`, which copies no data until a file changes. `CARGO_INCREMENTAL=0` keeps it smaller.
- Remove a ticket worktree and its target directory as soon as its branch merges into the integration branch.
- In a ticket worktree, run only the checks for what the ticket changes: `cargo fmt`, Clippy and the tests of the crates it touches (`cargo clippy -p <crate> --all-targets`, `cargo test -p <crate>`), and `bun run typecheck`, `bun run lint` and `bun run test` in the packages it touches. Do not run `mise run test`, `mise run check` or `mise run test:image` for the whole workspace. The pull request's checks build and test everything, including the container image.
- Ticket branches merge into the integration branch without waiting for the pull request's checks. They run once, after the spec's last ticket merges. A failing check then gets fixed on the integration branch.

## Before you start

- Tests: read `docs/agents/testing.md` before you write, move or run a test.
- Migrations: read `docs/agents/migrations.md` before you change a model in `domain`.
- Device checks: read `docs/agents/device-checks.md` before you run an app on a simulator, an emulator or a browser.

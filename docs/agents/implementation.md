# Building tickets in worktrees

- Ticket worktrees share one Cargo target directory, set by a `.cargo/config.toml` in the folder that holds them, outside the repository. Cargo locks the directory during a build, so parallel builds in different worktrees wait for each other.
- Remove a ticket worktree as soon as its branch merges into the integration branch. A worktree with its own build output holds several GiB.
- In a ticket worktree, run only the checks for what the ticket changes: `cargo fmt`, Clippy and the tests of the crates it touches (`cargo clippy -p <crate> --all-targets`, `cargo test -p <crate>`), and `bun run typecheck`, `bun run lint` and `bun run test` in the packages it touches. Do not run `mise run test`, `mise run check` or `mise run test:image` for the whole workspace. The pull request's checks build and test everything, including the container image.
- Ticket branches merge into the integration branch without waiting for the pull request's checks. They run once, after the spec's last ticket merges. A failing check then gets fixed on the integration branch.

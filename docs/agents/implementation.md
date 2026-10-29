# Building tickets in worktrees

- Ticket worktrees share one Cargo target directory, set by a `.cargo/config.toml` in the folder that holds them, outside the repository. Cargo locks the directory during a build, so parallel builds in different worktrees wait for each other.
- Remove a ticket worktree as soon as its branch merges into the integration branch. A worktree with its own build output holds several GiB.
- Run `mise run test:image` only when a change touches what the image build reads: `Dockerfile`, `.dockerignore`, `scripts/smoke-test-image`, the tools in `mise.toml`, the package manifests or the `generate` scripts. Otherwise the "Build and test: Container image" check on the pull request covers it. A cold build takes about 12 minutes on an arm64 Mac, because Docker emulates amd64.

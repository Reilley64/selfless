# Monorepo tooling for Cargo and TypeScript

Research for [SELFLESS-11](https://app.plane.so/reilley/browse/SELFLESS-11/). Sources were read on 2026-10-29. Version numbers come from crates.io, the npm registry and GitHub releases on that date.

## Summary

The repository needs two workspaces side by side. Each task runner below calls Cargo and the JavaScript package manager instead of replacing them [moon-rust](https://moonrepo.dev/docs/guides/rust/handbook) [turbo-rust](https://turborepo.dev/docs/guides/tools/rust). A root `Cargo.toml` holds the Rust crates. A pnpm or Bun workspace holds the web app, the TV app and a shared `api-client` package. Expo detects Bun, npm, pnpm and Yarn workspaces and configures Metro for them since SDK 52 [expo-monorepos](https://docs.expo.dev/guides/monorepos/).

- **Workspace manager.** pnpm is the recommended choice, with Bun as an equal alternative. Expo supports isolated installs from SDK 54, which is pnpm's default, and documents a `nodeLinker: hoisted` fallback for libraries that break [expo-monorepos](https://docs.expo.dev/guides/monorepos/). Both have catalogs to keep one React version across apps [pnpm-catalogs](https://pnpm.io/catalogs) [bun-catalogs](https://bun.com/docs/pm/catalogs).
- **Task runner.** Three tools can run Cargo and TypeScript tasks from one command. moon has stable support for Rust and Node [moon-intro](https://moonrepo.dev/docs). Turborepo reads Cargo workspaces natively since 2.10.4, but marks that support experimental [turbo-rust](https://turborepo.dev/docs/guides/tools/rust) [turbo-releases](https://github.com/vercel/turborepo/releases/tag/v2.11.0). mise pins Rust, Node and pnpm in one file and runs tasks across projects, while its workspace graph is experimental [mise-monorepo](https://mise.jdx.dev/tasks/monorepo.html) [mise-graph](https://mise.jdx.dev/tasks/workspace-graph.html). Nx needs a community plugin for Rust [monodon](https://github.com/cammisuli/monodon/blob/main/packages/rust/README.md). The recommendation is to start with mise for tool versions and tasks, let Cargo and pnpm order their own builds, and move to Turborepo or moon only if caching or affected-only runs become worth the extra config.
- **Typed API client.** Generate an OpenAPI 3.1 document from the Rust server with utoipa, commit it, and generate TypeScript types from it with openapi-typescript. Call the API through openapi-fetch, with openapi-react-query where the apps use TanStack Query [utoipa](https://github.com/juhaku/utoipa) [openapi-ts](https://openapi-ts.dev/introduction) [openapi-fetch](https://openapi-ts.dev/openapi-fetch/) [openapi-rq](https://openapi-ts.dev/openapi-react-query/). orval and Hey API are heavier alternatives that generate hooks, validators and mocks [orval](https://github.com/orval-labs/orval) [heyapi](https://heyapi.dev/openapi-ts/get-started). Type-only exporters such as ts-rs, Specta and typeshare do not describe endpoints, and rspc is no longer maintained [ts-rs](https://github.com/Aleph-Alpha/ts-rs) [specta](https://github.com/specta-rs/specta) [typeshare](https://github.com/1Password/typeshare) [rspc](https://github.com/specta-rs/rspc).

## Suggested layout

This layout follows the Expo guide's `apps/` and `packages/` convention [expo-monorepos](https://docs.expo.dev/guides/monorepos/) and the Cargo workspace model, in which all crates share one `Cargo.lock` and one `target` directory [cargo-workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html). Directory names are placeholders.

```text
selfless/
├── Cargo.toml            # [workspace] with the server crates
├── Cargo.lock
├── crates/               # domain, application, infrastructure, presentation
├── package.json          # private root, scripts only
├── pnpm-workspace.yaml   # apps/*, packages/*, catalog
├── apps/
│   ├── web/              # React web app
│   └── tv/               # Expo app with react-native-tvos
├── packages/
│   └── api-client/       # openapi.json, generated types, client factory
└── mise.toml             # rust, node, pnpm versions and root tasks
```

## Workspace managers

| Tool | Version | Workspace file | Shared versions | Default layout in a workspace |
| --- | --- | --- | --- | --- |
| pnpm | 12.16.1 | `pnpm-workspace.yaml` | catalogs in `pnpm-workspace.yaml` | isolated |
| Bun | 1.4.4 | `workspaces` in `package.json` | catalogs in root `package.json` | isolated for new lockfiles |
| npm | 12.2.1 | `workspaces` in `package.json` | `overrides` only | hoisted |
| Yarn Berry | 4.18.1 | `workspaces` in `package.json` | `resolutions` | hoisted |

Sources: [pnpm-ws](https://pnpm.io/workspaces) [pnpm-catalogs](https://pnpm.io/catalogs) [bun-ws](https://bun.com/docs/pm/workspaces) [bun-isolated](https://bun.com/docs/pm/isolated-installs) [bun-catalogs](https://bun.com/docs/pm/catalogs) [expo-monorepos](https://docs.expo.dev/guides/monorepos/). Versions from the npm registry.

### What Expo requires

- Expo configures Metro for monorepos automatically from SDK 52. A `metro.config.js` should not set `watchFolders`, `resolver.nodeModulesPath`, `resolver.extraNodeModules` or `resolver.disableHierarchicalLookup` [expo-monorepos](https://docs.expo.dev/guides/monorepos/).
- Expo supports isolated installs from SDK 54. Some React Native libraries can still fail to build or resolve under isolated installs, and the guide's fix for pnpm is `nodeLinker: hoisted` [expo-monorepos](https://docs.expo.dev/guides/monorepos/).
- A monorepo cannot hold two React Native versions, and one app cannot load two React versions. From SDK 55, Expo matches Metro resolution to the native modules that autolinking links, and enables this by default in monorepos [expo-monorepos](https://docs.expo.dev/guides/monorepos/).
- If one Expo project in the monorepo targets TV, the Expo TV guide says every Expo project in it should use `react-native-tvos` as its `react-native` dependency, even projects that do not target TV. From SDK 56 an SDK upgrade also upgrades the `react-native-tvos` alias [expo-tv](https://docs.expo.dev/guides/building-for-tv/). A web app built with Vite and `react-dom` does not depend on `react-native`, so this rule only matters if the web app is also an Expo project.

### pnpm or Bun

Both work with Expo, Turborepo, moon and mise [expo-monorepos](https://docs.expo.dev/guides/monorepos/) [turbo-support](https://turborepo.dev/docs/getting-started/support-policy) [moon-intro](https://moonrepo.dev/docs) [mise-graph](https://mise.jdx.dev/tasks/workspace-graph.html).

- pnpm runs recursive scripts in topological order by default [pnpm-recursive](https://pnpm.io/cli/recursive). It defines catalogs and `nodeLinker` in `pnpm-workspace.yaml` [pnpm-catalogs](https://pnpm.io/catalogs) [pnpm-settings](https://pnpm.io/settings). Corepack no longer ships with Node.js from version 25 [node-corepack](https://nodejs.org/api/corepack.html), so the pnpm version needs another pin, such as mise.
- Bun runs `bun --filter` scripts in dependency order [bun-filter](https://bun.com/docs/pm/filter), defaults new workspaces to isolated installs [bun-isolated](https://bun.com/docs/pm/isolated-installs), and has catalogs [bun-catalogs](https://bun.com/docs/pm/catalogs). The owner already uses Bun with Expo in another project, which lowers the learning cost.

pnpm is the recommendation because isolated installs are its default, while Bun uses them by default only in workspaces whose lockfile was created by Bun 1.3.2 or later, and because pnpm holds its workspace settings in one file [expo-monorepos](https://docs.expo.dev/guides/monorepos/) [bun-isolated](https://bun.com/docs/pm/isolated-installs) [pnpm-settings](https://pnpm.io/settings). The evidence does not rule out Bun.

## Task runners

The repository has one cross-language edge. The server's OpenAPI document feeds the `api-client` package, which both apps import. Every other build order lives inside Cargo or inside the JavaScript workspace. This keeps the task runner's job small.

| Tool | Version | Rust support | TypeScript and Expo support | Caching | Config per project | License |
| --- | --- | --- | --- | --- | --- | --- |
| moon | 2.6.4 | Stable, tiers 0 to 3, installs Rust through rustup | Node, Bun, npm, pnpm, Yarn at tier 3 | Local, remote over Bazel RE API | `moon.yml` | MIT |
| Turborepo | 2.11.10 | Experimental native Cargo workspaces behind `futureFlags.experimentalCargoWorkspaces` | Stable for pnpm, npm, Yarn, Bun | Local, remote over an open HTTP spec | Root `turbo.json`, reads workspace manifests | MIT |
| mise | 2026.10.22 | Pins Rust through rustup, reads Cargo workspaces into an experimental graph | Pins Node and pnpm, imports package scripts into the experimental graph | Freshness checks stable, artifact cache experimental | `mise.toml` per config root | MIT |
| Nx | 24.0.2 | Community plugin `@monodon/rust` 3.0.0 | Official `@nx/expo` plugin | Local, Nx Cloud or a self-built server | `project.json` or inferred | MIT |
| just | 1.59.0 | Runs commands only | Runs commands only | none | `justfile` | CC0 |

Sources: [moon-intro](https://moonrepo.dev/docs) [moon-rust](https://moonrepo.dev/docs/guides/rust/handbook) [moon-remote](https://moonrepo.dev/docs/guides/remote-cache) [turbo-rust](https://turborepo.dev/docs/guides/tools/rust) [turbo-support](https://turborepo.dev/docs/getting-started/support-policy) [turbo-remote](https://turborepo.dev/docs/core-concepts/remote-caching) [mise-monorepo](https://mise.jdx.dev/tasks/monorepo.html) [mise-graph](https://mise.jdx.dev/tasks/workspace-graph.html) [mise-cache](https://mise.jdx.dev/tasks/caching.html) [mise-rust](https://mise.jdx.dev/lang/rust.html) [nx-intro](https://nx.dev/docs/getting-started/intro) [monodon](https://github.com/cammisuli/monodon/blob/main/packages/rust/README.md) [nx-expo](https://nx.dev/docs/technologies/react/expo/introduction) [nx-selfhost](https://nx.dev/docs/guides/tasks--caching/self-hosted-caching) [just](https://github.com/casey/just). Versions from npm and GitHub releases. Licenses from GitHub and crates.io.

### moon

moon treats Rust and Node as fully supported languages. It parses manifests and lockfiles for hashing and can install pinned toolchains [moon-intro](https://moonrepo.dev/docs). Its Rust handbook says moon runs `cargo` commands and does not replace Cargo, and it advises against caching the `target` directory [moon-rust](https://moonrepo.dev/docs/guides/rust/handbook). Tasks in a TypeScript project can depend on tasks in a Rust project through `project:task` targets such as `deps: ['server:openapi']` [moon-target](https://moonrepo.dev/docs/concepts/target). moon 2.0 shipped on 2026-02-18 [moon-releases](https://github.com/moonrepo/moon/releases/tag/v2.0.0). The cost is a `moon.yml` per project and one more config format to learn.

### Turborepo

Turborepo added Cargo crate discovery, Cargo task execution and Cargo hashing in 2.10.4 on 2026-07-06, and the 2.11.0 minor release on 2026-09-18 collected that work [turbo-releases](https://github.com/vercel/turborepo/releases/tag/v2.11.0). With the flag on, it maps `build`, `test`, `check`, `lint` and `format` to Cargo commands and hashes `Cargo.lock`, `rustc -vV` and local crate sources [turbo-rust](https://turborepo.dev/docs/guides/tools/rust). The docs say the support "is experimental and can change at any time" and suggest it for side projects and proofs of concept [turbo-rust](https://turborepo.dev/docs/guides/tools/rust). Files a crate reads from outside its own directory, such as an `include_str!` of a shared schema, must be declared as inputs by hand [turbo-rust](https://turborepo.dev/docs/guides/tools/rust). Before this feature, the documented approach was a `package.json` wrapper beside each non-JavaScript project, kept in sync with that ecosystem's own dependency list [turbo-multi](https://turborepo.dev/docs/guides/multi-language). A cross-package edge such as `api-client#generate` depending on a server task uses the `package#task` form of `dependsOn` [turbo-config](https://turborepo.dev/docs/reference/configuration).

### mise

mise sets one `mise.toml` per config root, gives tasks path names such as `//apps/web:build`, and lets a task depend on tasks in other projects [mise-monorepo](https://mise.jdx.dev/tasks/monorepo.html). It installs Rust through rustup and can read `rust-toolchain.toml` [mise-rust](https://mise.jdx.dev/lang/rust.html), so one file can pin Rust, Node and pnpm. The monorepo page itself advises letting one mise task call Cargo or Turborepo when that tool already orders the builds, instead of declaring the same edges twice [mise-monorepo](https://mise.jdx.dev/tasks/monorepo.html). The workspace graph, which reads Cargo and Node workspaces to run only affected tasks, is experimental [mise-graph](https://mise.jdx.dev/tasks/workspace-graph.html). Freshness checks based on file times are stable, and the content-addressed artifact cache is experimental [mise-cache](https://mise.jdx.dev/tasks/caching.html).

### Nx

Nx describes itself as a build system for JavaScript and TypeScript monorepos with plugins for other languages [nx-intro](https://nx.dev/docs/getting-started/intro). Rust support comes from `@monodon/rust`, a community plugin with 55 GitHub stars whose 3.0.0 release depends on `@nx/devkit` `^22.0.0` while Nx is at 24 [monodon](https://github.com/cammisuli/monodon/blob/main/packages/rust/README.md). The official `@nx/expo` docs list support for Expo 53 to 55, while Expo is at SDK 58 [nx-expo](https://nx.dev/docs/technologies/react/expo/introduction) [npm-expo](https://www.npmjs.com/package/expo). Nx's managed remote cache is Nx Cloud, and a self-hosted cache means implementing Nx's OpenAPI cache spec [nx-selfhost](https://nx.dev/docs/guides/tasks--caching/self-hosted-caching). These gaps make Nx the weakest fit here.

### Not considered further

just saves and runs project commands [just](https://github.com/casey/just). It has no project graph or caching, so it adds nothing over mise tasks. Bazel and Buck2 were out of scope for this research.

### Recommendation

Start with mise. It pins Rust, Node and pnpm, which is needed anyway now that Node no longer ships Corepack [node-corepack](https://nodejs.org/api/corepack.html). Root tasks such as `check`, `test` and `generate-client` call `cargo` and `pnpm -r`, which already order their own packages [pnpm-recursive](https://pnpm.io/cli/recursive) [cargo-workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html). Move to Turborepo once its Cargo support is stable, or to moon now, if CI time or affected-only runs start to matter. Both read the same Cargo and pnpm workspaces, so the move does not require restructuring.

## Typed API client from the Rust server

### Recommended pipeline

1. The server describes its HTTP API with utoipa and exposes `ApiDoc::openapi()`.
2. A small binary or test writes `packages/api-client/openapi.json`. The utoipa README links to an example of dumping the document at build time [utoipa](https://github.com/juhaku/utoipa). The file is committed.
3. `packages/api-client` runs `openapi-typescript openapi.json -o src/schema.d.ts` and exports a client built with `openapi-fetch` and, if wanted, `openapi-react-query` [openapi-ts](https://openapi-ts.dev/introduction) [openapi-fetch](https://openapi-ts.dev/openapi-fetch/) [openapi-rq](https://openapi-ts.dev/openapi-react-query/).
4. `apps/web` and `apps/tv` depend on `api-client` through `workspace:*`.
5. CI regenerates both files and fails on a diff, so the document and the client cannot drift from the server.

Committing `openapi.json` lets the TypeScript apps build and type-check without a Rust toolchain, and makes API changes visible in review.

### Producing the OpenAPI document on the server

| Crate | Version | Released | Approach | Frameworks | Notes |
| --- | --- | --- | --- | --- | --- |
| utoipa | 6.0.0 | 2026-09-22 | Derive macros on types and `#[utoipa::path]` on handlers | Framework-agnostic, bindings for axum, actix-web, Rocket | OpenAPI 3.1, MIT or Apache-2.0 |
| utoipa-axum | 0.3.0 | 2026-09-22 | `OpenApiRouter` registers a handler in the router and the document at once | axum 0.8 | Reduces drift between routes and spec |
| aide | 0.15.1 | 2026-04-14 | Infers the document from axum extractor and response types, schemas from schemars | axum only | OpenAPI 3.1, depends on schemars 0.9 while schemars is at 1.2.3 |
| dropshot | 0.17.1 | 2026-06-02 | Its own REST framework that generates OpenAPI from handler types | Dropshot only | Would replace the HTTP framework choice |

Sources: [utoipa](https://github.com/juhaku/utoipa) [utoipa-axum](https://github.com/juhaku/utoipa/tree/master/utoipa-axum) [aide-docs](https://docs.rs/aide/latest/aide/) [dropshot](https://docs.rs/dropshot/latest/dropshot/) [crates-io](https://crates.io/).

utoipa is the recommendation. It has the most recent downloads on crates.io of these crates, a release last month, and it does not tie the server to one framework [crates-io](https://crates.io/) [utoipa](https://github.com/juhaku/utoipa). Its handler annotations repeat what the handler signature already says. `utoipa-axum` limits that because the router and the document come from the same `routes!` call [utoipa-axum](https://github.com/juhaku/utoipa/tree/master/utoipa-axum). aide infers more from types but supports only axum and lags one schemars major version [aide-docs](https://docs.rs/aide/latest/aide/) [crates-io](https://crates.io/).

The HTTP framework is not chosen yet. The repository used as the architecture model, payments-axus-solid, depends on Topcoat 0.11 in its workspace `Cargo.toml`. Topcoat renders HTML on the server and lists "OpenAPI endpoints" as an unchecked roadmap item [topcoat](https://github.com/tokio-rs/topcoat). A JSON API for the TV app would need either another framework such as axum for that layer or a hand-built utoipa document, since utoipa also works without a framework [utoipa](https://github.com/juhaku/utoipa).

### Generating TypeScript for the apps

| Package | Version | Released | Output | Runtime |
| --- | --- | --- | --- | --- |
| openapi-typescript | 7.13.0 | 2026-02-11 | Types only, OpenAPI 3.0 and 3.1 | none |
| openapi-fetch | 0.17.0 | 2026-02-11 | Typed `GET`, `POST` and other calls over `fetch` | about 6 kB |
| openapi-react-query | 0.5.4 | 2026-02-11 | TanStack Query hooks over openapi-fetch | about 1 kB |
| orval | 8.45.0 | 2026-10-25 | Functions, React Query hooks, zod schemas, MSW mocks | generated code, Node 22.18 or newer |
| @hey-api/openapi-ts | 0.99.0 | 2026-06-22 | SDK, types, TanStack Query, zod and other plugins | generated code, Node 22 or newer |

Sources: [openapi-ts](https://openapi-ts.dev/introduction) [openapi-fetch](https://openapi-ts.dev/openapi-fetch/) [openapi-rq](https://openapi-ts.dev/openapi-react-query/) [orval](https://github.com/orval-labs/orval) [heyapi](https://heyapi.dev/openapi-ts/get-started) [npm](https://www.npmjs.com/). All five are MIT licensed.

openapi-typescript with openapi-fetch is the recommendation. It generates one type file and uses a small runtime, so the generated code stays small and readable, and it supports the OpenAPI 3.1 that utoipa emits [openapi-ts](https://openapi-ts.dev/introduction) [openapi-fetch](https://openapi-ts.dev/openapi-fetch/) [utoipa](https://github.com/juhaku/utoipa). orval fits better if the apps want generated hooks per endpoint and MSW mocks for tests [orval](https://github.com/orval-labs/orval). Hey API says it is in initial development and asks users to pin an exact version [heyapi](https://heyapi.dev/openapi-ts/get-started).

### Options that do not produce a client

- ts-rs, Specta and typeshare export Rust structs and enums as TypeScript types [ts-rs](https://github.com/Aleph-Alpha/ts-rs) [specta](https://github.com/specta-rs/specta) [typeshare](https://github.com/1Password/typeshare). They share DTO shapes but not paths, methods, parameters or status codes, so the apps would still hand-write every call.
- rspc offered tRPC-style end-to-end types for Rust, but its README says it is no longer maintained [rspc](https://github.com/specta-rs/rspc).
- progenitor generates Rust clients, not TypeScript, and reads only OpenAPI 3.0.x [progenitor](https://github.com/oxidecomputer/progenitor). It could serve Rust integration tests if the server ever emits 3.0.

## Open questions

1. pnpm or Bun as the JavaScript workspace manager.
2. mise alone, Turborepo with its experimental Cargo support, or moon. This depends on whether CI caching and affected-only runs matter for a one-developer project.
3. Which HTTP framework serves the JSON API, and whether it supports utoipa or another OpenAPI generator. Topcoat does not emit OpenAPI today [topcoat](https://github.com/tokio-rs/topcoat).
4. Whether the web app is a plain React app with `react-dom` or an Expo project with web output. An Expo web app must also use `react-native-tvos` [expo-tv](https://docs.expo.dev/guides/building-for-tv/).
5. Whether the apps share UI code, or only the API client.
6. Thin client with openapi-fetch or generated hooks with orval, and whether both apps use TanStack Query.
7. How non-JSON endpoints, such as HLS playlists, media segments and images, appear in the OpenAPI document, or whether the client builds those URLs itself.
8. Whether `openapi.json` and the generated types are committed or generated in CI only.

## Sources

All sources were read on 2026-10-29.

- cargo-workspaces: [The Cargo Book, Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- expo-monorepos: [Expo docs, Work with monorepos](https://docs.expo.dev/guides/monorepos/)
- expo-tv: [Expo docs, Build Expo apps for TV (modified 2026-10-16)](https://docs.expo.dev/guides/building-for-tv/)
- npm-expo: [npm registry, expo (SDK 58.0.0 published 2026-09-29)](https://www.npmjs.com/package/expo)
- pnpm-ws: [pnpm docs, Workspace](https://pnpm.io/workspaces)
- pnpm-catalogs: [pnpm docs, Catalogs](https://pnpm.io/catalogs)
- pnpm-settings: [pnpm docs, Settings](https://pnpm.io/settings)
- pnpm-recursive: [pnpm docs, pnpm -r](https://pnpm.io/cli/recursive)
- bun-ws: [Bun docs, Workspaces](https://bun.com/docs/pm/workspaces)
- bun-isolated: [Bun docs, Isolated installs](https://bun.com/docs/pm/isolated-installs)
- bun-catalogs: [Bun docs, Catalogs](https://bun.com/docs/pm/catalogs)
- bun-filter: [Bun docs, --filter](https://bun.com/docs/pm/filter)
- node-corepack: [Node.js docs, Corepack](https://nodejs.org/api/corepack.html)
- moon-intro: [moon docs, Introduction and supported languages](https://moonrepo.dev/docs)
- moon-rust: [moon docs, Rust handbook](https://moonrepo.dev/docs/guides/rust/handbook)
- moon-target: [moon docs, Targets](https://moonrepo.dev/docs/concepts/target)
- moon-remote: [moon docs, Remote caching](https://moonrepo.dev/docs/guides/remote-cache)
- moon-releases: [moon v2.0.0 release](https://github.com/moonrepo/moon/releases/tag/v2.0.0)
- turbo-rust: [Turborepo docs, Rust (Experimental), updated 2026-10-26](https://turborepo.dev/docs/guides/tools/rust)
- turbo-multi: [Turborepo docs, Multi-language support](https://turborepo.dev/docs/guides/multi-language)
- turbo-config: [Turborepo docs, Configuration reference](https://turborepo.dev/docs/reference/configuration)
- turbo-support: [Turborepo docs, Support policy](https://turborepo.dev/docs/getting-started/support-policy)
- turbo-remote: [Turborepo docs, Remote caching](https://turborepo.dev/docs/core-concepts/remote-caching)
- turbo-releases: [Turborepo v2.11.0 release notes](https://github.com/vercel/turborepo/releases/tag/v2.11.0)
- mise-monorepo: [mise docs, Monorepo tasks](https://mise.jdx.dev/tasks/monorepo.html)
- mise-graph: [mise docs, Workspace project graph (experimental)](https://mise.jdx.dev/tasks/workspace-graph.html)
- mise-cache: [mise docs, Task caching](https://mise.jdx.dev/tasks/caching.html)
- mise-rust: [mise docs, Rust](https://mise.jdx.dev/lang/rust.html)
- nx-intro: [Nx docs, What is Nx?](https://nx.dev/docs/getting-started/intro)
- nx-expo: [Nx docs, Nx with Expo](https://nx.dev/docs/technologies/react/expo/introduction)
- nx-selfhost: [Nx docs, Self-hosted remote cache](https://nx.dev/docs/guides/tasks--caching/self-hosted-caching)
- monodon: [@monodon/rust README](https://github.com/cammisuli/monodon/blob/main/packages/rust/README.md)
- just: [just README](https://github.com/casey/just)
- utoipa: [utoipa README](https://github.com/juhaku/utoipa)
- utoipa-axum: [utoipa-axum README](https://github.com/juhaku/utoipa/tree/master/utoipa-axum)
- aide-docs: [aide docs on docs.rs](https://docs.rs/aide/latest/aide/)
- topcoat: [Topcoat README and roadmap](https://github.com/tokio-rs/topcoat)
- crates-io: [crates.io API, versions and recent downloads](https://crates.io/)
- openapi-ts: [openapi-typescript docs](https://openapi-ts.dev/introduction)
- openapi-fetch: [openapi-fetch docs](https://openapi-ts.dev/openapi-fetch/)
- openapi-rq: [openapi-react-query docs](https://openapi-ts.dev/openapi-react-query/)
- orval: [orval README](https://github.com/orval-labs/orval)
- heyapi: [Hey API docs, Get started](https://heyapi.dev/openapi-ts/get-started)
- npm: [npm registry, versions and licenses](https://www.npmjs.com/)
- ts-rs: [ts-rs README](https://github.com/Aleph-Alpha/ts-rs)
- specta: [Specta README](https://github.com/specta-rs/specta)
- typeshare: [typeshare README](https://github.com/1Password/typeshare)
- rspc: [rspc README](https://github.com/specta-rs/rspc)
- progenitor: [progenitor README](https://github.com/oxidecomputer/progenitor)
- dropshot: [Dropshot docs on docs.rs](https://docs.rs/dropshot/latest/dropshot/)

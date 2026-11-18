# Development

How to build Kition Doc from source, run its tests, take the README's pictures and publish the
kit's package. To use Doc, start with the [README](../README.md).

## Repository layout

| Path | Contents |
| --- | --- |
| `crates/` | The engine (Rust), in groups: `kdm`, `engine`, `formats`, `render`, `support`; compiled to WebAssembly for the editor |
| `bindings/` | The engine's two outlets: `wasm` (the in-page editor's engine, behind Kition's `KitDocument` contract) and `headless` (the `kition-doc` program for Kition's agent and `kition-kits-mcp`) |
| `web/` | The kit's editor (React, on kition-sdk's `@kition/kit-editor`) |
| `agent/` | The agent guide and checks (KHP) |
| `conformance/` | The sample documents the conformance checks open (KHP) |
| `kit.json` | The kit manifest (KHP) |
| `features.yaml`, `cases/` | The feature catalogue and its cases ([quality.md](quality.md)) |
| `docs/` | Documents: entry documents at the first level, parity documents in `docs/parity/`, feature documents in `docs/features/` |
| `assets/` | Fonts and hyphenation data the engine ships with; `readme/`: the README's pictures |
| `tools/` | The repository's own tools: `tools/xtask` (`cargo xtask`) and the Python WPS layout comparison (`golden_*.py`) |

## Build and test

```sh
cargo test --workspace
cargo build --release -p kition-doc                    # the headless kit: target/release/kition-doc
cd web && pnpm install && pnpm engine && pnpm test:flow
```

`pnpm engine` builds `bindings/wasm` into `web/public/engine` and copies fonts from a
kition-fonts checkout next to this repository (or the one `KITION_FONTS_DIR` names). `pnpm dev`
serves `dev.html`, the editor in a development host; `web/package.json` lists the other browser
tests (`test:drawing`, `test:chart`, `test:modes`, `test:export`, …).

The check every change passes is `cargo xtask ci` ([quality.md](quality.md)); `cargo xtask --help`
lists the other tasks (`layers`, `assets`, `parity`, …). The WPS layout comparison is described in
[features/wps-layout-testing.md](features/wps-layout-testing.md).

### The headless kit for an agent

kition-sdk's `kition-kits-mcp` serves `kition-doc` as MCP tools (`doc_inspect`, `doc_run`,
`doc_check`, …) on a folder of documents, for any MCP client:

```sh
cargo build --release -p kition-doc
cargo install --git https://github.com/KitionAI/kition-sdk kition-kits --bin kition-kits-mcp
kition-kits-mcp --workspace <folder> --kit target/release/kition-doc
```

[agent/guide.md](../agent/guide.md) explains the tools, and [writing-format.md](writing-format.md)
lists every command.

### The editor's dependencies

By default `pnpm install` in `web/` installs the published `@kition/*` packages from npm, at the
versions the committed `web/pnpm-lock.yaml` pins, so the editor needs no other checkout. To
develop against a kition-sdk checkout instead, run `KITION_LINK=1 pnpm install` in `web/`:
`web/.pnpmfile.cjs` then links `@kition/kit-editor`, `@kition/kit-host`,
`@kition/kit-ui-protocol` and `@kition/kit-dev-host` from the checkout next to this repository
(or the one `KITION_SDK` names); the Kition Tessera packages still come from npm. The linked
install rewrites `web/pnpm-lock.yaml`; do not commit a lockfile changed that way.

## Take the README pictures

[`web/scripts/readme-shots.mjs`](../web/scripts/readme-shots.mjs) takes the editor pictures in
`assets/readme/` with kition-sdk's shared runner (`tools/readme-shots/` there): the development
host in Classic mode, light theme, English, 1440 × 900 at device scale 2, on the kition-showcase
samples `magazine-feature` and `quarterly-business-report`. It needs kition-sdk and
kition-showcase checkouts next to this repository (or `KITION_SDK` and `KITION_SHOWCASE`), the
engine in `web/public/` (`pnpm engine`), and no HTTP proxy set:

```sh
cd web
node scripts/readme-shots.mjs                 # every picture, or name some: hero table
cd ../assets/readme
pngquant --quality 70-90 --force --ext .png hero.png styles.png table.png header.png columns.png
```

Each picture stays under 400 KB. The `made-*.png` pictures are copies of kition-showcase samples'
`thumb.png`. Every file in `assets/readme/` has a row in [attribution.md](attribution.md)
(`cargo xtask assets`).

## Publish

Kition ships the kit's package (`kition-doc-<version>.tgz`, a release asset of this repository,
pinned in Kition's `kits/kits.lock.json`). Publish it from a Kition checkout with
`scripts/publish-kit.sh doc` (see `docs/kits.md` there).

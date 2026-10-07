# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

The **native functions** of the Betty Blocks action runtime — the built-in steps a builder drags
onto an action canvas (create record, http, send mail, store file, expression, …). Each is a Rust
crate compiled to a WebAssembly component (`wasm32-wasip2`) and published to an OCI registry as a
versioned WIT package.

The repository is two layers:

- **Rust** — one component per directory, `functions/<name>/<version>/` (e.g. `functions/http/1.0/`).
- **Elixir** — a thin test harness only. It has no runtime role: `test/` loads each built `.wasm`
  with Wasmex and calls it for real, which is the only place the components are exercised
  end to end.

## ⚠️ wasmCloud v2, never v1

**All components target wasmCloud v2 tooling. Never introduce v1 patterns.**

| | v1 (do not use) | v2 (correct) |
|---|---|---|
| Config file | `wasmcloud.toml` | `.wash/config.yml` |
| CLI | `wash` 1.x | **`wash` 2.x** (2.3.0+; 2.0.0-rc builds are too old) |
| Build | `wash build` | `wash wit fetch` then `wash build --skip-fetch` |
| Target | `wasm32-wasi` (wasip1) | **`wasm32-wasip2`** |

No `wasmcloud.toml` remains anywhere and no component targets wasip1. If you find either, it is a
mistake, not a pattern to copy. Note that `wasm-base-components` is a **separate repository on its
own toolchain** (wit-bindgen versions vary per component) — read it for the shared WIT types it
defines, not as a template for component structure here.

Update wash with:

```sh
curl -fsSL https://wasmcloud.com/sh | INSTALL_DIR=$HOME/.local/bin bash
```

## Commands

From the repo root:

```bash
mix build            # just build in every function directory
mix test.components  # just test (cargo test) in every function directory
mix test             # Elixir tests — loads each .wasm in Wasmex and calls it
mix test --only live # only tests tagged @tag :live (real external API calls)
mix format --check-formatted
mix compile --warnings-as-errors   # CI runs this
```

From a single function directory:

```bash
just build   # wash wit fetch -> cargo build --release --target wasm32-wasip2 -> mv *.wasm .
just test    # cargo test
```

`mix build` iterates alphabetically, so an early failure hides every later function.

## WIT dependencies

**Always `wash wit fetch`. Never the standalone `wkg` binary.** The repo-root `wkg.toml` uses the
old combined wasm-pkg schema (`[namespace_registries]`); `wash` carries an older resolver that
still accepts it, while `wkg` 0.16+ rejects it outright:

```
unknown field `namespace_registries`, expected one of `workspace`, `overrides`, `metadata`
```

send-mail's Justfile called `wkg` and broke the build for everyone with a recent `wkg` installed.
All function Justfiles now use `wash`; keep it that way.

### Digest mismatch after a package is republished

```
package X (v2.0.0) has digest sha256:aaa… but the lock file specifies sha256:bbb…
```

`wash wit fetch` refuses to update a mismatched digest in place. Regenerate:

```sh
rm -rf wkg.lock wit/deps && just build
```

This happens whenever `betty-blocks-types:*` packages are republished under an unchanged version
number — a recurring source of repo-wide CI breakage, not a local problem.

The registry (`bettyblocksdev.azurecr.io`) is **per zone** — edge, acceptance and production
differ via `WASM_COMPONENTS_REGISTRY`. A package existing in dev does not mean it exists in prod.

## Versioning

`package betty-blocks:<name>@X.Y.Z;` on **line 1 of `wit/world.wit` drives the OCI tag**. The
release workflow extracts it with a `sed` regex, so a malformed first line means the component is
silently never published. `scripts/check-version-bumps.sh` runs on every PR and exempts new
components.

Bumping a shared type (`betty-blocks-types:types@2.2.0` → `@2.3.0`) means bumping the consuming
function's own package version too, and regenerating its lock.

## The IDE contract: flat arguments

`function.json` — the file describing a step's options in the IDE — is **generated, never
committed**.

### Publishing flows

Three exist; two are being retired:

1. **Upload custom WASM** in the action builder — generates function.json automatically (Block
   Store's own `WitParser`, used when the publish declaration carries *exactly* `name` and
   `version` and nothing else). Being removed, but currently the quickest way to test a component.
2. **CLI publish** from the root folder — the only flow where function.json is hand-written.
   Deprecated; don't add one by hand because this flow needed it.
3. **OCI registry push** — the future default and eventually the only flow. One component per
   push, no other files allowed.

Flow 3's pipeline: an Azure webhook fires on push → Block Store asks **wit-discovery** for the
functions in the registry → Block Store sends the WIT to the **wit-ui-generator** service →
generated function.json comes back and is stored as a block.

#### Pushing a component to the edge Block Store by hand (Flow 3)

To get a locally built component into the Block Store on edge for testing:

```sh
docker login -u <user> -p <token> wascodevdev.azurecr.io
wkg oci push wascodevdev.azurecr.io/wasco-dev/<function-name>:<version> <file>.wasm
```

e.g. `wkg oci push wascodevdev.azurecr.io/wasco-dev/ai-agent:1.0.0 ai_agent.wasm`.

Credentials are shared by the platform team — ask, don't commit them. This is the **dev**
registry (`wascodevdev`), which is separate from the one `wkg.toml` maps for WIT dependency
resolution (`bettyblocksdev`). Pushing here only makes the component available in the edge Block
Store; it is not a production release.

WIT carries no Betty metadata (icon, colour, description, category, allowed kinds) — OCI is an
open standard with nowhere to put it — so wit-ui-generator hardcodes defaults (default action
icon, orange, category `WASM`) and custom Betty types are matched **by name**. Overriding metadata
today means editing the block store database directly; a JSON editor in MyBB is in progress.

To re-test a wit-ui-generator change: delete the block's row from the block store database, then
press the webhook/ping button to re-trigger the whole generation flow.

**One WIT function argument becomes one IDE option**, and a `record` argument is *always* rendered
as a single opaque SchemaModel/OBJECT (wit-ui-generator ADR 004). So step configuration must be
**flat function arguments**, never wrapped in an `input` record:

```wit
// wrong — collapses into one opaque blob in the IDE
ai-agent: func(input: input) -> result<output, string>;

// right — one option per argument
ai-agent: func(provider: ai-provider, instructions: string, message: string,
               max-tokens: option<u32>) -> result<string, string>;
```

Type mapping: `string`/`char` → Text, ints/floats → Number, `bool` → Boolean, `enum` → Select,
`record` → SchemaModel + OBJECT, `list` → ARRAY, `json-string` alias → Map. `option<T>` unwraps
and sets `required: false`.

Betty Blocks types (`betty-model`, `betty-property`, `betty-ai-provider`, …) are defined in
`wasm-base-components/wit/types/types.wit` and recognised by `wit-ui-generator` through a registry
keyed on fully-qualified path + **major** version. A brand-new shared type needs a matching
registry entry there, or it renders as a generic opaque object.

## Rust conventions

- **`wstd` for HTTP**, not `std::net` — WASM has no system networking. Prefer `wstd` over `waki`:
  it exposes connect, first-byte and between-bytes timeouts, which matter for long-running AI
  calls. `wstd::runtime::block_on` bridges a synchronous WIT export to async code.
- **Trait-based seams for testing.** An `HttpClient` trait with a real impl and a mock
  (`test_helpers.rs`) lets provider/business logic be unit-tested under plain `cargo test`, with
  no wasm runtime. Copy this pattern rather than calling a concrete client directly.
- Errors are `result<T, string>` everywhere — a plain message, never a variant.
- Never echo an upstream error body into a returned error: those requests carry API keys.
- Unit tests live beside the code they test, in `#[cfg(test)] mod tests`.

## Elixir test layer

`test/native_wasm_components/<name>_test.exs` loads the built `.wasm` through
`TestHelper.run_component/4` and calls the exported function. **Build before testing** — these
tests read the `.wasm` from disk.

- `Sham` starts a real localhost Bandit server, so the component makes a genuine socket call
  (Wasmex runs with `allow_http: true`). Assert on the outgoing request inside `Sham.expect/2`.
- Tests hitting a real external API are `@tag :live` and excluded by default via
  `ExUnit.start(exclude: [:live])`.

WIT ↔ Elixir marshalling, confirmed against the existing tests:

- The interface id is `{"betty-blocks:<name>/<interface>@<version>", "<function>"}`.
- A `record` argument is a map keyed by **kebab-case field names as strings** (`"api-key"`).
- `option<T>` is `{:some, value}` or `:none`.
- Coming back out, keys are inconsistent: kebab names arrive as strings, single-word names as
  atoms. Assert the real shape; don't "fix" it.

## Gotchas

- **A `functions/<name>/<version>/` directory without a `Justfile` used to fork bomb the machine.**
  `just` walks *up* the tree when it finds no local Justfile, reaches the root recipe, and
  re-enters the loop. Both `Justfile` and `mix.exs` now skip such directories. This state arises
  naturally: switching away from a branch that adds a component leaves its gitignored `target/`
  behind while the tracked files vanish.
- **CI pins neither `wash` nor `wkg`** — it installs the latest of both, so local-versus-CI
  tooling drift is a real failure mode.
- `.wasm` files are gitignored. Build artifacts are never committed.

## Branch strategy

Promotion runs feature → `edge` → `acceptance` → `main`. Open feature PRs against **`edge`**
(note `origin/HEAD` points at `main`, which is *not* the PR target). Never merge `edge` into a
feature branch — merge `acceptance` if you need to catch up.
- ⚠️ **This repository is on GitHub**, unlike most Betty Blocks repositories (GitLab). Use `gh`,
  not `glab`.

## Elixir Skills

If elixir-phase-skills are installed at `~/.claude/skills/`, read and apply the relevant phase
skill when writing Elixir code:

- `elixir-implementing` — idiomatic patterns, testing
- `elixir-planning` — architecture, supervision
- `elixir-reviewing` — code review checklists

## Coding Standards & Guidelines

@.claude/standards/CLAUDE.md

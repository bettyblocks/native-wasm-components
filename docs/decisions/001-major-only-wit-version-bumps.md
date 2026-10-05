# Major-only version bumps for function components

## Status

**Superseded.** Major-only bumps are no longer required. `scripts/check-version-bumps.sh` now
accepts any version increase — major, minor or patch — as long as the version goes up, matching
the same change in `wasm-base-components`. See [Current policy](#current-policy).

The analysis below of what `wit-parser` merges inside one compatibility class still holds, and is
kept as the reason breaking changes should take a new major.

Originally applied to every component with a `wit/world.wit` in this repository — the function
components under `functions/` (namespace `betty-blocks`).

This was the same policy as
[`wasm-base-components` ADR 001](https://github.com/bettyblocks/wasm-base-components/blob/main/docs/decisions/001-major-only-wit-version-bumps.md),
applied to this repository's components. That ADR carries the full evidence; the summary below
is what matters here.

## Context

`wit-parser` buckets every version into a **semver compatibility class** — the leftmost non-zero
component, so `2.0.0`, `2.1.0` and `2.3.0` are all class `2`. When one build ends up with two
versions of a package in the same class, `Resolve::merge_world_imports_based_on_semver` picks the
highest and silently rewrites the older consumer's imports onto it.

The check run before that rewrite compares type **names**, function signatures and the flattened
core ABI — never the inside of a named type. So every layout-preserving edit passes: a renamed
record field, a reordered field, a `u32`→`s32` flip. The older consumer is rebound onto the new
memory layout without warning.

Actions-compiler's Resolve set-unions an app's dependencies, so builds carrying two versions of
one package are the normal case, not an edge case.

## Decision

**Every published release of a component increments the major. Minor and patch are always `0`.**

```
2.0.0  ->  3.0.0  ->  4.0.0  ->  5.0.0
```

Equivalently, stated as the invariant that actually matters:

> Two versions of one WIT package must never appear in a single build within the same
> compatibility class. Every published release gets a new compatibility class.

The version that counts is the one in `<component>/wit/world.wit`:

```wit
package betty-blocks:store-file@3.0.0;
```

It is independent of the component's `Cargo.toml` version and of the `functions/<name>/<version>/`
directory (the Betty function version). `release.yaml` derives the Azure registry tag from the
`world.wit` version, so it is the version that is actually published.

### Enforcement

Was enforced in CI by `scripts/check-version-bumps.sh`, run by the **Version Check** workflow on
every PR. No longer enforced — see [Current policy](#current-policy).

## Consequences

**No consolidation, ever.** Two consumers on two versions of one component means two components
deployed, where same-class merging would have collapsed them to one. That is the price of
refusing the unchecked rebind, and it grows unless consumers are deliberately moved forward onto
the current major.

## Current policy

**Every published release of a component must increase its version.** Any increase — major,
minor or patch — is accepted, as long as the version goes up.

```
2.0.0  ->  2.1.0  ->  2.1.1  ->  3.0.0
```

Pick the bump by semver, and treat a breaking change as a major: a breaking change shipped as a
minor or patch is silently rebound onto older consumers in the same compatibility class (see
[Context](#context)).

`scripts/check-version-bumps.sh`, run by the **Version Check** workflow on every PR, compares the
`world.wit` version of each component whose `src/`, `build.rs`, `Cargo.toml` or `wit/` (excluding
the fetched `wit/deps/`) changed against the same file on the PR's base branch:

- changed and not bumped → ❌, unless the base is `edge`, where reuse is allowed;
- changed and the version went down → ❌, on every base;
- a component with no version on the base is new and may start at any version.

The `edge` exception mirrors the `dev` exception in `wasm-base-components`: `edge` is an
integration environment rather than a production one, so overwriting a version that is still in
development is safe. Promotion PRs into `acceptance` and `main` compare against a branch real
apps run from, so every changed component must show a version bump there.

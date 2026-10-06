# Edge builds re-resolve WIT deps instead of verifying wkg.lock

## Status

**Accepted.** Applies to every component under `functions/`.

## Context

`wasm-base-components` publishes every merge to its `dev` branch under the version the package
already has. Its "dev exception" allows this: a package still in development may reuse its
version instead of bumping it per PR. So `betty-blocks-types:types@3.1.0` gets new content, and a
new digest, on every dev publish.

This repository commits a `wkg.lock` per component, which pins each dependency's **content
digest**. `wkg.toml` points `betty-blocks-types` at the dev registry for local development and
PR CI. After every dev publish, the lock no longer matches the registry, and `fetch-wit-deps`
fails:

```
component registry package `betty-blocks-types:types` (v`3.1.0`) has digest `sha256:6f24…`
but the lock file specifies digest `sha256:173e…`
```

Refreshing the locks by hand after every publish does not last, because the next publish breaks
them again. A moving version and a lock that pins its content contradict each other.

## Decision

**A build that targets `edge` re-resolves its WIT dependencies from the registry and ignores the
committed `wkg.lock`. Builds that target `acceptance` and `main` keep verifying it.**

- **Edge** is where development against `dev` packages happens, so it builds against whatever
  `dev` last published. That is the point of the dev exception.
- **Acceptance and main** build what apps run. A package that reaches them has been released off
  `dev`, so its tag no longer moves and the lock stays valid. Before promoting, run
  `just refresh-wit-locks` once and commit the result.

### How

- Every function's `fetch-wit-deps` recipe removes `wkg.lock` and `wit/deps` before fetching when
  `REFRESH_WIT_LOCKS=true`. With neither present, `wash wit fetch` and `wkg wit fetch` resolve
  every dependency from the registry and write a fresh lock.
- `ci.yml` sets the variable when a pull request's base, or a push's branch, is `edge`.
  `release.yaml` and the Docker build (`build.yaml` → `Dockerfile` build arg) set it on pushes to
  `edge`.
- `just refresh-wit-locks` at the repository root does the same for every component locally.
  Locally, a stale `~/.cache/wash/package_cache` can still map a version to an old digest; clear
  it if a refreshed lock disagrees with CI.

## Alternatives considered

- **Immutable dev versions** (prereleases such as `3.1.0-dev.7`, a new one per publish). Locks
  would never go stale, but every consumer's WIT would need a new reference on every dev publish,
  and a rewrite to `3.1.0` at release. That is the churn the dev exception was introduced to
  avoid.
- **Not committing `wkg.lock` at all.** This would also drop the reproducibility that acceptance
  and main rely on.

## Consequences

- Edge builds are not reproducible across dev publishes. The same commit can build against
  different `dev` content on different days.
- Committed locks can be stale on `edge` without anything failing. They only have to be right
  when they reach `acceptance`.

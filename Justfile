find-wasms:
  find functions -name '*.wasm' -not -path "*/target/*"

build:
  #!/usr/bin/env sh
  for path in functions/*/*; do
    (cd "$path" && just build)
  done
clean:
  #!/usr/bin/env sh
  for path in functions/*/*; do
    (cd "$path" && rm -rf target)
  done

# Re-resolve every component's WIT deps from the registry and rewrite its wkg.lock.
# Needed after a betty-blocks-types publish on wasm-base-components' dev, which
# overwrites the version tag (docs/decisions/002).
refresh-wit-locks:
  #!/usr/bin/env sh
  set -e
  for path in functions/*/*; do
    (cd "$path" && REFRESH_WIT_LOCKS=true just fetch-wit-deps)
  done

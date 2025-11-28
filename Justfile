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

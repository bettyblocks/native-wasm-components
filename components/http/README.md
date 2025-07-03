# Logging

Http component that works similarly to the native function http. The actual sending of the request is done via the wasi:http/outgoing-handler interface.

## Building

requires rust with target wasm32-wasip2 installed and the `wkg` tool for fetching the wit dependencies.

run:

```sh
just build
```

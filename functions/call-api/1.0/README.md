# Call API

Stub for the Call API action. It takes a `betty-blocks-types:types.betty-api`
(`id`, `software-name`) and, for now, only echoes it back as JSON:

```json
{"id": "1", "softwareName": "Exact"}
```

A blank `software-name` returns `Err("No API selected")`. No HTTP call is made
yet; the component exists to pin down the WIT and Rust types.

## Building

```sh
just build
```

## Testing

```sh
just test   # cargo test
mix test test/native_wasm_components/call_api_test.exs   # from the repo root
```

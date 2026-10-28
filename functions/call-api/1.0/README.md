# Call API

Stub for the Call API action. It takes a `betty-blocks-types:types.betty-api`
(`software-name`, `api-name`, `base-url`, `path`, `method`, `authentication`)
and, for now, only describes the call as JSON:

```json
{"softwareName": "Exact", "apiName": "Get a user", "method": "get",
 "url": "https://api.example.com/users/{id}", "authentication": "none"}
```

`url` is `base-url` and `path` joined with exactly one slash. `authentication`
is only the kind (`none`, `api-key` or `oauth2`); secrets are never returned.
An empty `base-url` or `path` returns an error. No HTTP call is made yet; the
component exists to pin down the WIT and Rust types.

## Building

```sh
just build
```

## Testing

```sh
just test   # cargo test
mix test test/native_wasm_components/call_api_test.exs   # from the repo root
```

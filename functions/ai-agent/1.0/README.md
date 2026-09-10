# AI Agent

Calls an AI model from an AI Agent action. Takes a provider configuration,
instructions (the system prompt) and a message (the user prompt), and returns the
model's text.

Only Anthropic is supported. Providers are dispatched on `provider.provider`, so
adding one is a module implementing the `Provider` trait plus a match arm in
`lib.rs`. Outbound HTTP goes through the `HttpClient` trait, which keeps provider
modules testable without a wasm runtime.

The API key arrives in the input, not from the environment.

## Configuration

| Variable | Default |
| --- | --- |
| `ANTHROPIC_API_URL` | `https://api.anthropic.com/v1/messages` |
| `ANTHROPIC_API_VERSION` | `2023-06-01` |

`max-tokens` is optional on the input and defaults to 4096. Anthropic requires the
field, and it truncates silently when the limit is reached.

## Building

Requires rust with the `wasm32-wasip2` target installed, plus `wash` 2.x for
fetching the wit dependencies.

```sh
just build
```

## Testing

```sh
just test
```

Unit tests run on the host against a mock HTTP client. The component itself is
exercised from Elixir in `test/native_wasm_components/ai_agent_test.exs`, which
points `ANTHROPIC_API_URL` at a local server:

```sh
mix test
```

That suite never calls Anthropic. To run the one test that does:

```sh
ANTHROPIC_API_KEY=... mix test --include live
```

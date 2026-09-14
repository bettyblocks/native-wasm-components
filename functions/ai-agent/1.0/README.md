# AI Agent

Calls an AI model from an AI Agent action. Takes a provider configuration,
instructions (the system prompt) and a message (the user prompt), and returns the
model's text.

Only Anthropic is supported. Providers are dispatched on `provider.name`, so
adding one is a module implementing the `Provider` trait plus a match arm in
`lib.rs`. Outbound HTTP goes through the `HttpClient` trait, which keeps provider
modules testable without a wasm runtime.

The `ai-provider` record mirrors `betty-blocks-types:types.betty-ai-provider`,
with `api-key` added. Once that type is published the local record is replaced by
a `use`, and the key moves to a secret read from the environment.

## Configuration

The endpoint comes from `provider.url`, not the environment.

| Variable | Default |
| --- | --- |
| `ANTHROPIC_API_VERSION` | `2023-06-01` |

`max-tokens` is optional and defaults to 4096. Anthropic requires the field, and
it truncates silently when the limit is reached.

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
passes a local server's URL as the provider's `url`:

```sh
mix test
```

That suite never calls Anthropic. To run the one test that does:

```sh
ANTHROPIC_API_KEY=... mix test --only live
```

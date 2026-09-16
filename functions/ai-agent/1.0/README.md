# AI Agent

Calls an AI model from an AI Agent action. Takes a provider configuration, a
system prompt and a prompt, and returns the model's text.

Only Anthropic is supported. Providers are dispatched on `provider.name`, so
adding one is a module in `src/providers/` implementing the `Provider` trait,
plus a match arm in `lib.rs`. Outbound HTTP goes through the `HttpClient` trait, which keeps provider
modules testable without a wasm runtime.

The provider argument is `betty-blocks-types:types.betty-ai-provider`.

## Configuration

Everything about the provider — endpoint, model, and API key — comes from the
`betty-ai-provider` record the runtime passes in, not from the environment. The
component reads `provider.api-key` and never looks the secret up itself.

`provider.url` is a **base** url — `https://api.anthropic.com/v1` — and the
component appends the endpoint it needs (`/messages`), with or without a
trailing slash on the base.

Nothing is read from the environment — a component is never granted one. The
Anthropic API version is the `API_VERSION` constant in
`src/providers/anthropic.rs`.

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
passes a local server's base url as the provider's `url`:

```sh
mix test
```

That suite never calls Anthropic. To run the one test that does:

```sh
ANTHROPIC_API_KEY=... mix test --only live
```

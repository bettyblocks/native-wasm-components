# Prompt Call

Calls an AI model from a Prompt Call action. Takes a provider configuration, a
system prompt and a prompt, and returns the model's text.

Anthropic, Mistral, Google and OpenAI are supported. Providers are dispatched on `provider.name`, so
adding one is a module in `src/providers/` implementing the `Provider` trait,
plus a match arm in `lib.rs`. Outbound HTTP goes through the `HttpClient` trait, which keeps provider
modules testable without a wasm runtime.

The provider argument is `betty-blocks-types:types.betty-ai-provider`.

## Configuration

Everything about the provider — endpoint, model, and API key — comes from the
`betty-ai-provider` record the runtime passes in, not from the environment. The
component reads `provider.api-key` and never looks the secret up itself.

`provider.url` is a **base** url — `https://api.anthropic.com/v1` or
`https://api.mistral.ai/v1` — and the component appends the endpoint it needs
(`/messages` for Anthropic, `/conversations` for Mistral), with or without a
trailing slash on the base.

Nothing is read from the environment — a component is never granted one. The
Anthropic API version is the `API_VERSION` constant in
`src/providers/anthropic.rs`.

`max-tokens` is optional and defaults to 4096. Anthropic requires the field, and
it truncates silently when the limit is reached. Mistral takes completion
parameters under `completion_args` in the Conversations API; the component
always sends `max_tokens` there.

Tool descriptions (http-search, mcp servers) are appended to the system prompt
for every provider. The Conversations API only accepts `user` and `assistant`
input roles, so for Mistral the system prompt travels inside the user message.

Anthropic additionally registers the tools server-side (`tools` /
`mcp_servers`) in the request body. Mistral executes tools server-side on the
Conversations API: an http-search tool becomes the built-in
`{"type": "web_search"}` tool.

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
exercised from Elixir in `test/native_wasm_components/prompt_call_test.exs`, which
passes a local server's base url as the provider's `url`:

```sh
mix test
```

That suite never calls Anthropic or Mistral. To run the live tests:

```sh
ANTHROPIC_API_KEY=... mix test --only live
MISTRAL_API_KEY=... mix test --only live
```

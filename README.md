# NativeWasmComponents

Native Wasm components used by Betty Blocks in the Action Builder.

## Installation

Install Elixir with any way you like (I would suggest using asdf/[mise](https://mise.jdx.dev/getting-started.html))

```sh
mix deps.get
```

## Building

building the wasm components can be done via `mix build`. This will output the components in the `target/wasm32-wasip2/release` folder.

## Testing

component tests are ran via Elixir WasmEx.

```sh
mix test
```

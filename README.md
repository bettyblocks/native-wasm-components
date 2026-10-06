# NativeWasmComponents

Native Wasm components used by Betty Blocks in the Action Builder.

## Development

[Just](https://github.com/casey/just) is used for executing commands inside the components folder.

It follows this pattern:

```sh
# builds the wasm component
just build
```

```sh
# runs the unit tests for the component/code
just test
```

[Wkg](https://github.com/bytecodealliance/wasm-pkg-tools#installation) is needed to fetch the wasi wit dependencies 

### Installation

Install Elixir with any way you like (I would suggest using asdf/[mise](https://mise.jdx.dev/getting-started.html))

```sh
mix deps.get
```

### Building

building the wasm components can be done via `mix build`. This will call `just build` in each component folder. This will output the components in the `target/wasm32-wasip2/release` folder.

### WIT dependency locks

Each component commits a `wkg.lock`. Builds that target `edge` ignore it and re-resolve from the
registry, because `wasm-base-components`' `dev` overwrites version tags on publish. Run
`just refresh-wit-locks` to rewrite every lock, for example before promoting to `acceptance`. See
[ADR 002](docs/decisions/002-edge-builds-re-resolve-wit-locks.md).

### Testing

component tests are ran via Elixir Wasmex.

```sh
mix test
```

To run all the unit tests for the component run:

```sh
mix test.components
```

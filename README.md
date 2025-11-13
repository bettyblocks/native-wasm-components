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

### Testing

component tests are ran via Elixir Wasmex.

```sh
mix test
```

To run all the unit tests for the component run:

```sh
mix test.components
```

### Running on locally

For running locally, there is a `wasmcloud/local.wamd.yaml` file. The Github components / providers are pulled from the Github repository. However, for components / providers in private repositories, you need to push them to a local repository and pull them from there. Currently, the local repository is set to be `wasmcloud-registry:5000`.

The steps to run it then are:

1. Run a wasmcloud server, for example with `wash dev`
2. Deploy the private provider / components to the local repository with `wash push` to the specified registry
3. Change the secret to the correct secret in the key-vault provider
4. Deploy the app: `wash app deploy local.wadm.yaml`

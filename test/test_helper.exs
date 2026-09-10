defmodule TestHelper do
  @moduledoc false

  def run_component(wasm, function, args, imports \\ %{}, env \\ %{}) do
    {:ok, pid} =
      Wasmex.Components.start_link(%{
        path: wasm,
        wasi: %Wasmex.Wasi.WasiP2Options{allow_http: true, env: env},
        imports: imports
      })

    {:ok, result} = Wasmex.Components.call_function(pid, function, List.wrap(args))
    result
  end
end

ExUnit.start(exclude: [:live])

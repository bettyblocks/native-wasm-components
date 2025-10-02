defmodule NativeWasmComponents.MixProject do
  use Mix.Project

  def project do
    [
      app: :native_wasm_components,
      version: "0.1.0",
      elixir: "~> 1.17",
      start_permanent: Mix.env() == :prod,
      preferred_cli_env: [
        "test.components": :test
      ],
      deps: deps(),
      aliases: aliases()
    ]
  end

  # Run "mix help compile.app" to learn about applications.
  def application do
    [
      extra_applications: [:logger]
    ]
  end

  # Run "mix help deps" to learn about dependencies.
  defp deps do
    [
      {:wasmex, "~> 0.12"},
      {:styler, "~> 1.4", only: [:dev, :test], runtime: false},
      {:jason, "~> 1.0"},
      {:bandit, "~> 1.0"},
      {:sham, "~> 1.2"}
    ]
  end

  def aliases do
    [
      build: &builder/1,
      "test.components": &test_components/1,
      "test.providers": &test_providers/1
    ]
  end

  defp builder(_args) do
    "components/*"
    |> Path.wildcard()
    |> Enum.map(fn path ->
      {_, 0} = System.cmd("just", ["build"], cd: path, into: IO.stream())
    end)
  end

  defp test_components(_args) do
    "components/*"
    |> Path.wildcard()
    |> Enum.map(fn path ->
      {_, 0} = System.cmd("just", ["test"], cd: path, into: IO.stream())
    end)
  end

  defp test_providers(_args) do
    {_, status} = System.cmd("just", [], cd: "wasmcloud/", into: IO.stream())
  end
end

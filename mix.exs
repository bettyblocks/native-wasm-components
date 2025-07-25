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
      extra_applications: [:logger],
      mod: {NativeWasmComponents.Application, []}
    ]
  end

  # Run "mix help deps" to learn about dependencies.
  defp deps do
    [
      {:wasmex,
       git: "https://github.com/tessi/wasmex.git", rev: "e8d2f63cdf278ced11720cc58d93f96f72cb9872"},
      {:styler, "~> 1.4", only: [:dev, :test], runtime: false},
      {:jason, "~> 1.0"},
      {:plug, "~> 1.0"},
      {:bandit, "~> 1.0"}
    ]
  end

  def aliases do
    [
      format: &format_helper/1,
      build: &builder/1,
      test: &test_helper/1,
      "test.components": &test_components/1
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

  defp test_helper(args) do
    System.put_env("WASMEX_BUILD", "1")

    Mix.Task.run("test", args)
  end

  defp format_helper(args) do
    System.put_env("WASMEX_BUILD", "1")

    Mix.Task.run("format", args)
  end
end

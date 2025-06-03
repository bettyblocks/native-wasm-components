defmodule NativeWasmComponents.MixProject do
  use Mix.Project

  def project do
    [
      app: :native_wasm_components,
      version: "0.1.0",
      elixir: "~> 1.17",
      start_permanent: Mix.env() == :prod,
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
      {:wasmex,
       git: "https://github.com/tessi/wasmex.git", rev: "e8d2f63cdf278ced11720cc58d93f96f72cb9872"},
      {:styler, "~> 1.4", only: [:dev, :test], runtime: false}
    ]
  end

  def aliases do
    [
      format: &format_helper/1,
      build: &builder/1,
      test: &test_helper/1
    ]
  end

  defp builder(_args) do
    {_, 0} = System.cmd("just", ["build"], cd: "components/logging", into: IO.stream())
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

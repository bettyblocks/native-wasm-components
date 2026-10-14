# Runs the prompt-call component from the terminal.
#
# Usage, from the repository root:
#
#   mix run functions/prompt-call/1.0/harness/call.exs --prompt "Why is the sky blue?"
#
# Options:
#   --prompt      The user message. Required unless - (read from stdin).
#   --system      System prompt (default: "You are helpful.").
#   --model       Model id (default: claude-sonnet-5).
#   --url         Provider base url (default: https://api.anthropic.com/v1).
#   --api-key     Anthropic api key; falls back to ANTHROPIC_API_KEY.
#   --max-tokens  Output cap; falls back to the component default of 4096.
#   --timeout     Response timeout in ms (default: 120000; Wasmex's own
#                 default is 5000, which a real Anthropic call easily exceeds).
#   --mcp-url     Attach an mcp server. Without it no tools are sent at all.
#   --mcp-token   Bearer token for that server; omit for an open server.
#   --enabled-tools  Which of the server's tools the agent may use:
#                 omitted      -> none        (no restriction, legacy behaviour)
#                 "a,b"        -> some([a,b]) (only those two)
#                 ""           -> some([])    (every tool disabled)
#
# The component must be built first: `just build` in
# functions/prompt-call/1.0 (or `mix build` from the root).

default_provider_url = "https://api.anthropic.com/v1"

{opts, _argv, invalid} =
  OptionParser.parse(System.argv(),
    strict: [
      prompt: :string,
      system: :string,
      model: :string,
      url: :string,
      api_key: :string,
      max_tokens: :integer,
      timeout: :integer,
      mcp_url: :string,
      mcp_token: :string,
      enabled_tools: :string
    ],
    aliases: [p: :prompt]
  )

usage = fn message ->
  IO.puts(:stderr, "error: #{message}")
  IO.puts(:stderr, ~s(usage: mix run functions/prompt-call/1.0/harness/call.exs --prompt "..."))
  System.halt(2)
end

if invalid != [], do: usage.("unknown option: #{inspect(invalid)}")

prompt =
  case Keyword.get(opts, :prompt) do
    nil -> usage.("missing --prompt")
    "-" -> IO.read(:stdio, :eof)
    prompt -> prompt
  end

api_key = Keyword.get(opts, :api_key) || System.get_env("ANTHROPIC_API_KEY") || ""

max_tokens =
  case opts |> Keyword.get(:max_tokens) |> IO.inspect(label: "MAX TOKEN") do
    nil -> :none
    n when is_integer(n) and n > 0 -> {:some, n}
    _ -> usage.("--max-tokens must be a positive integer")
  end

timeout =
  case Keyword.get(opts, :timeout, 120_000) do
    n when is_integer(n) and n > 0 -> n
    _ -> usage.("--timeout must be a positive integer")
  end

option = fn
  nil -> :none
  value -> {:some, value}
end

# Absent means `none` (no restriction); an empty string means `some([])`, which
# disables every tool. Those are different states, so match before splitting.
enabled_tools =
  case Keyword.get(opts, :enabled_tools) do
    nil ->
      :none

    "" ->
      {:some, []}

    names ->
      {:some, names |> String.split(",") |> Enum.map(&String.trim/1) |> Enum.reject(&(&1 == ""))}
  end

tools =
  case Keyword.get(opts, :mcp_url) do
    nil ->
      :none

    mcp_url ->
      {:some,
       [
         {:mcp,
          %{
            "description" => :none,
            "url" => mcp_url,
            "authentication" => %{
              "kind" => "access_token",
              "value" => option.(Keyword.get(opts, :mcp_token))
            },
            "enabled-tools" => enabled_tools
          }}
       ]}
  end

provider = %{
  "name" => "anthropic",
  "ai-model-name" => Keyword.get(opts, :model, "claude-sonnet-5"),
  "url" => Keyword.get(opts, :url, default_provider_url),
  "tools" => tools,
  "api-key" => api_key
}

interface = {"betty-blocks:prompt-call/prompt-call@1.1.0", "prompt-call"}
component = Path.expand("../prompt_call.wasm", __DIR__)

if !File.exists?(component) do
  IO.puts(
    :stderr,
    "error: #{component} not found — run `just build` in functions/prompt-call/1.0"
  )

  System.halt(2)
end

case Wasmex.Components.start_link(%{
       path: component,
       wasi: %Wasmex.Wasi.WasiP2Options{allow_http: true},
       imports: %{}
     }) do
  {:ok, pid} ->
    args = [
      provider,
      Keyword.get(opts, :system, "You are helpful."),
      prompt,
      max_tokens
    ]

    case Wasmex.Components.call_function(pid, interface, args, timeout) do
      {:ok, {:ok, text}} ->
        IO.puts(text)
        System.halt(0)

      {:ok, {:error, reason}} ->
        IO.puts(:stderr, "error: #{reason}")
        System.halt(1)

      {:error, reason} ->
        IO.puts(:stderr, "error: #{inspect(reason)}")
        System.halt(1)
    end

  {:error, reason} ->
    IO.puts(:stderr, "error: failed to start component: #{inspect(reason)}")
    System.halt(1)
end

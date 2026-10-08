defmodule NativeWasmComponents.PromptCallTest do
  use ExUnit.Case, async: true

  @component_path "functions/prompt-call/1.0/prompt_call.wasm"
  @interface {"betty-blocks:prompt-call/prompt-call@1.0.0", "prompt-call"}

  defp run_component(args) do
    TestHelper.run_component(@component_path, @interface, args)
  end

  defp build_args(url, overrides \\ %{}) do
    provider =
      Map.merge(
        %{
          "name" => "anthropic",
          "ai-model-name" => "claude-sonnet-5",
          "url" => url,
          "tools" => :none,
          "api-key" => "test-key"
        },
        Map.get(overrides, :provider, %{})
      )

    [
      provider,
      Map.get(overrides, :instructions, "You are helpful."),
      Map.get(overrides, :message, "What is 2 + 2?"),
      Map.get(overrides, :max_tokens, :none)
    ]
  end

  defp anthropic_response(text) do
    Jason.encode!(%{content: [%{type: "text", text: text}]})
  end

  describe "prompt-call component" do
    setup do
      sham = Sham.start()

      {:ok, %{sham: sham, url: "http://localhost:#{sham.port}/v1"}}
    end

    test "returns the assistant text", %{sham: sham, url: url} do
      Sham.expect(sham, fn conn ->
        Plug.Conn.send_resp(conn, 200, anthropic_response("4"))
      end)

      assert {:ok, "4"} == run_component(build_args(url))
    end

    test "appends the messages endpoint to the provider base url", %{sham: sham, url: url} do
      Sham.expect(sham, fn conn ->
        assert conn.path_info == ["v1", "messages"]
        assert {"x-api-key", "test-key"} in conn.req_headers
        assert {"anthropic-version", "2023-06-01"} in conn.req_headers
        assert {"content-type", "application/json"} in conn.req_headers

        {:ok, body, conn} = Plug.Conn.read_body(conn, length: 1_000_000)

        assert %{
                 "model" => "claude-sonnet-5",
                 "max_tokens" => 4096,
                 "system" => "You are helpful.",
                 "messages" => [%{"role" => "user", "content" => "What is 2 + 2?"}]
               } == Jason.decode!(body)

        Plug.Conn.send_resp(conn, 200, anthropic_response("4"))
      end)

      assert {:ok, "4"} == run_component(build_args(url))
    end

    test "sends tool calls in the request", %{sham: sham, url: url} do
      tools = [
        {:"http-search", %{"description" => :none}},
        {:mcp,
         %{
           "description" => {:some, "call this tool if the prompt contain \"mcp call\""},
           "url" => "https://gateway.mcpservers.org/yahoo-finance/mcp",
           "authentication" => %{"kind" => "access_token", "value" => {:some, "secret-token"}}
         }}
      ]

      Sham.expect(sham, fn conn ->
        assert {"anthropic-beta", "mcp-client-2025-11-20"} in conn.req_headers

        {:ok, body, conn} = Plug.Conn.read_body(conn, length: 1_000_000)
        request = Jason.decode!(body)

        assert [
                 %{
                   "type" => "web_search_20260318",
                   "name" => "web_search",
                   "allowed_callers" => ["direct"]
                 },
                 %{"type" => "mcp_toolset", "mcp_server_name" => "gateway-mcpservers-org"}
               ] == request["tools"]

        assert [
                 %{
                   "type" => "url",
                   "url" => "https://gateway.mcpservers.org/yahoo-finance/mcp",
                   "name" => "gateway-mcpservers-org",
                   "authorization_token" => "secret-token"
                 }
               ] == request["mcp_servers"]

        assert request["system"] =~ "Tools:"

        assert request["system"] =~
                 ~s|- mcp-server https://gateway.mcpservers.org/yahoo-finance/mcp: call this tool if the prompt contain "mcp call"|

        Plug.Conn.send_resp(conn, 200, anthropic_response("ok"))
      end)

      assert {:ok, "ok"} ==
               run_component(build_args(url, %{provider: %{"tools" => {:some, tools}}}))
    end

    test "honours an explicit max-tokens", %{sham: sham, url: url} do
      Sham.expect(sham, fn conn ->
        {:ok, body, conn} = Plug.Conn.read_body(conn, length: 1_000_000)

        assert %{"max_tokens" => 256} = Jason.decode!(body)

        Plug.Conn.send_resp(conn, 200, anthropic_response("ok"))
      end)

      assert {:ok, "ok"} == run_component(build_args(url, %{max_tokens: {:some, 256}}))
    end

    test "joins multiple text blocks", %{sham: sham, url: url} do
      response =
        Jason.encode!(%{content: [%{type: "text", text: "2 + 2 = "}, %{type: "text", text: "4"}]})

      Sham.expect(sham, fn conn -> Plug.Conn.send_resp(conn, 200, response) end)

      assert {:ok, "2 + 2 =\n\n4"} == run_component(build_args(url))
    end

    test "reports a non-success status", %{sham: sham, url: url} do
      Sham.expect(sham, fn conn -> Plug.Conn.send_resp(conn, 429, "slow down") end)

      assert {:error, "Anthropic returned status 429: slow down"} ==
               run_component(build_args(url))
    end

    test "rejects an unsupported provider", %{url: url} do
      args = build_args(url, %{provider: %{"name" => "openai", "ai-model-name" => "gpt-5.4"}})

      assert {:error, "Unsupported provider: openai"} == run_component(args)
    end

    test "rejects a provider without an api key", %{url: url} do
      args = build_args(url, %{provider: %{"api-key" => ""}})

      assert {:error, "No API key configured for agent: anthropic"} == run_component(args)
    end

    test "rejects an empty url" do
      args = build_args("")

      assert {:error, "No URL configured for agent: anthropic"} == run_component(args)
    end
  end

  describe "against the real anthropic api" do
    @tag :live
    test "completes a prompt" do
      key =
        System.get_env("ANTHROPIC_API_KEY") ||
          flunk("set ANTHROPIC_API_KEY to run the live test")

      args =
        build_args("https://api.anthropic.com/v1", %{
          provider: %{"api-key" => key},
          instructions: "Reply with a single word and no punctuation.",
          message: "Reply with the word pong"
        })

      assert {:ok, text} = run_component(args)
      assert text =~ "pong"
    end
  end
end

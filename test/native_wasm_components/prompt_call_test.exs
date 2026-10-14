defmodule NativeWasmComponents.PromptCallTest do
  use ExUnit.Case, async: true

  @component_path "functions/prompt-call/1.0/prompt_call.wasm"
  @interface {"betty-blocks:prompt-call/prompt-call@1.1.0", "prompt-call"}

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

  defp openai_response(text) do
    Jason.encode!(%{
      output: [
        %{type: "message", role: "assistant", content: [%{type: "output_text", text: text}]}
      ]
    })
  end

  defp openai_args(url, overrides \\ %{}) do
    provider =
      Map.merge(
        %{"name" => "openai", "ai-model-name" => "gpt-5.4"},
        Map.get(overrides, :provider, %{})
      )

    build_args(url, Map.put(overrides, :provider, provider))
  end

  defp mcp_tool(selected) do
    {:mcp,
     %{
       "description" => :none,
       "url" => "https://mcp.example.com/mcp",
       "authentication" => %{"kind" => "access_token", "value" => :none},
       "enabled-tools" => selected
     }}
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
           "authentication" => %{"kind" => "access_token", "value" => {:some, "secret-token"}},
           "enabled-tools" => :none
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

    test "enables only the mcp tools the builder picked", %{sham: sham, url: url} do
      tools = [mcp_tool({:some, ["get_customer", "list_orders"]})]

      Sham.expect(sham, fn conn ->
        {:ok, body, conn} = Plug.Conn.read_body(conn, length: 1_000_000)

        assert [
                 %{
                   "type" => "mcp_toolset",
                   "mcp_server_name" => "mcp-example-com",
                   "default_config" => %{"enabled" => false},
                   "configs" => %{
                     "get_customer" => %{"enabled" => true},
                     "list_orders" => %{"enabled" => true}
                   }
                 }
               ] == Jason.decode!(body)["tools"]

        Plug.Conn.send_resp(conn, 200, anthropic_response("ok"))
      end)

      assert {:ok, "ok"} ==
               run_component(build_args(url, %{provider: %{"tools" => {:some, tools}}}))
    end

    test "disables every mcp tool when the builder picked none", %{sham: sham, url: url} do
      tools = [mcp_tool({:some, []})]

      Sham.expect(sham, fn conn ->
        {:ok, body, conn} = Plug.Conn.read_body(conn, length: 1_000_000)

        assert [%{"default_config" => %{"enabled" => false}} = toolset] =
                 Jason.decode!(body)["tools"]

        refute Map.has_key?(toolset, "configs")

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

    test "reports the anthropic error with the api key redacted", %{sham: sham, url: url} do
      body =
        Jason.encode!(%{
          type: "error",
          error: %{type: "authentication_error", message: "invalid x-api-key sk-ant-abcd"}
        })

      Sham.expect(sham, fn conn -> Plug.Conn.send_resp(conn, 401, body) end)

      assert {:error,
              "Anthropic returned status 401 (authentication_error): invalid x-api-key [redacted]"} ==
               run_component(build_args(url))
    end

    test "does not echo a body that is not an anthropic error", %{sham: sham, url: url} do
      Sham.expect(sham, fn conn -> Plug.Conn.send_resp(conn, 429, "slow down") end)

      assert {:error, "Anthropic returned status 429"} == run_component(build_args(url))
    end

    test "rejects an unsupported provider", %{url: url} do
      args = build_args(url, %{provider: %{"name" => "mistral"}})

      assert {:error, "Unsupported provider: mistral"} == run_component(args)
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

  describe "prompt-call component with openai" do
    setup do
      sham = Sham.start()

      {:ok, %{sham: sham, url: "http://localhost:#{sham.port}/v1"}}
    end

    test "posts to the responses endpoint and returns the output text", %{sham: sham, url: url} do
      Sham.expect(sham, fn conn ->
        assert conn.path_info == ["v1", "responses"]
        assert {"authorization", "Bearer test-key"} in conn.req_headers
        assert {"content-type", "application/json"} in conn.req_headers

        {:ok, body, conn} = Plug.Conn.read_body(conn, length: 1_000_000)

        assert %{
                 "model" => "gpt-5.4",
                 "max_output_tokens" => 4096,
                 "instructions" => "You are helpful.",
                 "input" => "What is 2 + 2?",
                 "store" => false
               } == Jason.decode!(body)

        Plug.Conn.send_resp(conn, 200, openai_response("4"))
      end)

      assert {:ok, "4"} == run_component(openai_args(url))
    end

    test "sends web search and mcp tools", %{sham: sham, url: url} do
      tools = [
        {:"http-search", %{"description" => :none}},
        {:mcp,
         %{
           "description" => :none,
           "url" => "https://gateway.mcpservers.org/yahoo-finance/mcp",
           "authentication" => %{"kind" => "access_token", "value" => {:some, "secret-token"}},
           "enabled-tools" => :none
         }}
      ]

      Sham.expect(sham, fn conn ->
        {:ok, body, conn} = Plug.Conn.read_body(conn, length: 1_000_000)

        assert [
                 %{"type" => "web_search"},
                 %{
                   "type" => "mcp",
                   "server_label" => "gateway-mcpservers-org",
                   "server_url" => "https://gateway.mcpservers.org/yahoo-finance/mcp",
                   "require_approval" => "never",
                   "authorization" => "secret-token"
                 }
               ] == Jason.decode!(body)["tools"]

        Plug.Conn.send_resp(conn, 200, openai_response("ok"))
      end)

      assert {:ok, "ok"} ==
               run_component(openai_args(url, %{provider: %{"tools" => {:some, tools}}}))
    end

    test "allows only the mcp tools the builder picked", %{sham: sham, url: url} do
      tools = [mcp_tool({:some, ["get_customer", "list_orders"]})]

      Sham.expect(sham, fn conn ->
        {:ok, body, conn} = Plug.Conn.read_body(conn, length: 1_000_000)

        assert [
                 %{
                   "type" => "mcp",
                   "server_label" => "mcp-example-com",
                   "server_url" => "https://mcp.example.com/mcp",
                   "require_approval" => "never",
                   "allowed_tools" => ["get_customer", "list_orders"]
                 }
               ] == Jason.decode!(body)["tools"]

        Plug.Conn.send_resp(conn, 200, openai_response("ok"))
      end)

      assert {:ok, "ok"} ==
               run_component(openai_args(url, %{provider: %{"tools" => {:some, tools}}}))
    end

    test "reports the openai error with the api key redacted", %{sham: sham, url: url} do
      body =
        Jason.encode!(%{
          error: %{
            message: "Incorrect API key provided: sk-proj-...abcd.",
            type: "invalid_request_error",
            code: "invalid_api_key"
          }
        })

      Sham.expect(sham, fn conn -> Plug.Conn.send_resp(conn, 401, body) end)

      assert {:error,
              "OpenAI returned status 401 (invalid_api_key): Incorrect API key provided: [redacted]"} ==
               run_component(openai_args(url))
    end

    test "does not echo a body that is not an openai error", %{sham: sham, url: url} do
      Sham.expect(sham, fn conn -> Plug.Conn.send_resp(conn, 502, "bad gateway") end)

      assert {:error, "OpenAI returned status 502"} == run_component(openai_args(url))
    end

    test "reports a response that ran out of tokens", %{sham: sham, url: url} do
      body =
        Jason.encode!(%{
          status: "incomplete",
          incomplete_details: %{reason: "max_output_tokens"},
          output: [%{type: "reasoning", summary: []}]
        })

      Sham.expect(sham, fn conn -> Plug.Conn.send_resp(conn, 200, body) end)

      assert {:error,
              "OpenAI response was incomplete (max_output_tokens) before producing any text"} ==
               run_component(openai_args(url))
    end
  end

  describe "against the real openai api" do
    @tag :live
    test "completes a prompt" do
      key =
        System.get_env("OPENAI_API_KEY") ||
          flunk("set OPENAI_API_KEY to run the live test")

      args =
        openai_args("https://api.openai.com/v1", %{
          provider: %{"api-key" => key},
          instructions: "Reply with a single word and no punctuation.",
          message: "Reply with the word pong"
        })

      assert {:ok, text} = run_component(args)
      assert text =~ "pong"
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

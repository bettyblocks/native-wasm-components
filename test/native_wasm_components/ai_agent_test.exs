defmodule NativeWasmComponents.AiAgentTest do
  use ExUnit.Case, async: true

  @component_path "functions/ai-agent/1.0/ai_agent.wasm"
  @interface {"betty-blocks:ai-agent/ai-agent@2.0.0", "ai-agent"}
  @api_key_name "ai_provider:anthropic"

  defp run_component(args, config \\ %{@api_key_name => "test-key"}) do
    TestHelper.run_component(@component_path, @interface, args, config_imports(config))
  end

  defp config_imports(config) do
    %{
      "wasi:config/store@0.2.0-rc.1" => %{
        "get" => {:fn, fn key -> {:ok, to_option(Map.get(config, key))} end},
        "get-all" => {:fn, fn -> {:ok, Map.to_list(config)} end}
      }
    }
  end

  defp to_option(nil), do: :none
  defp to_option(value), do: {:some, value}

  defp build_args(url, overrides \\ %{}) do
    provider =
      Map.merge(
        %{
          "name" => "anthropic",
          "ai-model-name" => "claude-sonnet-5",
          "url" => url,
          "tools" => :none
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

  describe "ai-agent component" do
    setup do
      sham = Sham.start()

      {:ok, %{sham: sham, url: "http://localhost:#{sham.port}/v1/messages"}}
    end

    test "returns the assistant text", %{sham: sham, url: url} do
      Sham.expect(sham, fn conn ->
        Plug.Conn.send_resp(conn, 200, anthropic_response("4"))
      end)

      assert {:ok, "4"} == run_component(build_args(url))
    end

    test "sends the messages request anthropic expects", %{sham: sham, url: url} do
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

      assert {:ok, "2 + 2 = 4"} == run_component(build_args(url))
    end

    test "reports a non-success status", %{sham: sham, url: url} do
      Sham.expect(sham, fn conn -> Plug.Conn.send_resp(conn, 429, "slow down") end)

      assert {:error, "Anthropic returned status 429"} == run_component(build_args(url))
    end

    test "rejects an unsupported provider", %{url: url} do
      args = build_args(url, %{provider: %{"name" => "openai", "ai-model-name" => "gpt-5.4"}})

      assert {:error, "Unsupported provider: openai"} == run_component(args)
    end

    test "reports a missing api key naming the provider-scoped config key", %{url: url} do
      args = build_args(url)

      assert {:error, "No '#{@api_key_name}' found in runtime configuration"} ==
               run_component(args, %{})
    end

    test "rejects an empty url" do
      args = build_args("")

      assert {:error, "No URL configured for provider: anthropic"} == run_component(args)
    end
  end

  describe "against the real anthropic api" do
    @tag :live
    test "completes a prompt" do
      key =
        System.get_env("ANTHROPIC_API_KEY") ||
          flunk("set ANTHROPIC_API_KEY to run the live test")

      args =
        build_args("https://api.anthropic.com/v1/messages", %{
          instructions: "Reply with a single word and no punctuation.",
          message: "Reply with the word pong"
        })

      assert {:ok, text} = run_component(args, %{@api_key_name => key})
      assert text =~ "pong"
    end
  end
end

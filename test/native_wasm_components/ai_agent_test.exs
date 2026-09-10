defmodule NativeWasmComponents.AiAgentTest do
  use ExUnit.Case, async: true

  @component_path "functions/ai-agent/1.0/ai_agent.wasm"
  @interface {"betty-blocks:ai-agent/ai-agent@2.0.0", "ai-agent"}

  defp run_component(input, env \\ %{}) do
    TestHelper.run_component(@component_path, @interface, input, %{}, env)
  end

  defp build_input(overrides \\ %{}) do
    Map.merge(
      %{
        "provider" => %{
          "provider" => "anthropic",
          "model" => "claude-sonnet-5",
          "api-key" => "test-key"
        },
        "instructions" => "You are helpful.",
        "message" => "What is 2 + 2?",
        "max-tokens" => :none
      },
      overrides
    )
  end

  defp anthropic_response(text) do
    Jason.encode!(%{content: [%{type: "text", text: text}]})
  end

  describe "ai-agent component" do
    setup do
      sham = Sham.start()

      {:ok,
       %{
         sham: sham,
         env: %{"ANTHROPIC_API_URL" => "http://localhost:#{sham.port}/v1/messages"}
       }}
    end

    test "returns the assistant text", %{sham: sham, env: env} do
      Sham.expect(sham, fn conn ->
        Plug.Conn.send_resp(conn, 200, anthropic_response("4"))
      end)

      assert {:ok, %{as: "4"}} == run_component(build_input(), env)
    end

    test "sends the messages request anthropic expects", %{sham: sham, env: env} do
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

      assert {:ok, %{as: "4"}} == run_component(build_input(), env)
    end

    test "honours an explicit max-tokens", %{sham: sham, env: env} do
      Sham.expect(sham, fn conn ->
        {:ok, body, conn} = Plug.Conn.read_body(conn, length: 1_000_000)

        assert %{"max_tokens" => 256} = Jason.decode!(body)

        Plug.Conn.send_resp(conn, 200, anthropic_response("ok"))
      end)

      payload = build_input(%{"max-tokens" => {:some, 256}})

      assert {:ok, %{as: "ok"}} == run_component(payload, env)
    end

    test "joins multiple text blocks", %{sham: sham, env: env} do
      response =
        Jason.encode!(%{content: [%{type: "text", text: "2 + 2 = "}, %{type: "text", text: "4"}]})

      Sham.expect(sham, fn conn -> Plug.Conn.send_resp(conn, 200, response) end)

      assert {:ok, %{as: "2 + 2 = 4"}} == run_component(build_input(), env)
    end

    test "reports a non-success status", %{sham: sham, env: env} do
      Sham.expect(sham, fn conn -> Plug.Conn.send_resp(conn, 429, "slow down") end)

      assert {:error, "Anthropic returned status 429"} == run_component(build_input(), env)
    end

    test "rejects an unsupported provider", %{env: env} do
      payload =
        build_input(%{
          "provider" => %{
            "provider" => "openai",
            "model" => "gpt-5.4",
            "api-key" => "test-key"
          }
        })

      assert {:error, "Unsupported provider: openai"} == run_component(payload, env)
    end

    test "rejects an empty api key", %{env: env} do
      payload =
        build_input(%{
          "provider" => %{
            "provider" => "anthropic",
            "model" => "claude-sonnet-5",
            "api-key" => ""
          }
        })

      assert {:error, "No API key configured for provider: anthropic"} ==
               run_component(payload, env)
    end
  end

  describe "against the real anthropic api" do
    @tag :live
    test "completes a prompt" do
      key =
        System.get_env("ANTHROPIC_API_KEY") ||
          flunk("set ANTHROPIC_API_KEY to run the live test")

      payload =
        build_input(%{
          "provider" => %{
            "provider" => "anthropic",
            "model" => "claude-sonnet-5",
            "api-key" => key
          },
          "instructions" => "Reply with a single word and no punctuation.",
          "message" => "Reply with the word pong"
        })

      assert {:ok, %{as: text}} = run_component(payload)
      assert text =~ "pong"
    end
  end
end

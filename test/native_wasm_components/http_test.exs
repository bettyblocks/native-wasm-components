defmodule NativeWasmComponents.HttpTest do
  use ExUnit.Case, async: true

  alias NativeWasmComponents.ServerMock

  @component_path "target/wasm32-wasip2/release/http.wasm"

  defp run_component(
         method,
         url,
         url_parameters,
         body,
         body_parameters,
         query_parameters,
         headers \\ %{},
         protocol \\ :HTTP
       ) do
    case TestHelper.run_component(
           @component_path,
           {"betty-blocks:http/http@0.1.0", "http"},
           %{
             "method" => method,
             "protocol" => protocol,
             "headers" => Jason.encode!(headers),
             "url" => url,
             "url-parameters" => Jason.encode!(url_parameters),
             "body" => {:some, body},
             "body-parameters" => Jason.encode!(body_parameters),
             "query-parameters" => Jason.encode!(query_parameters)
           }
         ) do
      {:ok, result} ->
        # `as` is always json
        {:ok, Map.update!(result, :as, &Jason.decode!/1)}

      e ->
        e
    end
  end

  describe "http component" do
    setup do
      {:ok, pid} =
        ServerMock.open(callback: fn conn -> Plug.Conn.send_resp(conn, 200, "works") end)

      on_exit(fn ->
        ServerMock.shutdown(pid)
      end)

      port = ServerMock.get_port(pid)

      {:ok, %{host: "http://localhost:#{port}", port: port, pid: pid}}
    end

    test "simple", %{host: host, pid: pid} do
      assert {:ok, %{"response-code" => 200, as: "works"}} ==
               run_component(:get, host, %{}, "", %{}, %{})

      assert 1 == ServerMock.get_call_count(pid)
    end

    test "request returns json object", %{host: host, pid: pid} do
      ServerMock.set_callback(pid, fn conn -> Plug.Conn.send_resp(conn, 200, ~s|{"test": 1}|) end)

      assert {:ok, %{"response-code" => 200, as: %{"test" => 1}}} ==
               run_component(:get, host, %{}, "", %{}, %{})

      assert 1 == ServerMock.get_call_count(pid)
    end

    test "request returns json number", %{host: host, pid: pid} do
      ServerMock.set_callback(pid, fn conn -> Plug.Conn.send_resp(conn, 200, ~s|1|) end)

      assert {:ok, %{"response-code" => 200, as: 1}} ==
               run_component(:get, host, %{}, "", %{}, %{})

      assert 1 == ServerMock.get_call_count(pid)
    end

    test "request returns json string", %{host: host, pid: pid} do
      ServerMock.set_callback(pid, fn conn -> Plug.Conn.send_resp(conn, 200, ~s|"works"|) end)

      assert {:ok, %{"response-code" => 200, as: "works"}} ==
               run_component(:get, host, %{}, "", %{}, %{})

      assert 1 == ServerMock.get_call_count(pid)
    end

    test "status code 404", %{host: host, pid: pid} do
      ServerMock.set_callback(pid, fn conn -> Plug.Conn.send_resp(conn, 404, "not found") end)

      assert {:ok, %{"response-code" => 404, as: "not found"}} ==
               run_component(:get, host, %{}, "", %{}, %{})

      assert 1 == ServerMock.get_call_count(pid)
    end

    test "format liquid body", %{host: host, pid: pid} do
      ServerMock.set_callback(pid, fn conn ->
        {:ok, body, conn} = Plug.Conn.read_body(conn, length: 1_000_000)
        assert "testing" == body

        Plug.Conn.send_resp(conn, 200, "works")
      end)

      assert {:ok, %{"response-code" => 200, as: "works"}} ==
               run_component(:post, host, %{}, "{{ text }}", %{"text" => "testing"}, %{})

      assert 1 == ServerMock.get_call_count(pid)
    end

    test "format liquid url", %{host: host, pid: pid} do
      ServerMock.set_callback(pid, fn conn ->
        assert conn.path_info == ["hello", "bye"]
        Plug.Conn.send_resp(conn, 200, "works")
      end)

      assert {:ok, %{"response-code" => 200, as: "works"}} ==
               run_component(
                 :post,
                 "#{host}/{{path1}}/{{path2}}",
                 %{path1: "hello", path2: "bye"},
                 "",
                 %{},
                 %{}
               )

      assert 1 == ServerMock.get_call_count(pid)
    end

    test "use query parameters", %{host: host, pid: pid} do
      ServerMock.set_callback(pid, fn conn ->
        conn = Plug.Conn.fetch_query_params(conn)

        assert %{"search" => "true"} == conn.query_params
        assert "search=true" == conn.query_string

        Plug.Conn.send_resp(conn, 200, "works")
      end)

      assert {:ok, %{"response-code" => 200, as: "works"}} ==
               run_component(
                 :get,
                 host,
                 %{},
                 "",
                 %{},
                 %{"search" => true}
               )

      assert 1 == ServerMock.get_call_count(pid)
    end

    test "use headers", %{host: host, pid: pid} do
      ServerMock.set_callback(pid, fn conn ->
        assert {"content-type", "application/json"} in conn.req_headers

        Plug.Conn.send_resp(conn, 200, "works")
      end)

      assert {:ok, %{"response-code" => 200, as: "works"}} ==
               run_component(
                 :post,
                 host,
                 %{},
                 ~s|{"test": true}|,
                 %{},
                 %{},
                 %{"content-type" => "application/json"}
               )

      assert 1 == ServerMock.get_call_count(pid)
    end

    test "works without scheme set in url, http", %{port: port, pid: pid} do
      assert {:ok, %{"response-code" => 200, as: "works"}} ==
               run_component(
                 :get,
                 "localhost:#{port}",
                 %{},
                 ~s|{"test": true}|,
                 %{},
                 %{},
                 %{"content-type" => "application/json"},
                 :HTTP
               )

      assert 1 == ServerMock.get_call_count(pid)
    end

    @tag :capture_log
    test "works without scheme set in url, https", %{port: port, pid: pid} do
      assert {:error, "ErrorCode::TlsProtocolError"} ==
               run_component(
                 :get,
                 "localhost:#{port}",
                 %{},
                 ~s|{"test": true}|,
                 %{},
                 %{},
                 %{"content-type" => "application/json"},
                 :HTTPS
               )

      # bandit already catches this, before callback is executed
      assert 0 == ServerMock.get_call_count(pid)
    end
  end
end

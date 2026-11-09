defmodule NativeWasmComponents.CallApiTest do
  use ExUnit.Case, async: true

  @component_path "functions/call-api/1.0/call_api.wasm"
  @interface {"betty-blocks:call-api/call-api@1.1.0", "call-api"}

  defp run_component(args) do
    TestHelper.run_component(@component_path, @interface, args)
  end

  defp build_api(base_url, overrides \\ %{}) do
    Map.merge(
      %{
        "software-name" => "Exact",
        "api-name" => "Get users",
        "base-url" => base_url,
        "path" => "/users",
        "method" => :get
      },
      overrides
    )
  end

  describe "calling the api" do
    setup do
      sham = Sham.start()

      %{sham: sham, base_url: "http://localhost:#{sham.port}/"}
    end

    test "sends the method of the api to the joined url", %{sham: sham, base_url: base_url} do
      Sham.expect(sham, fn conn ->
        assert conn.method == "GET"
        assert conn.request_path == "/users"

        Plug.Conn.resp(conn, 200, "{}")
      end)

      assert {:ok, "{}"} = run_component([build_api(base_url)])
    end

    test "sends a post when the api is a post", %{sham: sham, base_url: base_url} do
      Sham.expect(sham, fn conn ->
        assert conn.method == "POST"

        Plug.Conn.resp(conn, 201, "{}")
      end)

      assert {:ok, "{}"} = run_component([build_api(base_url, %{"method" => :post})])
    end

    test "returns the json body of the response", %{sham: sham, base_url: base_url} do
      Sham.expect(sham, fn conn ->
        Plug.Conn.resp(conn, 200, ~s|{"users": [{"id": 1}]}|)
      end)

      assert {:ok, json} = run_component([build_api(base_url)])
      assert Jason.decode!(json) == %{"users" => [%{"id" => 1}]}
    end

    test "returns a body that is not json as a json string", %{sham: sham, base_url: base_url} do
      Sham.expect(sham, fn conn -> Plug.Conn.resp(conn, 200, "OK") end)

      assert {:ok, ~s|"OK"|} = run_component([build_api(base_url)])
    end

    test "returns only the status when the api responds with an error",
         %{sham: sham, base_url: base_url} do
      Sham.expect(sham, fn conn ->
        Plug.Conn.resp(conn, 401, ~s|{"error": "invalid key abc123"}|)
      end)

      assert {:error, "The API responded with status 401"} = run_component([build_api(base_url)])
    end
  end

  test "returns an error when the api can not be reached" do
    assert {:error, "The API request failed: " <> _reason} =
             run_component([build_api("http://localhost:1")])
  end

  test "returns an error when the base url is empty" do
    assert {:error, "The API has no base url"} = run_component([build_api("")])
  end

  test "returns an error when the path is empty" do
    assert {:error, "The API has no path"} =
             run_component([build_api("https://api.example.com", %{"path" => ""})])
  end
end

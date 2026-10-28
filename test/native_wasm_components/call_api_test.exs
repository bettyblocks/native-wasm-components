defmodule NativeWasmComponents.CallApiTest do
  use ExUnit.Case, async: true

  @component_path "functions/call-api/1.0/call_api.wasm"
  @interface {"betty-blocks:call-api/call-api@1.0.0", "call-api"}

  defp run_component(args) do
    TestHelper.run_component(@component_path, @interface, args)
  end

  defp build_api(overrides \\ %{}) do
    Map.merge(
      %{
        "software-name" => "Exact",
        "api-name" => "Get a user",
        "base-url" => "https://api.example.com/",
        "path" => "/users/{id}",
        "method" => :get,
        "authentication" => :none
      },
      overrides
    )
  end

  test "returns the selected api as json" do
    assert {:ok, json} = run_component([build_api()])

    assert Jason.decode!(json) == %{
             "softwareName" => "Exact",
             "apiName" => "Get a user",
             "method" => "get",
             "url" => "https://api.example.com/users/{id}",
             "authentication" => "none"
           }
  end

  test "reports api-key authentication without the secret" do
    authentication =
      {:"api-key",
       %{
         "location" => :header,
         "name" => "Authorization",
         "prefix" => {:some, "Bearer "},
         "value" => "super-secret-key"
       }}

    assert {:ok, json} =
             run_component([build_api(%{"method" => :post, "authentication" => authentication})])

    assert %{"method" => "post", "authentication" => "api-key"} = Jason.decode!(json)
    refute json =~ "super-secret-key"
  end

  test "reports oauth2 authentication without the secret" do
    authentication =
      {:oauth2,
       %{
         "token-url" => "https://auth.example.com/token",
         "client-id" => "client-id",
         "client-secret" => "super-secret-client",
         "scopes" => {:some, ["read"]}
       }}

    assert {:ok, json} = run_component([build_api(%{"authentication" => authentication})])

    assert %{"authentication" => "oauth2"} = Jason.decode!(json)
    refute json =~ "super-secret-client"
  end

  test "returns an error when the base url is empty" do
    assert {:error, "The API has no base url"} = run_component([build_api(%{"base-url" => ""})])
  end

  test "returns an error when the path is empty" do
    assert {:error, "The API has no path"} = run_component([build_api(%{"path" => ""})])
  end
end

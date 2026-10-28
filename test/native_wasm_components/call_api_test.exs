defmodule NativeWasmComponents.CallApiTest do
  use ExUnit.Case, async: true

  @component_path "functions/call-api/1.0/call_api.wasm"
  @interface {"betty-blocks:call-api/call-api@1.0.0", "call-api"}

  defp run_component(args) do
    TestHelper.run_component(@component_path, @interface, args)
  end

  test "returns the selected api as json" do
    assert {:ok, json} = run_component([%{"id" => "1", "software-name" => "Exact"}])
    assert Jason.decode!(json) == %{"id" => "1", "softwareName" => "Exact"}
  end

  test "returns an error when no api is selected" do
    assert {:error, "No API selected"} =
             run_component([%{"id" => "1", "software-name" => ""}])
  end
end

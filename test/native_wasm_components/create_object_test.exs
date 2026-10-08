defmodule NativeWasmComponents.CreateObjectTest do
  use ExUnit.Case, async: true

  @component_path "functions/create-object/1.0/create_object.wasm"

  defp run_create_object(key_value_map, schema_model) do
    @component_path
    |> TestHelper.run_component(
      {"betty-blocks:create-object/create-object@4.0.0", "create-object"},
      [Jason.encode!(key_value_map), schema_model]
    )
    |> Jason.decode!()
  end

  describe "create-object component" do
    test "returns the key value map as the object" do
      result = run_create_object(%{first_name: "John", age: 42}, :none)
      assert result == %{"first_name" => "John", "age" => 42}
    end

    test "ignores the schema model" do
      result = run_create_object(%{first_name: "John"}, {:some, "Person"})
      assert result == %{"first_name" => "John"}
    end
  end
end

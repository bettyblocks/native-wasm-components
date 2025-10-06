defmodule NativeWasmComponents.ExpressionTest do
  use ExUnit.Case, async: true

  @component_path "functions/expression/1.0/expression.wasm"

  defp run_expression(expression, variables) do
    case TestHelper.run_component(
           @component_path,
           {"betty-blocks:expression/expression@0.1.0", "expression"},
           %{
             "expression" => expression,
             "variables" => Jason.encode!(variables),
             "schema-model" => :none,
             "debug-logging" => :none
           }
         ) do
      {:ok, %{result: result}} -> {:ok, Jason.decode!(result)}
      e -> e
    end
  end

  describe "expression component" do
    test "simple expression" do
      {:ok, result} = run_expression(~s|1 + 2|, %{})
      assert result == 3
    end

    test "expression with template" do
      {:ok, result} =
        run_expression(~s|"{{ first_name }}" + " " + "{{ last_name }}"|, %{
          first_name: "John",
          last_name: "Doe"
        })

      assert result == "John Doe"
    end

    test "expression with templated magic" do
      {:ok, result} = run_expression(~s|{{ array.length }}|, %{array: [1, 2, 3, 4, 5, 6]})
      assert result == 6
    end
  end
end

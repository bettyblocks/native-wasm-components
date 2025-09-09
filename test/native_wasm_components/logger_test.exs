defmodule NativeWasmComponents.LoggerTest do
  use ExUnit.Case, async: false

  alias ExUnit.CaptureLog

  require Logger

  def logger_function(level, _context, message) do
    level = if level == :warn, do: :warning, else: level
    Logger.log(level, message)
  end

  defp run_component(component, imports, severity, variables) do
    TestHelper.run_component(
      component,
      {"betty-blocks:logging/logger@0.1.0", "log"},
      [
        %{
          severity: severity,
          variables: variables
        }
      ],
      imports
    )
  end

  describe "logging component" do
    setup do
      imports = %{
        "wasi:logging/logging@0.1.0-draft" => %{
          "log" => {:fn, &logger_function/3}
        }
      }

      {:ok, imports: imports, component: "target/wasm32-wasip2/release/logging.wasm"}
    end

    test "simple error", %{imports: imports, component: logger} do
      assert CaptureLog.capture_log(fn ->
               run_component(logger, imports, :error, ~s({"greeting":"Hello World!"}))
             end) =~ ~s|[error] greeting : "Hello World!"|
    end

    test "nested object", %{imports: imports, component: logger} do
      assert CaptureLog.capture_log(fn ->
               run_component(
                 logger,
                 imports,
                 :warn,
                 """
                 {
                   "data": [
                     {
                       "name": "John",
                       "address": {
                         "city": {
                           "coordinates": [
                             9123,
                             98113
                           ]
                         }
                       }
                     }
                   ]
                 }
                 """
               )
             end) =~
               ~s([warning] data : [{"name":"John","address":{"city":{"coordinates":[9123,98113]}}}])
    end

    test "multiple lines", %{imports: imports, component: logger} do
      {:ok, pid} = Agent.start_link(fn -> [] end)

      imports =
        put_in(
          imports,
          ["wasi:logging/logging@0.1.0-draft", "log"],
          {:fn,
           fn level, _, message ->
             Agent.update(pid, fn data -> [{level, message} | data] end)
           end}
        )

      assert :ok ==
               run_component(logger, imports, :info, """
               {
                 "item1": 1,
                 "item2": 2,
                 "item3": 3,
                 "item4": 4
               }
               """)

      assert [info: "item1 : 1", info: "item2 : 2", info: "item3 : 3", info: "item4 : 4"] ==
               pid |> Agent.get(fn data -> data end) |> Enum.reverse()
    end
  end
end

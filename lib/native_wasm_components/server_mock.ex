defmodule NativeWasmComponents.ServerMock do
  @moduledoc """
  Documentation for `NativeWasmComponents.ServerMock`.
  """

  def open(args \\ []) do
    args
    |> Keyword.take([:callback, :port])
    |> start_server()
  end

  def shutdown(pid) when is_pid(pid) do
    DynamicSupervisor.terminate_child(__MODULE__.DynamicSupervisor, pid)
  end

  defdelegate get_port(pid), to: __MODULE__.Instance
  defdelegate set_callback(pid, callback), to: __MODULE__.Instance
  defdelegate get_callback(pid), to: __MODULE__.Instance
  defdelegate get_call_count(pid), to: __MODULE__.Instance

  def start_server(args) do
    DynamicSupervisor.start_child(__MODULE__.DynamicSupervisor, {__MODULE__.Instance, args})
  end
end

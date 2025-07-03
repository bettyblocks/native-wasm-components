defmodule NativeWasmComponents.ServerMock.Plug do
  @moduledoc false
  @behaviour Plug

  alias NativeWasmComponents.ServerMock.Instance

  @impl true
  def init(opts), do: opts

  @impl true
  def call(conn, pid) do
    callback = Instance.get_callback(pid)

    callback.(conn)
  end
end

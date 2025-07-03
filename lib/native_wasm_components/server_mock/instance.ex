defmodule NativeWasmComponents.ServerMock.Instance do
  @moduledoc false
  use GenServer

  alias NativeWasmComponents.ServerMock

  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, [opts])
  end

  def init([opts]) do
    port = Keyword.get(opts, :port, 0)

    default_callback =
      Keyword.get(opts, :callback, fn conn ->
        Plug.Conn.send_resp(conn, 418, "Pass in the callback")
      end)

    {:ok, pid} =
      Bandit.start_link(port: port, plug: {ServerMock.Plug, self()}, startup_log: false)

    port =
      if port == 0 do
        {:ok, {_, port}} = ThousandIsland.listener_info(pid)
        port
      else
        port
      end

    {:ok, %{bandit: pid, port: port, callback: default_callback, call_count: 0}}
  end

  def get_port(pid) do
    GenServer.call(pid, :get_port)
  end

  def get_call_count(pid) do
    GenServer.call(pid, :get_call_count)
  end

  def set_callback(pid, callback) do
    GenServer.call(pid, {:set_callback, callback})
  end

  def get_callback(pid) do
    GenServer.call(pid, :get_callback)
  end

  def handle_call(:get_port, _from, state) do
    {:reply, state[:port], state}
  end

  def handle_call(:get_call_count, _from, state) do
    {:reply, state[:call_count], state}
  end

  def handle_call({:set_callback, callback}, _from, state) do
    {:reply, :ok, state |> Map.put(:callback, callback) |> Map.put(:call_count, 0)}
  end

  def handle_call(:get_callback, _from, state) do
    {:reply, Map.fetch!(state, :callback), Map.update!(state, :call_count, &(&1 + 1))}
  end
end

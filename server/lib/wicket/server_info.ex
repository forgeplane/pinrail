defmodule Wicket.ServerInfo do
  @moduledoc """
  Advertises the running server to the CLI: writes `<data dir>/server.json`
  with the URL, port and OS pid when the endpoint is serving, and removes it
  on shutdown. One instance per machine is settled by this file.
  """
  use GenServer

  @file_name "server.json"

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  @spec path() :: Path.t()
  def path, do: Path.join(Wicket.data_dir(), @file_name)

  @doc "The advertised info, or nil when not serving."
  @spec info() :: map() | nil
  def info do
    case File.read(path()) do
      {:ok, body} -> JSON.decode!(body)
      {:error, _} -> nil
    end
  end

  @impl true
  def init(_opts) do
    if Phoenix.Endpoint.server?(:wicket, WicketWeb.Endpoint) do
      Process.flag(:trap_exit, true)
      write()
      {:ok, %{written: true}}
    else
      {:ok, %{written: false}}
    end
  end

  @impl true
  def terminate(_reason, %{written: true}), do: File.rm(path())
  def terminate(_reason, _state), do: :ok

  defp write do
    port = WicketWeb.Endpoint.config(:http)[:port]

    info = %{
      "url" => "http://127.0.0.1:#{port}",
      "port" => port,
      "pid" => System.pid() |> String.to_integer(),
      "started_at" => Wicket.Gate.iso(DateTime.utc_now())
    }

    File.write!(path(), JSON.encode!(info) <> "\n")
  end
end

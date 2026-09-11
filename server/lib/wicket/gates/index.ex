defmodule Wicket.Gates.Index do
  @moduledoc """
  In-memory index of every gate's envelope (payload excluded), held in an ETS
  table. Built by scanning the data directory at boot and updated on every
  write, it is what listing, filtering and sorting run against. It is never a
  source of truth: `reload/0` rebuilds it from the files at any time.
  """
  use GenServer

  require Logger

  alias Wicket.{Gate, Store}

  @table __MODULE__

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  @doc "Inserts or replaces a gate's index entry."
  @spec put(Gate.t()) :: :ok
  def put(%Gate{} = gate), do: GenServer.call(__MODULE__, {:put, gate})

  @doc "The indexed gate (payload `nil`), or `nil`."
  @spec get(String.t()) :: Gate.t() | nil
  def get(id) do
    case :ets.lookup(@table, id) do
      [{^id, gate}] -> gate
      [] -> nil
    end
  end

  @doc "Every indexed gate, unordered."
  @spec all() :: [Gate.t()]
  def all, do: for({_id, gate} <- :ets.tab2list(@table), do: gate)

  @doc "Rescans the data directory. Returns the number of gates indexed."
  @spec reload() :: non_neg_integer()
  def reload, do: GenServer.call(__MODULE__, :reload)

  @impl true
  def init(_opts) do
    :ets.new(@table, [:named_table, :set, :protected, read_concurrency: true])
    {:ok, %{}, {:continue, :scan}}
  end

  @impl true
  def handle_continue(:scan, state) do
    scan()
    {:noreply, state}
  end

  @impl true
  def handle_call({:put, gate}, _from, state) do
    :ets.insert(@table, {gate.id, %{gate | payload: nil}})
    {:reply, :ok, state}
  end

  def handle_call(:reload, _from, state) do
    :ets.delete_all_objects(@table)
    {:reply, scan(), state}
  end

  defp scan do
    root = Wicket.data_dir()

    root
    |> Store.list_ids()
    |> Enum.reduce(0, fn id, count ->
      case Store.load(root, id) do
        {:ok, gate} ->
          :ets.insert(@table, {gate.id, %{gate | payload: nil}})
          count + 1

        {:error, reason} ->
          Logger.warning("skipping gate #{id}: #{inspect(reason)}")
          count
      end
    end)
  end
end

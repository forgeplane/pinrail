defmodule Wicket.Types.Registry do
  @moduledoc """
  Holds the loaded plugins. Scans the plugin directories at boot and on
  `reload/0`; a name found in two directories is an error that names both
  paths, and at boot it is fatal.

  Plugins loaded from version snapshots (for gates created under an older
  version) are cached here too, keyed by `{name, version}`.
  """
  use GenServer

  require Logger

  alias Wicket.Types.Plugin

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  @spec all() :: [Plugin.t()]
  def all, do: GenServer.call(__MODULE__, :all)

  @spec get(String.t()) :: Plugin.t() | nil
  def get(name), do: GenServer.call(__MODULE__, {:get, name})

  @doc "A plugin at a specific version: from the registry if current, else from its snapshot."
  @spec get(String.t(), pos_integer()) :: Plugin.t() | nil
  def get(name, version), do: GenServer.call(__MODULE__, {:get, name, version})

  @doc "The directories scanned, in order."
  @spec dirs() :: [Path.t()]
  def dirs, do: GenServer.call(__MODULE__, :dirs)

  @doc "Rescans. On a duplicate name the previous registry is kept and the error returned."
  @spec reload() :: {:ok, non_neg_integer()} | {:error, String.t()}
  def reload, do: GenServer.call(__MODULE__, :reload)

  @impl true
  def init(_opts) do
    case scan() do
      {:ok, plugins} -> {:ok, %{plugins: plugins, snapshots: %{}}}
      {:error, message} -> {:stop, message}
    end
  end

  @impl true
  def handle_call(:all, _from, state) do
    {:reply, state.plugins |> Map.values() |> Enum.sort_by(& &1.name), state}
  end

  def handle_call({:get, name}, _from, state), do: {:reply, state.plugins[name], state}

  def handle_call({:get, name, version}, _from, state) do
    case state.plugins[name] do
      %Plugin{version: ^version} = plugin ->
        {:reply, plugin, state}

      _ ->
        case state.snapshots[{name, version}] do
          %Plugin{} = plugin ->
            {:reply, plugin, state}

          nil ->
            case load_snapshot(name, version) do
              nil -> {:reply, nil, state}
              plugin -> {:reply, plugin, put_in(state.snapshots[{name, version}], plugin)}
            end
        end
    end
  end

  def handle_call(:dirs, _from, state), do: {:reply, Wicket.Types.dirs(), state}

  def handle_call(:reload, _from, state) do
    case scan() do
      {:ok, plugins} ->
        {:reply, {:ok, map_size(plugins)}, %{state | plugins: plugins, snapshots: %{}}}

      {:error, message} ->
        {:reply, {:error, message}, state}
    end
  end

  defp scan do
    loaded =
      for dir <- Wicket.Types.dirs(), sub <- subdirs(dir) do
        Plugin.load(sub)
      end

    duplicates =
      loaded
      |> Enum.group_by(& &1.name)
      |> Enum.filter(fn {_name, ps} -> length(ps) > 1 end)

    case duplicates do
      [] ->
        for %Plugin{error: error} = p when not is_nil(error) <- loaded,
            do: Logger.warning("plugin #{p.name} at #{p.path} is not usable: #{error}")

        {:ok, Map.new(loaded, &{&1.name, &1})}

      dups ->
        message =
          Enum.map_join(dups, "; ", fn {name, ps} ->
            "gate type #{name} is defined at " <> Enum.map_join(ps, " and ", & &1.path)
          end)

        {:error, message}
    end
  end

  defp subdirs(dir) do
    case File.ls(dir) do
      {:ok, names} ->
        names
        |> Enum.sort()
        |> Enum.map(&Path.join(dir, &1))
        |> Enum.filter(&File.regular?(Path.join(&1, "manifest.json")))

      {:error, _} ->
        []
    end
  end

  defp load_snapshot(name, version) do
    dir = Wicket.Types.snapshot_dir(name, version)

    if File.regular?(Path.join(dir, "manifest.json")) do
      case Plugin.load(dir) do
        %Plugin{version: ^version, error: nil} = plugin -> plugin
        _ -> nil
      end
    end
  end
end

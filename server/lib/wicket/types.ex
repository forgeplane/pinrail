defmodule Wicket.Types do
  @moduledoc """
  Gate types as plugins: discovery, validation and version snapshots.

  Plugin directories come from `config :wicket, :plugin_dirs` (default: the
  built-in `priv/plugins` and `<config dir>/plugins`) plus any directory added
  with `add_dir/1`, which is persisted in `<config dir>/plugin_dirs.json`.

  History must render what was shown: the first gate created under a plugin
  version copies the plugin into `<data dir>/plugins/<name>/<version>/`, and
  gates keep validating and rendering from that copy after the live plugin
  moves on. A manifest with `"dev": true` is served live and never snapshotted.
  """

  alias Wicket.GateError
  alias Wicket.Types.{Plugin, Registry}

  @dirs_file "plugin_dirs.json"

  # -- lookup ---------------------------------------------------------------

  @spec all() :: [Plugin.t()]
  defdelegate all, to: Registry

  @doc "The current plugin for a type, or an `:invalid` error pointing at `/type`."
  @spec fetch(String.t()) :: {:ok, Plugin.t()} | {:error, GateError.t()}
  def fetch(name) when is_binary(name) do
    case Registry.get(name) do
      %Plugin{error: nil} = p -> {:ok, p}
      %Plugin{error: error} -> {:error, invalid_type("type #{name} is not usable: #{error}")}
      nil -> {:error, invalid_type("unknown gate type #{name}")}
    end
  end

  @doc "The plugin at the version a gate was created under."
  @spec fetch(String.t(), pos_integer()) :: {:ok, Plugin.t()} | {:error, GateError.t()}
  def fetch(name, version) when is_binary(name) and is_integer(version) do
    case Registry.get(name, version) do
      %Plugin{error: nil} = p -> {:ok, p}
      _ -> {:error, invalid_type("gate type #{name} version #{version} is not available")}
    end
  end

  # -- validation -----------------------------------------------------------

  @spec validate_payload(Plugin.t(), term()) :: :ok | {:error, GateError.t()}
  defdelegate validate_payload(plugin, payload), to: Plugin

  @spec validate_decision(Plugin.t(), term()) :: :ok | {:error, GateError.t()}
  defdelegate validate_decision(plugin, decision), to: Plugin

  # -- directories ----------------------------------------------------------

  @doc "The plugin directories in scan order: configured defaults, then added ones."
  @spec dirs() :: [Path.t()]
  def dirs do
    defaults =
      Application.get_env(:wicket, :plugin_dirs) ||
        [Application.app_dir(:wicket, "priv/plugins"), Path.join(config_dir(), "plugins")]

    Enum.uniq(defaults ++ added_dirs())
  end

  @doc "Registers a directory of plugins, persists it, and reloads."
  @spec add_dir(Path.t()) :: {:ok, non_neg_integer()} | {:error, String.t()}
  def add_dir(dir) do
    dir = Path.expand(dir)

    cond do
      not File.dir?(dir) ->
        {:error, "#{dir} is not a directory"}

      dir in dirs() ->
        Registry.reload()

      true ->
        before = added_dirs()
        write_added_dirs(before ++ [dir])

        case Registry.reload() do
          {:ok, n} ->
            {:ok, n}

          {:error, _} = error ->
            write_added_dirs(before)
            Registry.reload()
            error
        end
    end
  end

  @spec reload() :: {:ok, non_neg_integer()} | {:error, String.t()}
  defdelegate reload, to: Registry

  @doc "Where `add_dir/1` persists its list and where user plugins live by default."
  @spec config_dir() :: Path.t()
  def config_dir, do: Application.fetch_env!(:wicket, :config_dir)

  defp added_dirs do
    with {:ok, body} <- File.read(Path.join(config_dir(), @dirs_file)),
         {:ok, list} when is_list(list) <- JSON.decode(body) do
      Enum.filter(list, &is_binary/1)
    else
      _ -> []
    end
  end

  defp write_added_dirs(list) do
    File.mkdir_p!(config_dir())
    File.write!(Path.join(config_dir(), @dirs_file), JSON.encode!(list) <> "\n")
  end

  # -- snapshots ------------------------------------------------------------

  @spec snapshot_dir(String.t(), pos_integer()) :: Path.t()
  def snapshot_dir(name, version),
    do: Path.join([Wicket.data_dir(), "plugins", name, Integer.to_string(version)])

  @doc """
  Makes sure the plugin's version is snapshotted, unless it is a dev plugin.
  Called before a gate is created. Returns the directory the plugin's bundle
  should be served from.
  """
  @spec ensure_snapshot(Plugin.t()) :: {:ok, Path.t()} | {:error, term()}
  def ensure_snapshot(%Plugin{dev: true, path: path}), do: {:ok, path}

  def ensure_snapshot(%Plugin{} = p) do
    dest = snapshot_dir(p.name, p.version)

    if File.regular?(Path.join(dest, "manifest.json")) do
      {:ok, dest}
    else
      tmp = dest <> ".tmp"
      File.rm_rf!(tmp)

      with :ok <- File.mkdir_p(Path.dirname(dest)),
           {:ok, _} <- File.cp_r(p.path, tmp),
           :ok <- File.rename(tmp, dest) do
        {:ok, dest}
      end
    end
  end

  @doc "The directory a plugin's bundle is served from: live for dev plugins, else the snapshot."
  @spec bundle_dir(Plugin.t()) :: Path.t()
  def bundle_dir(%Plugin{dev: true, path: path}), do: path
  def bundle_dir(%Plugin{name: name, version: version}), do: snapshot_dir(name, version)

  defp invalid_type(message), do: GateError.invalid(GateError.violation("/type", message))
end

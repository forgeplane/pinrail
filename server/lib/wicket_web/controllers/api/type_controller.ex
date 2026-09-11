defmodule WicketWeb.API.TypeController do
  @moduledoc """
  `/api/types`: registered gate types, reload, and adding a plugin directory.
  """
  use WicketWeb, :controller

  alias Wicket.Types
  alias Wicket.Types.Plugin

  action_fallback WicketWeb.API.FallbackController

  def index(conn, _params) do
    json(conn, %{
      "dirs" => Types.dirs(),
      "types" => Enum.map(Types.all(), &type_map/1)
    })
  end

  def reload(conn, _params) do
    with {:ok, count} <- Types.reload() do
      json(conn, %{"ok" => true, "count" => count})
    end
  end

  def add_dir(conn, %{"dir" => dir}) when is_binary(dir) do
    with {:ok, count} <- Types.add_dir(dir) do
      json(conn, %{"ok" => true, "count" => count, "dirs" => Types.dirs()})
    end
  end

  def add_dir(_conn, _params), do: {:error, "dir is required"}

  defp type_map(%Plugin{} = p) do
    manifest = Plugin.manifest(p)

    %{
      "name" => p.name,
      "version" => p.version,
      "title" => p.title,
      "path" => p.path,
      "entry" => p.entry,
      "min_height" => p.min_height,
      "dev" => p.dev,
      "usable" => Plugin.usable?(p),
      "error" => p.error,
      "payload_schema" => manifest["payload_schema"],
      "decision_schema" => manifest["decision_schema"]
    }
  end
end

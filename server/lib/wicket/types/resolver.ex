defmodule Wicket.Types.Resolver do
  @moduledoc """
  JSV resolver for a plugin's own schema files.

  A plugin's schemas are built with `$id` set to
  `wicket-plugin://<name>/<version>/manifest.json`, so a relative `$ref` such as
  `payload.schema.json` or `common.json#/$defs/item` resolves to a URI under
  that prefix. This resolver maps such URIs back to files in the plugin
  directory and nowhere else: no other scheme is fetched, and a path that
  escapes the directory is refused.
  """
  @behaviour JSV.Resolver

  @scheme "wicket-plugin://"

  @doc "The `$id` prefix for a plugin, ending in a slash."
  @spec prefix(String.t(), pos_integer()) :: String.t()
  def prefix(name, version), do: "#{@scheme}#{name}/#{version}/"

  @impl true
  def resolve(uri, %{prefix: prefix, dir: dir}) do
    with true <- String.starts_with?(uri, prefix) || {:error, {:unknown_uri, uri}},
         relative = uri |> String.replace_prefix(prefix, "") |> strip_fragment(),
         {:ok, safe} <- Path.safe_relative(relative, dir) |> ok_or({:unsafe_path, relative}),
         {:ok, body} <- File.read(Path.join(dir, safe)),
         {:ok, schema} <- JSON.decode(body) do
      {:normal, schema}
    else
      {:error, reason} -> {:error, {:schema_file, uri, reason}}
    end
  end

  def resolve(uri, _), do: {:error, {:unknown_uri, uri}}

  defp strip_fragment(s), do: s |> String.split("#", parts: 2) |> hd()
  defp ok_or({:ok, v}, _), do: {:ok, v}
  defp ok_or(:error, reason), do: {:error, reason}
end

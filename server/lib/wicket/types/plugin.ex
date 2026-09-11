defmodule Wicket.Types.Plugin do
  @moduledoc """
  One registered gate type: the parsed manifest and the compiled schemas.

  A plugin that fails to load is still represented, with `error` set, so the
  types page can say what is wrong; it cannot be used to create gates.
  """

  alias Wicket.GateError
  alias Wicket.Types.Resolver

  @type t :: %__MODULE__{
          name: String.t(),
          version: pos_integer(),
          title: String.t(),
          path: Path.t(),
          entry: String.t(),
          min_height: pos_integer(),
          dev: boolean(),
          payload_schema: JSV.Root.t() | nil,
          decision_schema: JSV.Root.t() | nil,
          error: String.t() | nil
        }

  @enforce_keys [:name, :version, :path]
  defstruct [
    :name,
    :version,
    :path,
    :payload_schema,
    :decision_schema,
    :error,
    title: nil,
    entry: "index.html",
    min_height: 400,
    dev: false
  ]

  @manifest "manifest.json"

  @doc "Loads the plugin at `dir`. Never raises: a bad plugin comes back with `error`."
  @spec load(Path.t()) :: t()
  def load(dir) do
    with {:ok, manifest} <- read_manifest(dir),
         {:ok, plugin} <- from_manifest(manifest, dir),
         :ok <- ensure_entry(plugin),
         {:ok, plugin} <- build_schemas(plugin, manifest) do
      plugin
    else
      {:error, message} ->
        %__MODULE__{name: name_hint(dir), version: 0, path: dir, error: message}
    end
  end

  @doc "Validates a payload; violations are keyed into the payload."
  @spec validate_payload(t(), map()) :: :ok | {:error, GateError.t()}
  def validate_payload(%__MODULE__{} = p, payload), do: validate(p, p.payload_schema, payload)

  @doc """
  Validates a decision against the decision schema. That schema is the whole
  contract: what the items inside mean is the plugin's business, and the
  requester reads the decision in the shape the schema defines.
  """
  @spec validate_decision(t(), map()) :: :ok | {:error, GateError.t()}
  def validate_decision(%__MODULE__{} = p, decision), do: validate(p, p.decision_schema, decision)

  @doc "True if the plugin loaded cleanly."
  def usable?(%__MODULE__{error: nil}), do: true
  def usable?(%__MODULE__{}), do: false

  @doc "The manifest as read from disk (raw map), for the types page and API."
  @spec manifest(t()) :: map()
  def manifest(%__MODULE__{path: dir}) do
    case read_manifest(dir) do
      {:ok, m} -> m
      _ -> %{}
    end
  end

  # -- loading --------------------------------------------------------------

  defp read_manifest(dir) do
    path = Path.join(dir, @manifest)

    with {:ok, body} <- File.read(path) |> wrap("cannot read #{@manifest}"),
         {:ok, map} <- JSON.decode(body) |> wrap("#{@manifest} is not valid JSON"),
         true <- is_map(map) || {:error, "#{@manifest} must be a JSON object"} do
      {:ok, map}
    end
  end

  defp from_manifest(m, dir) do
    with {:ok, name} <- string_field(m, "name", ~r/^[a-z][a-z0-9_]*$/),
         {:ok, version} <- int_field(m, "version"),
         {:ok, entry} <-
           string_field(Map.put_new(m, "entry", "index.html"), "entry", ~r/^[^\/][^\0]*$/),
         :ok <- present(m, "payload_schema"),
         :ok <- present(m, "decision_schema") do
      {:ok,
       %__MODULE__{
         name: name,
         version: version,
         path: dir,
         title: m["title"] || name,
         entry: entry,
         min_height:
           if(is_integer(m["min_height"]) and m["min_height"] > 0, do: m["min_height"], else: 400),
         dev: m["dev"] == true
       }}
    end
  end

  defp ensure_entry(%__MODULE__{path: dir, entry: entry}) do
    if File.regular?(Path.join(dir, entry)),
      do: :ok,
      else: {:error, "entry #{entry} not found"}
  end

  defp build_schemas(plugin, manifest) do
    with {:ok, payload_schema} <- build(plugin, "payload_schema", manifest["payload_schema"]),
         {:ok, decision_schema} <- build(plugin, "decision_schema", manifest["decision_schema"]) do
      {:ok, %{plugin | payload_schema: payload_schema, decision_schema: decision_schema}}
    end
  end

  defp build(%__MODULE__{name: name, version: version, path: dir}, key, schema)
       when is_map(schema) do
    prefix = Resolver.prefix(name, version)

    root =
      schema
      |> Map.put_new("$schema", "https://json-schema.org/draft/2020-12/schema")
      |> Map.put("$id", prefix <> key)

    case JSV.build(root, resolver: {Resolver, %{prefix: prefix, dir: dir}}) do
      {:ok, built} -> {:ok, built}
      {:error, err} -> {:error, "#{key}: #{build_message(err)}"}
    end
  end

  defp build(_plugin, key, _), do: {:error, "#{key} must be a JSON Schema object"}

  # JSV's message embeds the whole resolver chain; say what our resolver hit.
  defp build_message(%JSV.BuildError{reason: {:resolver_error, entries}} = err) do
    case List.keyfind(entries, Resolver, 0) do
      {Resolver, {:schema_file, uri, reason}} ->
        "cannot load #{Path.basename(uri)}: #{:file.format_error(reason)}"

      {Resolver, {:unknown_uri, uri}} ->
        "cannot resolve #{uri}: only files in the plugin directory can be referenced"

      _ ->
        build_message(%{err | reason: :other})
    end
  end

  defp build_message(err) do
    err |> Exception.message() |> String.replace(~r/ with \S+\(.*\), /s, ": ")
  end

  # -- validation -----------------------------------------------------------

  defp validate(%__MODULE__{error: nil}, %JSV.Root{} = schema, data) when is_map(data) do
    case JSV.validate(data, schema) do
      {:ok, _} -> :ok
      {:error, %JSV.ValidationError{} = err} -> {:error, GateError.invalid(violations(err))}
    end
  end

  defp validate(%__MODULE__{error: nil}, _schema, _data),
    do: {:error, GateError.invalid(GateError.violation("", "must be a JSON object"))}

  defp validate(%__MODULE__{name: name, error: error}, _schema, _data),
    do:
      {:error,
       GateError.invalid(GateError.violation("/type", "type #{name} is not usable: #{error}"))}

  # JSV reports every schema keyword on the path to a failure; keep the leaves,
  # which are the ones that name the actual problem.
  @container_kinds [:properties, :items, :prefixItems, :allOf, :anyOf, :oneOf, :contains]

  @doc false
  def violations(%JSV.ValidationError{} = err) do
    %{details: details} = JSV.normalize_error(err, sort: :asc)

    for unit <- details,
        %{kind: kind, message: message} <- unit.errors,
        kind not in @container_kinds do
      GateError.violation(
        pointer(unit.instanceLocation),
        tidy(kind, unit.evaluationPath, message)
      )
    end
    |> Enum.uniq()
  end

  defp pointer("#" <> rest), do: rest
  defp pointer(other), do: other

  defp tidy(:boolean_schema, path, _message) do
    if String.ends_with?(path, "additionalProperties"),
      do: "unexpected property",
      else: "not allowed here"
  end

  defp tidy(_kind, _path, message), do: message

  # -- manifest field helpers ---------------------------------------------

  defp string_field(m, key, regex) do
    case m[key] do
      s when is_binary(s) ->
        if Regex.match?(regex, s),
          do: {:ok, s},
          else: {:error, "#{key} #{inspect(s)} is not valid"}

      _ ->
        {:error, "#{key} is required and must be a string"}
    end
  end

  defp int_field(m, key) do
    case m[key] do
      n when is_integer(n) and n > 0 -> {:ok, n}
      _ -> {:error, "#{key} is required and must be a positive integer"}
    end
  end

  defp present(m, key),
    do: if(Map.has_key?(m, key), do: :ok, else: {:error, "#{key} is required"})

  defp wrap({:ok, v}, _), do: {:ok, v}
  defp wrap({:error, reason}, msg), do: {:error, "#{msg} (#{inspect(reason)})"}

  defp name_hint(dir), do: Path.basename(dir)
end

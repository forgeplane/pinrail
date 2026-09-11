defmodule Wicket.Gates do
  @moduledoc """
  The gates context: create, read, list, decide, withdraw, expire.

  Writes go to the file store, then to the index, then out on PubSub. Reads of
  a single gate come from the files (they carry the payload); listings come
  from the index and carry no payload.

  Status transitions are `pending -> decided | withdrawn | expired` and nothing
  else; the store enforces that with exclusive file creation, so two racing
  decisions cannot both win.
  """

  alias Phoenix.PubSub
  alias Wicket.{Gate, GateError, Gates.Index, Store, Types, ULID}

  @pubsub Wicket.PubSub
  @topic "gates"

  @type filters :: [
          status: Gate.status() | [Gate.status()],
          type: String.t(),
          repo: String.t(),
          workflow: String.t(),
          ref: String.t(),
          run_id: String.t(),
          superseded: boolean(),
          limit: pos_integer()
        ]

  @doc """
  Creates a gate. `attrs` keys may be atoms or strings: `type` and `title` are
  required, `source` should be a map with any of `repo`, `workflow`, `run_id`,
  `ref`, `url`; `payload`, `summary`, `supersedes`, `expires_at` and
  `requested_by` are optional.

  The type must be registered and the payload must satisfy its payload
  schema; `type_version` is set from the registry. Every failure is an
  `:invalid` `Wicket.GateError` whose violations point into the attrs.
  """
  @spec create(map()) :: {:ok, Gate.t()} | {:error, GateError.t()}
  def create(attrs) when is_map(attrs) do
    attrs = Map.new(attrs, fn {k, v} -> {to_string(k), v} end)
    payload = attrs["payload"] || %{}

    with :ok <- envelope_violations(attrs),
         {:ok, plugin} <- Types.fetch(attrs["type"]),
         :ok <- Types.validate_payload(plugin, payload) |> prefix_violations("/payload"),
         {:ok, _bundle} <- Types.ensure_snapshot(plugin) do
      gate = %Gate{
        id: "g_" <> ULID.next(),
        type: plugin.name,
        type_version: plugin.version,
        title: attrs["title"],
        source: Gate.normalize_source(attrs["source"]),
        requested_by: attrs["requested_by"],
        created_at: DateTime.utc_now() |> DateTime.truncate(:second),
        expires_at: parse_datetime(attrs["expires_at"]),
        supersedes: attrs["supersedes"],
        summary: attrs["summary"],
        payload: payload
      }

      with :ok <- Store.write_gate(Wicket.data_dir(), gate) do
        Index.put(gate)
        broadcast(:created, gate)
        {:ok, gate}
      end
    end
  end

  @doc "The gate with its payload, from disk."
  @spec get(String.t()) :: {:ok, Gate.t()} | {:error, GateError.t()}
  def get(id) when is_binary(id) do
    case Store.load(Wicket.data_dir(), id) do
      {:ok, gate} -> {:ok, gate}
      {:error, _} -> {:error, GateError.not_found(id)}
    end
  end

  @doc "Like `get/1` but raises the `Wicket.GateError`."
  @spec get!(String.t()) :: Gate.t()
  def get!(id) do
    case get(id) do
      {:ok, gate} -> gate
      {:error, error} -> raise error
    end
  end

  @doc "The gate's event log, oldest first."
  @spec events(String.t()) :: [map()]
  def events(id), do: Store.events(Wicket.data_dir(), id)

  @doc """
  Gates matching the filters, newest first, without payloads.

    * `status:` one status or a list; default all
    * `type:`, `repo:`, `workflow:`, `ref:`, `run_id:` exact matches
    * `superseded: false` drops gates another gate supersedes (the inbox view)
    * `limit:` at most this many
  """
  @spec list(filters()) :: [Gate.t()]
  def list(filters \\ []) do
    now = DateTime.utc_now()
    all = Index.all()

    superseded_ids =
      if filters[:superseded] == false,
        do: MapSet.new(all, & &1.supersedes) |> MapSet.delete(nil),
        else: MapSet.new()

    statuses = filters[:status] |> List.wrap() |> MapSet.new()

    all
    |> Enum.filter(fn g ->
      (MapSet.size(statuses) == 0 or MapSet.member?(statuses, Gate.status(g, now))) and
        not MapSet.member?(superseded_ids, g.id) and
        filter_match?(filters[:type], g.type) and
        filter_match?(filters[:repo], g.source["repo"]) and
        filter_match?(filters[:workflow], g.source["workflow"]) and
        filter_match?(filters[:ref], g.source["ref"]) and
        filter_match?(filters[:run_id], g.source["run_id"])
    end)
    |> Enum.sort_by(& &1.id, :desc)
    |> then(fn gates -> if limit = filters[:limit], do: Enum.take(gates, limit), else: gates end)
  end

  @doc "Number of pending gates that no other gate supersedes."
  @spec pending_count() :: non_neg_integer()
  def pending_count, do: list(status: :pending, superseded: false) |> length()

  @doc """
  The rounds before this gate, newest first: the gate it supersedes, the one
  that one supersedes, and so on. Without payloads.
  """
  @spec chain(Gate.t() | String.t()) :: [Gate.t()]
  def chain(%Gate{supersedes: nil}), do: []

  def chain(%Gate{supersedes: prev_id}) do
    case Index.get(prev_id) do
      nil -> []
      prev -> [prev | chain(prev)]
    end
  end

  def chain(id) when is_binary(id) do
    case Index.get(id) do
      nil -> []
      gate -> chain(gate)
    end
  end

  @doc "The gate that supersedes this one, if any (index lookup, no payload)."
  @spec superseded_by(String.t()) :: Gate.t() | nil
  def superseded_by(id), do: Enum.find(Index.all(), &(&1.supersedes == id))

  @doc """
  Records the decision. `data` is the type-specific decision, validated
  against the decision schema of the type version the gate was created under.
  `opts` may carry `decided_by` (defaults to the configured user) and
  `agent_note`.

  Errors: `:invalid` with violations into `data` (the gate stays pending),
  `:not_pending` if it is decided, withdrawn or expired, `:not_found`.
  """
  @spec decide(String.t(), term(), keyword()) :: {:ok, Gate.t()} | {:error, GateError.t()}
  def decide(id, data, opts \\ []) when is_binary(id) do
    transition(id, fn gate ->
      decision = %{
        "decided_by" => opts[:decided_by] || current_user(),
        "decided_at" => Gate.iso(DateTime.utc_now()),
        "data" => data,
        "agent_note" => blank_to_nil(opts[:agent_note])
      }

      with {:ok, plugin} <- Types.fetch(gate.type, gate.type_version),
           :ok <- Types.validate_decision(plugin, data),
           :ok <- Store.write_decision(Wicket.data_dir(), gate.id, decision) do
        {:ok, :decided}
      end
    end)
  end

  @doc "The requester gives up. Same errors as `decide/3`, minus validation."
  @spec withdraw(String.t()) :: {:ok, Gate.t()} | {:error, GateError.t()}
  def withdraw(id) when is_binary(id) do
    transition(id, fn gate ->
      withdrawn = %{"withdrawn_at" => Gate.iso(DateTime.utc_now())}

      with :ok <- Store.write_withdrawn(Wicket.data_dir(), gate.id, withdrawn) do
        {:ok, :withdrawn}
      end
    end)
  end

  @doc """
  Finds gates whose `expires_at` has passed without a decision, logs the
  `expired` event once for each, and broadcasts them. Status is derived, so
  the gates already read as expired; this makes waiters and the inbox notice.
  Returns the newly expired gates.
  """
  @spec sweep_expired() :: [Gate.t()]
  def sweep_expired do
    root = Wicket.data_dir()

    for gate <- list(status: :expired),
        not Enum.any?(Store.events(root, gate.id), &(&1["event"] == "expired")) do
      Store.append_event(root, gate.id, "expired", %{})
      broadcast(:expired, gate)
      gate
    end
  end

  @doc "Logs that a human opened the gate page."
  @spec mark_viewed(String.t()) :: :ok
  def mark_viewed(id) do
    case Store.append_event(Wicket.data_dir(), id, "viewed", %{}) do
      :ok -> :ok
      {:error, _} -> :ok
    end
  end

  @doc """
  Subscribes the caller to every gate event, or to one gate's events.
  Messages: `{:gate, :created | :decided | :withdrawn | :expired, gate}`.
  """
  @spec subscribe() :: :ok
  def subscribe, do: PubSub.subscribe(@pubsub, @topic)

  @spec subscribe(String.t()) :: :ok
  def subscribe(id) when is_binary(id), do: PubSub.subscribe(@pubsub, @topic <> ":" <> id)

  @doc "The configured local user, the default `decided_by`."
  @spec current_user() :: String.t()
  def current_user, do: Application.get_env(:wicket, :user) || "wicket"

  defp transition(id, fun) do
    with {:ok, gate} <- get(id),
         :ok <- ensure_pending(gate),
         {:ok, event} <- write_or_conflict(fun.(gate), id),
         {:ok, updated} <- get(id) do
      Index.put(updated)
      broadcast(event, updated)
      {:ok, updated}
    end
  end

  defp ensure_pending(gate),
    do: if(Gate.pending?(gate), do: :ok, else: {:error, GateError.not_pending(gate.id)})

  # A racing writer got there first: the file exists, so the gate is no longer pending.
  defp write_or_conflict({:error, :eexist}, id), do: {:error, GateError.not_pending(id)}
  defp write_or_conflict({:error, :not_found}, id), do: {:error, GateError.not_found(id)}
  defp write_or_conflict(other, _id), do: other

  defp broadcast(event, %Gate{} = gate) do
    msg = {:gate, event, %{gate | payload: nil}}
    PubSub.broadcast(@pubsub, @topic, msg)
    PubSub.broadcast(@pubsub, @topic <> ":" <> gate.id, msg)
  end

  defp filter_match?(nil, _), do: true
  defp filter_match?(expected, actual), do: to_string(expected) == actual

  # Envelope checks that run before the type's schema. All violations at once.
  defp envelope_violations(attrs) do
    violations =
      Enum.reject(
        [
          string_violation(attrs, "type"),
          string_violation(attrs, "title"),
          datetime_violation(attrs["expires_at"]),
          supersedes_violation(attrs["supersedes"]),
          if(attrs["source"] != nil and not is_map(attrs["source"]),
            do: GateError.violation("/source", "must be an object")
          ),
          if(attrs["payload"] != nil and not is_map(attrs["payload"]),
            do: GateError.violation("/payload", "must be an object")
          ),
          if(attrs["summary"] != nil and not is_map(attrs["summary"]),
            do: GateError.violation("/summary", "must be an object")
          )
        ],
        &is_nil/1
      )

    if violations == [], do: :ok, else: {:error, GateError.invalid(violations)}
  end

  defp string_violation(attrs, key) do
    case attrs[key] do
      s when is_binary(s) and s != "" -> nil
      _ -> GateError.violation("/" <> key, "is required")
    end
  end

  defp datetime_violation(nil), do: nil
  defp datetime_violation(%DateTime{}), do: nil

  defp datetime_violation(s) do
    case parse_datetime(s) do
      %DateTime{} -> nil
      nil -> GateError.violation("/expires_at", "must be an ISO 8601 datetime")
    end
  end

  defp supersedes_violation(nil), do: nil

  defp supersedes_violation(id) when is_binary(id) do
    if Index.get(id), do: nil, else: GateError.violation("/supersedes", "unknown gate #{id}")
  end

  defp supersedes_violation(_), do: GateError.violation("/supersedes", "must be a gate id")

  defp parse_datetime(nil), do: nil
  defp parse_datetime(%DateTime{} = dt), do: dt

  defp parse_datetime(s) when is_binary(s) do
    case DateTime.from_iso8601(s) do
      {:ok, dt, _} -> dt
      _ -> nil
    end
  end

  defp parse_datetime(_), do: nil

  defp prefix_violations(:ok, _prefix), do: :ok

  defp prefix_violations({:error, %GateError{reason: :invalid} = e}, prefix),
    do: {:error, GateError.invalid(Enum.map(e.violations, &%{&1 | path: prefix <> &1.path}))}

  defp prefix_violations(other, _prefix), do: other

  defp blank_to_nil(nil), do: nil

  defp blank_to_nil(s) when is_binary(s) do
    case String.trim(s) do
      "" -> nil
      trimmed -> trimmed
    end
  end
end

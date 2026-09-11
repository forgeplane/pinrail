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
  alias Wicket.{Gate, Gates.Index, Store, ULID}

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

  # -- create ---------------------------------------------------------------

  @doc """
  Creates a gate. `attrs` keys may be atoms or strings: `type` and `title` are
  required, `source` should be a map with any of `repo`, `workflow`, `run_id`,
  `ref`, `url`; `payload`, `summary`, `supersedes`, `expires_at`,
  `requested_by` and `type_version` are optional.

  Payload validation against the type's schema is not done here; see
  `Wicket.Types` (step 3).
  """
  @spec create(map()) :: {:ok, Gate.t()} | {:error, term()}
  def create(attrs) when is_map(attrs) do
    attrs = Map.new(attrs, fn {k, v} -> {to_string(k), v} end)

    with {:ok, type} <- required_string(attrs, "type"),
         {:ok, title} <- required_string(attrs, "title"),
         {:ok, expires_at} <- optional_datetime(attrs["expires_at"]),
         {:ok, supersedes} <- optional_supersedes(attrs["supersedes"]) do
      gate = %Gate{
        id: "g_" <> ULID.next(),
        type: type,
        type_version: attrs["type_version"] || 1,
        title: title,
        source: Gate.normalize_source(attrs["source"]),
        requested_by: attrs["requested_by"],
        created_at: DateTime.utc_now() |> DateTime.truncate(:second),
        expires_at: expires_at,
        supersedes: supersedes,
        summary: attrs["summary"],
        payload: attrs["payload"] || %{}
      }

      with :ok <- Store.write_gate(Wicket.data_dir(), gate) do
        Index.put(gate)
        broadcast(:created, gate)
        {:ok, gate}
      end
    end
  end

  # -- read -----------------------------------------------------------------

  @doc "The gate with its payload, from disk."
  @spec get(String.t()) :: {:ok, Gate.t()} | {:error, :not_found}
  def get(id) when is_binary(id) do
    case Store.load(Wicket.data_dir(), id) do
      {:ok, gate} -> {:ok, gate}
      {:error, _} -> {:error, :not_found}
    end
  end

  @doc "Like `get/1` but raises."
  @spec get!(String.t()) :: Gate.t()
  def get!(id) do
    case get(id) do
      {:ok, gate} -> gate
      {:error, :not_found} -> raise "gate #{id} not found"
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

  # -- transitions ----------------------------------------------------------

  @doc """
  Records the decision. `data` is the type-specific decision; `opts` may carry
  `decided_by` (defaults to the configured user) and `agent_note`.

  Returns `{:error, :not_pending}` if the gate is decided, withdrawn or
  expired, and `{:error, :not_found}` if it does not exist. Validation against
  the decision schema is the caller's job (step 3).
  """
  @spec decide(String.t(), map(), keyword()) ::
          {:ok, Gate.t()} | {:error, :not_pending | :not_found | term()}
  def decide(id, data, opts \\ []) when is_binary(id) and is_map(data) do
    transition(id, fn gate ->
      decision = %{
        "decided_by" => opts[:decided_by] || current_user(),
        "decided_at" => Gate.iso(DateTime.utc_now()),
        "data" => data,
        "agent_note" => blank_to_nil(opts[:agent_note])
      }

      with :ok <- Store.write_decision(Wicket.data_dir(), gate.id, decision) do
        {:ok, :decided}
      end
    end)
  end

  @doc "The requester gives up. Same errors as `decide/3`."
  @spec withdraw(String.t()) :: {:ok, Gate.t()} | {:error, :not_pending | :not_found | term()}
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

  # -- pubsub ---------------------------------------------------------------

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

  # -- internals ------------------------------------------------------------

  defp transition(id, fun) do
    with {:ok, gate} <- get(id),
         :ok <- ensure_pending(gate),
         {:ok, event} <- write_or_conflict(fun.(gate)),
         {:ok, updated} <- get(id) do
      Index.put(updated)
      broadcast(event, updated)
      {:ok, updated}
    end
  end

  defp ensure_pending(gate), do: if(Gate.pending?(gate), do: :ok, else: {:error, :not_pending})

  # A racing writer got there first: the file exists, so the gate is no longer pending.
  defp write_or_conflict({:error, :eexist}), do: {:error, :not_pending}
  defp write_or_conflict(other), do: other

  defp broadcast(event, %Gate{} = gate) do
    msg = {:gate, event, %{gate | payload: nil}}
    PubSub.broadcast(@pubsub, @topic, msg)
    PubSub.broadcast(@pubsub, @topic <> ":" <> gate.id, msg)
  end

  defp filter_match?(nil, _), do: true
  defp filter_match?(expected, actual), do: to_string(expected) == actual

  defp required_string(attrs, key) do
    case attrs[key] do
      s when is_binary(s) and s != "" -> {:ok, s}
      _ -> {:error, {:missing, key}}
    end
  end

  defp optional_datetime(nil), do: {:ok, nil}
  defp optional_datetime(%DateTime{} = dt), do: {:ok, dt}

  defp optional_datetime(s) when is_binary(s) do
    case DateTime.from_iso8601(s) do
      {:ok, dt, _} -> {:ok, dt}
      _ -> {:error, {:invalid, "expires_at"}}
    end
  end

  defp optional_datetime(_), do: {:error, {:invalid, "expires_at"}}

  defp optional_supersedes(nil), do: {:ok, nil}

  defp optional_supersedes(id) when is_binary(id) do
    if Index.get(id), do: {:ok, id}, else: {:error, {:unknown_gate, id}}
  end

  defp optional_supersedes(_), do: {:error, {:invalid, "supersedes"}}

  defp blank_to_nil(nil), do: nil

  defp blank_to_nil(s) when is_binary(s) do
    case String.trim(s) do
      "" -> nil
      trimmed -> trimmed
    end
  end
end

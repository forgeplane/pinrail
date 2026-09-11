defmodule Wicket.Gate do
  @moduledoc """
  One gate: the envelope, the payload snapshot and, once decided, the decision.

  Nothing here is mutated after creation except by the file the field mirrors:
  `decision`/`agent_note` come from `decision.json`, `withdrawn_at` from
  `withdrawn.json`. `status/2` derives the status from those and `expires_at`.
  """

  @type status :: :pending | :decided | :withdrawn | :expired

  @type decision :: %{
          decided_by: String.t(),
          decided_at: DateTime.t(),
          data: map()
        }

  @type t :: %__MODULE__{
          id: String.t(),
          type: String.t(),
          type_version: pos_integer(),
          title: String.t(),
          source: map(),
          requested_by: String.t() | nil,
          created_at: DateTime.t(),
          expires_at: DateTime.t() | nil,
          supersedes: String.t() | nil,
          summary: map() | nil,
          payload: map() | nil,
          decision: decision() | nil,
          agent_note: String.t() | nil,
          withdrawn_at: DateTime.t() | nil
        }

  @enforce_keys [:id, :type, :title, :source, :created_at]
  defstruct [
    :id,
    :type,
    :title,
    :source,
    :created_at,
    :requested_by,
    :expires_at,
    :supersedes,
    :summary,
    :payload,
    :decision,
    :agent_note,
    :withdrawn_at,
    type_version: 1
  ]

  @source_keys ~w(repo workflow run_id ref url)

  @doc "The derived status at `now`."
  @spec status(t(), DateTime.t()) :: status()
  def status(gate, now \\ DateTime.utc_now())
  def status(%__MODULE__{decision: %{}}, _now), do: :decided
  def status(%__MODULE__{withdrawn_at: %DateTime{}}, _now), do: :withdrawn

  def status(%__MODULE__{expires_at: %DateTime{} = at}, now) do
    if DateTime.compare(at, now) == :gt, do: :pending, else: :expired
  end

  def status(%__MODULE__{}, _now), do: :pending

  @doc "True if the gate is still waiting for a decision."
  def pending?(gate, now \\ DateTime.utc_now()), do: status(gate, now) == :pending

  @doc """
  The write-once part of the gate as stored in `gate.json`: envelope + payload,
  no decision and no derived status.
  """
  @spec envelope_json(t()) :: map()
  def envelope_json(%__MODULE__{} = g) do
    %{
      "id" => g.id,
      "type" => g.type,
      "type_version" => g.type_version,
      "title" => g.title,
      "source" => g.source,
      "requested_by" => g.requested_by,
      "created_at" => iso(g.created_at),
      "expires_at" => iso(g.expires_at),
      "supersedes" => g.supersedes,
      "summary" => g.summary,
      "payload" => g.payload
    }
  end

  @doc """
  The full public representation (§5 of the spec): envelope, payload, derived
  status, decision and agent note. Pass `payload: false` to omit the payload,
  which is what listings do.
  """
  @spec to_map(t(), keyword()) :: map()
  def to_map(%__MODULE__{} = g, opts \\ []) do
    base =
      g
      |> envelope_json()
      |> Map.merge(%{
        "status" => Atom.to_string(status(g)),
        "decision" => decision_json(g.decision),
        "agent_note" => g.agent_note,
        "withdrawn_at" => iso(g.withdrawn_at)
      })

    if Keyword.get(opts, :payload, true), do: base, else: Map.delete(base, "payload")
  end

  @doc "Rebuilds a gate from the stored `gate.json` map."
  @spec from_json(map()) :: t()
  def from_json(%{} = m) do
    %__MODULE__{
      id: m["id"],
      type: m["type"],
      type_version: m["type_version"] || 1,
      title: m["title"],
      source: m["source"] || %{},
      requested_by: m["requested_by"],
      created_at: parse_dt!(m["created_at"]),
      expires_at: parse_dt(m["expires_at"]),
      supersedes: m["supersedes"],
      summary: m["summary"],
      payload: m["payload"]
    }
  end

  @doc "Attaches the contents of `decision.json`."
  def put_decision(%__MODULE__{} = g, %{} = d) do
    %{
      g
      | decision: %{
          decided_by: d["decided_by"],
          decided_at: parse_dt!(d["decided_at"]),
          data: d["data"]
        },
        agent_note: d["agent_note"]
    }
  end

  @doc "Attaches the contents of `withdrawn.json`."
  def put_withdrawn(%__MODULE__{} = g, %{} = w),
    do: %{g | withdrawn_at: parse_dt!(w["withdrawn_at"])}

  @doc "The `source` map with only the known keys, all strings."
  def normalize_source(source) when is_map(source) do
    for {k, v} <- source, key = to_string(k), key in @source_keys, into: %{} do
      {key, if(is_nil(v), do: nil, else: to_string(v))}
    end
  end

  def normalize_source(_), do: %{}

  defp decision_json(nil), do: nil

  defp decision_json(d),
    do: %{"decided_by" => d.decided_by, "decided_at" => iso(d.decided_at), "data" => d.data}

  @doc false
  def iso(nil), do: nil
  def iso(%DateTime{} = dt), do: dt |> DateTime.truncate(:second) |> DateTime.to_iso8601()

  defp parse_dt(nil), do: nil
  defp parse_dt(s), do: parse_dt!(s)

  defp parse_dt!(%DateTime{} = dt), do: dt

  defp parse_dt!(s) when is_binary(s) do
    {:ok, dt, _offset} = DateTime.from_iso8601(s)
    dt
  end

  defimpl JSON.Encoder do
    def encode(gate, encoder), do: encoder.(Wicket.Gate.to_map(gate), encoder)
  end
end

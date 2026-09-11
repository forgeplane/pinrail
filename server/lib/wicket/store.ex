defmodule Wicket.Store do
  @moduledoc """
  The on-disk layout under the data directory, and the only module that writes
  to it. Every file is write-once:

      gates/<id>/gate.json        envelope + payload   (tmp + rename)
      gates/<id>/decision.json    the decision          (exclusive create)
      gates/<id>/withdrawn.json   withdrawal timestamp  (exclusive create)
      gates/<id>/events.jsonl     append-only log

  "At most one decision" is the exclusive create of `decision.json`: a second
  writer gets `{:error, :eexist}` from the filesystem, not from a lock.
  """

  alias Wicket.Gate

  @gate_file "gate.json"
  @decision_file "decision.json"
  @withdrawn_file "withdrawn.json"
  @events_file "events.jsonl"

  @spec gates_dir(Path.t()) :: Path.t()
  def gates_dir(root), do: Path.join(root, "gates")

  @spec gate_dir(Path.t(), String.t()) :: Path.t()
  def gate_dir(root, id), do: Path.join(gates_dir(root), id)

  @doc "Ids of every stored gate, lexicographic (creation) order."
  @spec list_ids(Path.t()) :: [String.t()]
  def list_ids(root) do
    case File.ls(gates_dir(root)) do
      {:ok, names} -> names |> Enum.filter(&String.starts_with?(&1, "g_")) |> Enum.sort()
      {:error, :enoent} -> []
    end
  end

  @doc """
  Writes a new gate. Fails if the gate directory already holds a `gate.json`.
  The file is written to a temp name and renamed, so a reader never sees a
  partial envelope.
  """
  @spec write_gate(Path.t(), Gate.t()) :: :ok | {:error, :eexist | File.posix()}
  def write_gate(root, %Gate{} = gate) do
    dir = gate_dir(root, gate.id)
    path = Path.join(dir, @gate_file)

    if File.exists?(path) do
      {:error, :eexist}
    else
      with :ok <- File.mkdir_p(dir),
           :ok <- write_atomic(path, encode(Gate.envelope_json(gate))) do
        append_event(root, gate.id, "created", %{})
      end
    end
  end

  @doc "Records the decision. `{:error, :eexist}` if one is already there."
  @spec write_decision(Path.t(), String.t(), map()) :: :ok | {:error, :eexist | File.posix()}
  def write_decision(root, id, %{} = decision) do
    with :ok <- create_exclusive(Path.join(gate_dir(root, id), @decision_file), encode(decision)) do
      append_event(root, id, "decided", %{"decided_by" => decision["decided_by"]})
    end
  end

  @doc "Records a withdrawal. `{:error, :eexist}` if already withdrawn."
  @spec write_withdrawn(Path.t(), String.t(), map()) :: :ok | {:error, :eexist | File.posix()}
  def write_withdrawn(root, id, %{} = withdrawn) do
    with :ok <-
           create_exclusive(Path.join(gate_dir(root, id), @withdrawn_file), encode(withdrawn)) do
      append_event(root, id, "withdrawn", %{})
    end
  end

  @doc "Appends one line to the gate's event log."
  @spec append_event(Path.t(), String.t(), String.t(), map()) :: :ok | {:error, File.posix()}
  def append_event(root, id, event, %{} = attrs) do
    line =
      JSON.encode!(Map.merge(attrs, %{"event" => event, "at" => Gate.iso(DateTime.utc_now())}))

    File.write(Path.join(gate_dir(root, id), @events_file), line <> "\n", [:append])
  end

  @doc "The gate's event log, oldest first."
  @spec events(Path.t(), String.t()) :: [map()]
  def events(root, id) do
    case File.read(Path.join(gate_dir(root, id), @events_file)) do
      {:ok, body} -> body |> String.split("\n", trim: true) |> Enum.map(&JSON.decode!/1)
      {:error, _} -> []
    end
  end

  @doc "Loads a gate with its decision or withdrawal, if any."
  @spec load(Path.t(), String.t()) :: {:ok, Gate.t()} | {:error, :not_found | term()}
  def load(root, id) do
    dir = gate_dir(root, id)

    with {:ok, envelope} <- read_json(Path.join(dir, @gate_file)) do
      gate = Gate.from_json(envelope)

      gate =
        case read_json(Path.join(dir, @decision_file)) do
          {:ok, d} -> Gate.put_decision(gate, d)
          _ -> gate
        end

      gate =
        case read_json(Path.join(dir, @withdrawn_file)) do
          {:ok, w} -> Gate.put_withdrawn(gate, w)
          _ -> gate
        end

      {:ok, gate}
    end
  end

  defp write_atomic(path, body) do
    tmp = path <> ".tmp." <> Integer.to_string(System.unique_integer([:positive]))

    with :ok <- File.write(tmp, body),
         :ok <- File.rename(tmp, path) do
      :ok
    else
      err ->
        File.rm(tmp)
        err
    end
  end

  defp create_exclusive(path, body) do
    case File.open(path, [:write, :exclusive, :binary]) do
      {:ok, io} ->
        result = IO.binwrite(io, body)
        File.close(io)
        result

      {:error, :enoent} ->
        {:error, :not_found}

      {:error, reason} ->
        {:error, reason}
    end
  end

  defp read_json(path) do
    case File.read(path) do
      {:ok, body} ->
        case JSON.decode(body) do
          {:ok, map} when is_map(map) -> {:ok, map}
          {:ok, _} -> {:error, {:invalid_json, path}}
          {:error, _} -> {:error, {:invalid_json, path}}
        end

      {:error, :enoent} ->
        {:error, :not_found}

      {:error, reason} ->
        {:error, reason}
    end
  end

  defp encode(map), do: JSON.encode!(map) <> "\n"
end

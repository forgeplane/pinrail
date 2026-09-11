defmodule WicketWeb.API.GateController do
  @moduledoc """
  `/api/gates`: create, read, list, long-poll wait, decide, withdraw.

  Listings carry envelopes without payloads; `show` and `wait` carry the
  payload too. Every error is a `Wicket.GateError` rendered by the fallback:
  422 with violations, 404, 409.
  """
  use WicketWeb, :controller

  alias Wicket.{Gate, GateError, Gates}

  action_fallback WicketWeb.API.FallbackController

  @statuses %{
    "pending" => :pending,
    "decided" => :decided,
    "withdrawn" => :withdrawn,
    "expired" => :expired
  }

  @default_wait 300
  @max_wait 600

  def index(conn, params) do
    with {:ok, filters} <- filters(params) do
      gates = Gates.list(filters)
      json(conn, Enum.map(gates, &Gate.to_map(&1, payload: false)))
    end
  end

  def create(conn, params) do
    with {:ok, gate} <- Gates.create(Map.drop(params, ["id"])) do
      conn
      |> put_status(:created)
      |> json(gate)
    end
  end

  def show(conn, %{"id" => id}) do
    with {:ok, gate} <- Gates.get(id) do
      json(conn, gate)
    end
  end

  @doc """
  Blocks until the gate leaves `pending`, or `timeout` seconds pass (204).
  The subscription is opened before the first read so a transition between
  the two cannot be missed.
  """
  def wait(conn, %{"id" => id} = params) do
    timeout = params["timeout"] |> parse_int(@default_wait) |> min(@max_wait) |> max(0)
    :ok = Gates.subscribe(id)

    with {:ok, gate} <- Gates.get(id) do
      deadline = System.monotonic_time(:millisecond) + timeout * 1000

      case await(gate, deadline) do
        {:ok, gate} -> json(conn, gate)
        :timeout -> send_resp(conn, :no_content, "")
      end
    end
  end

  def decide(conn, %{"id" => id} = params) do
    with {:ok, data} <- fetch_data(params),
         {:ok, gate} <-
           Gates.decide(id, data,
             decided_by: params["decided_by"],
             agent_note: params["agent_note"]
           ) do
      json(conn, gate)
    end
  end

  def withdraw(conn, %{"id" => id}) do
    with {:ok, gate} <- Gates.withdraw(id) do
      json(conn, gate)
    end
  end

  defp await(%Gate{} = gate, deadline) do
    if Gate.pending?(gate) do
      remaining = deadline - System.monotonic_time(:millisecond)
      # wake up when the gate would expire on its own, even without a sweep
      wake = min(remaining, until_expiry(gate))

      if remaining <= 0 do
        :timeout
      else
        receive do
          {:gate, _event, %Gate{id: id}} when id == gate.id ->
            refetch(gate.id, deadline)
        after
          max(wake, 0) -> refetch(gate.id, deadline)
        end
      end
    else
      {:ok, gate}
    end
  end

  defp refetch(id, deadline) do
    case Gates.get(id) do
      {:ok, gate} -> await(gate, deadline)
      {:error, _} -> :timeout
    end
  end

  defp until_expiry(%Gate{expires_at: nil}), do: :infinity

  defp until_expiry(%Gate{expires_at: at}),
    do: max(DateTime.diff(at, DateTime.utc_now(), :millisecond) + 50, 0)

  defp filters(params) do
    with {:ok, status} <- parse_status(params["status"]) do
      filters =
        [
          status: status,
          type: params["type"],
          repo: params["repo"],
          workflow: params["workflow"],
          ref: params["ref"],
          run_id: params["run_id"],
          superseded: if(params["superseded"] == "false", do: false),
          limit: parse_int(params["limit"], nil)
        ]
        |> Enum.reject(fn {_k, v} -> is_nil(v) end)

      {:ok, filters}
    end
  end

  defp parse_status(nil), do: {:ok, nil}

  defp parse_status(s) when is_binary(s) do
    names = String.split(s, ",", trim: true)

    case Enum.reject(names, &Map.has_key?(@statuses, &1)) do
      [] ->
        {:ok, Enum.map(names, &@statuses[&1])}

      bad ->
        {:error,
         GateError.invalid(
           GateError.violation("/status", "unknown status #{Enum.join(bad, ", ")}")
         )}
    end
  end

  defp parse_int(nil, default), do: default
  defp parse_int(n, _default) when is_integer(n), do: n

  defp parse_int(s, default) when is_binary(s) do
    case Integer.parse(s) do
      {n, ""} -> n
      _ -> default
    end
  end

  defp fetch_data(%{"data" => data}), do: {:ok, data}

  defp fetch_data(_),
    do: {:error, GateError.invalid(GateError.violation("/data", "is required"))}
end

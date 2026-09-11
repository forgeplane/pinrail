defmodule Wicket.GateError do
  @moduledoc """
  The one error value the gates API returns and raises.

  `reason` is the discriminator callers match on:

    * `:not_found` – no such gate (HTTP 404)
    * `:not_pending` – the gate is decided, withdrawn or expired (HTTP 409)
    * `:invalid` – the input failed validation; `violations` says where (HTTP 422)

  A violation is `%{path: "/decisions/0/action", message: "…"}`, the path a
  JSON pointer into the offending document. That is the shape the plugin
  bridge relays to the view and the API returns in a 422 body.
  """

  @type reason :: :not_found | :not_pending | :invalid
  @type violation :: %{path: String.t(), message: String.t()}
  @type t :: %__MODULE__{
          reason: reason(),
          message: String.t(),
          violations: [violation()],
          gate_id: String.t() | nil
        }

  defexception [:reason, :message, violations: [], gate_id: nil]

  @impl true
  def exception(opts) when is_list(opts) do
    reason = Keyword.fetch!(opts, :reason)
    violations = Keyword.get(opts, :violations, [])
    gate_id = Keyword.get(opts, :gate_id)

    %__MODULE__{
      reason: reason,
      violations: violations,
      gate_id: gate_id,
      message:
        Keyword.get_lazy(opts, :message, fn -> default_message(reason, gate_id, violations) end)
    }
  end

  @spec not_found(String.t()) :: t()
  def not_found(id), do: exception(reason: :not_found, gate_id: id)

  @spec not_pending(String.t()) :: t()
  def not_pending(id), do: exception(reason: :not_pending, gate_id: id)

  @spec invalid([violation()] | violation()) :: t()
  def invalid(violations), do: exception(reason: :invalid, violations: List.wrap(violations))

  @doc "Builds one violation."
  @spec violation(String.t(), String.t()) :: violation()
  def violation(path, message) when is_binary(path) and is_binary(message),
    do: %{path: path, message: message}

  @doc "The JSON body for an API error response."
  @spec to_map(t()) :: map()
  def to_map(%__MODULE__{} = e) do
    %{
      "error" => Atom.to_string(e.reason),
      "message" => e.message,
      "violations" => Enum.map(e.violations, &%{"path" => &1.path, "message" => &1.message})
    }
  end

  @doc "HTTP status for the reason."
  @spec status(t()) :: 404 | 409 | 422
  def status(%__MODULE__{reason: :not_found}), do: 404
  def status(%__MODULE__{reason: :not_pending}), do: 409
  def status(%__MODULE__{reason: :invalid}), do: 422

  defp default_message(:not_found, id, _), do: "gate #{id} not found"
  defp default_message(:not_pending, id, _), do: "gate #{id} is no longer pending"

  defp default_message(:invalid, _, violations) do
    lines =
      Enum.map(violations, fn v ->
        "  #{if(v.path == "", do: "/", else: v.path)}: #{v.message}"
      end)

    Enum.join(["validation failed:" | lines], "\n")
  end

  defimpl Plug.Exception do
    def status(err), do: Wicket.GateError.status(err)
    def actions(_), do: []
  end
end

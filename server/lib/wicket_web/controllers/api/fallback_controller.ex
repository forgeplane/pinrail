defmodule WicketWeb.API.FallbackController do
  @moduledoc """
  Renders the `{:error, ...}` values the API actions return: a
  `Wicket.GateError` becomes its status and JSON body, anything else a 400.
  """
  use WicketWeb, :controller

  alias Wicket.GateError

  def call(conn, {:error, %GateError{} = error}) do
    conn
    |> put_status(GateError.status(error))
    |> json(GateError.to_map(error))
  end

  def call(conn, {:error, message}) when is_binary(message) do
    conn
    |> put_status(:bad_request)
    |> json(%{"error" => "bad_request", "message" => message, "violations" => []})
  end
end

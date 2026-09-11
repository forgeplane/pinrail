defmodule Wicket.BrowserHelpers do
  @moduledoc """
  Helpers for the Playwright-driven browser tests.
  """

  @doc """
  Runs `fun` with the session scoped to the plugin's sandboxed iframe, so
  assertions and clicks land inside the plugin's document.
  """
  def within_plugin(conn, fun) when is_function(fun, 1) do
    PhoenixTest.within(conn, "#plugin-frame >> internal:control=enter-frame", fun)
  end
end

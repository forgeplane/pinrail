defmodule WicketWeb.Format do
  @moduledoc """
  Small formatting helpers for templates: relative ages and timestamps.
  """

  @doc "How long ago, tersely: 12s, 3m, 2h, 5d."
  @spec age(DateTime.t() | nil, DateTime.t()) :: String.t()
  def age(at, now \\ DateTime.utc_now())
  def age(nil, _now), do: ""

  def age(%DateTime{} = at, now) do
    seconds = max(DateTime.diff(now, at, :second), 0)

    cond do
      seconds < 60 -> "#{seconds}s"
      seconds < 3600 -> "#{div(seconds, 60)}m"
      seconds < 86_400 -> "#{div(seconds, 3600)}h"
      true -> "#{div(seconds, 86_400)}d"
    end
  end

  @doc "An absolute timestamp for titles and history rows."
  @spec stamp(DateTime.t() | nil) :: String.t()
  def stamp(nil), do: ""
  def stamp(%DateTime{} = at), do: Calendar.strftime(at, "%Y-%m-%d %H:%M UTC")
end

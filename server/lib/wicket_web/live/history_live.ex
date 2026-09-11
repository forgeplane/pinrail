defmodule WicketWeb.HistoryLive do
  @moduledoc """
  Decided, withdrawn and expired gates.
  """
  use WicketWeb, :live_view

  @impl true
  def mount(_params, _session, socket) do
    {:ok, assign(socket, page_title: "History", page: :history, pending_count: 0)}
  end

  @impl true
  def render(assigns) do
    ~H"""
    <Layouts.app flash={@flash} page={@page}>
      <.page_header title="History" />
      <.empty_state id="history-empty">No gate has been decided yet.</.empty_state>
    </Layouts.app>
    """
  end
end

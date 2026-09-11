defmodule WicketWeb.InboxLive do
  @moduledoc "Pending gates, newest first."
  use WicketWeb, :live_view

  @impl true
  def mount(_params, _session, socket) do
    {:ok, assign(socket, page_title: "Inbox", page: :inbox, pending_count: 0)}
  end

  @impl true
  def render(assigns) do
    ~H"""
    <Layouts.app flash={@flash} page={@page}>
      <.page_header title="Inbox" />
      <.empty_state id="inbox-empty">Nothing is waiting for a decision.</.empty_state>
    </Layouts.app>
    """
  end
end

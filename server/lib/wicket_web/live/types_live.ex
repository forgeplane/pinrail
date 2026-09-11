defmodule WicketWeb.TypesLive do
  @moduledoc "Registered gate types."
  use WicketWeb, :live_view

  @impl true
  def mount(_params, _session, socket) do
    {:ok, assign(socket, page_title: "Types", page: :types, pending_count: 0)}
  end

  @impl true
  def render(assigns) do
    ~H"""
    <Layouts.app flash={@flash} page={@page}>
      <.page_header title="Types" />
      <.empty_state id="types-empty">No gate types are registered.</.empty_state>
    </Layouts.app>
    """
  end
end

defmodule WicketWeb.Live.PendingCount do
  @moduledoc """
  `on_mount` hook that keeps `@pending_count` current on every page and
  carries it as a badge in the document title. Views set their title with
  `assign_title/2`; the hook re-derives `page_title` whenever the count
  changes. Subscribes to all gate events; views that handle those events
  themselves get them after this hook.
  """
  import Phoenix.Component
  import Phoenix.LiveView

  alias Wicket.Gates

  def on_mount(:default, _params, _session, socket) do
    if connected?(socket), do: Gates.subscribe()

    socket =
      socket
      |> assign(:pending_count, Gates.pending_count())
      |> attach_hook(:pending_count, :handle_info, &handle_info/2)

    {:cont, socket}
  end

  @doc "Sets the page title, badged with the pending count."
  def assign_title(socket, title) do
    socket
    |> assign(:base_title, title)
    |> assign(:page_title, badge(socket.assigns.pending_count) <> title)
  end

  defp handle_info({:gate, _event, _gate}, socket) do
    count = Gates.pending_count()
    socket = assign(socket, :pending_count, count)

    socket =
      case socket.assigns[:base_title] do
        nil -> socket
        title -> assign(socket, :page_title, badge(count) <> title)
      end

    if function_exported?(socket.view, :handle_info, 2),
      do: {:cont, socket},
      else: {:halt, socket}
  end

  defp handle_info(_other, socket), do: {:cont, socket}

  defp badge(count) when is_integer(count) and count > 0, do: "(#{count}) "
  defp badge(_), do: ""
end

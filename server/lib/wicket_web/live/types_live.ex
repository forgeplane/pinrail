defmodule WicketWeb.TypesLive do
  @moduledoc """
  Registered gate types, their versions and load errors, and the directories
  they come from. Mostly for debugging a requester.
  """
  use WicketWeb, :live_view

  alias Wicket.Types
  alias Wicket.Types.Plugin

  @impl true
  def mount(_params, _session, socket) do
    {:ok, socket |> assign_title("Types") |> assign(page: :types) |> load()}
  end

  @impl true
  def handle_event("reload", _params, socket) do
    socket =
      case Types.reload() do
        {:ok, n} -> put_flash(socket, :info, "Reloaded #{n} types")
        {:error, message} -> put_flash(socket, :error, message)
      end

    {:noreply, load(socket)}
  end

  defp load(socket), do: assign(socket, types: Types.all(), dirs: Types.dirs())

  @impl true
  def render(assigns) do
    ~H"""
    <Layouts.app flash={@flash} page={@page}>
      <.page_header title="Types">
        <:subtitle>{length(@types)} registered</:subtitle>
        <:actions><.button phx-click="reload">reload</.button></:actions>
      </.page_header>

      <.empty_state :if={@types == []} id="types-empty">No gate types are registered.</.empty_state>

      <div
        :for={t <- @types}
        id={"type-#{t.name}"}
        class="rounded-lg border border-border bg-panel px-4 py-3"
      >
        <div class="flex flex-wrap items-center gap-2">
          <span class="text-[13.5px] font-semibold text-text">{t.title}</span>
          <.type_badge type={t.name} version={t.version} />
          <span
            :if={t.dev}
            class="rounded bg-sev-major/15 px-1.5 py-0.5 text-[10.5px] font-semibold uppercase text-sev-major"
          >dev</span>
          <span
            :if={t.error}
            class="rounded bg-danger/10 px-1.5 py-0.5 text-[10.5px] font-semibold uppercase text-danger"
          >broken</span>
        </div>
        <div class="mt-1 font-mono text-[11.5px] text-faint">{t.path}</div>
        <div :if={t.error} class="mt-1 text-[12px] text-danger">{t.error}</div>
        <div :if={Plugin.usable?(t)} class="mt-1 text-[12px] text-dim">
          entry {t.entry} · min height {t.min_height}px ·
          <a href={~p"/plugins/#{t.name}/#{t.version}/#{t.entry}"} target="_blank" rel="noreferrer">bundle</a>
        </div>
      </div>

      <h2 class="pt-2 text-[11px] font-bold uppercase tracking-[0.08em] text-faint">
        plugin directories
      </h2>
      <ul id="type-dirs" class="space-y-1 font-mono text-[12px] text-dim">
        <li :for={d <- @dirs}>{d}</li>
      </ul>
    </Layouts.app>
    """
  end
end

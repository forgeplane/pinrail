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
    {:ok,
     socket
     |> assign_title("Types")
     |> assign(page: :types)
     |> stream_configure(:types, dom_id: &"type-#{&1.name}")
     |> load()}
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

  defp load(socket) do
    types = Types.all()

    socket
    |> assign(types_empty?: types == [], dirs: Types.dirs())
    |> stream(:types, types, reset: true)
  end

  @impl true
  def render(assigns) do
    ~H"""
    <Layouts.app
      flash={@flash}
      page={@page}
      pending_count={@pending_count}
      repositories={@repositories}
    >
      <.page_header title="Gate types">
        <:actions>
          <.button id="reload-types" phx-click="reload" phx-disable-with="Reloading…">
            <.icon name="hero-arrow-path" class="size-3.5" /> reload
          </.button>
        </:actions>
      </.page_header>

      <.empty_state
        :if={@types_empty?}
        id="types-empty"
        title="A view for every decision"
        icon="hero-squares-2x2"
      >
        No gate types are registered. Point wicket at a directory of plugins to give
        your agents a view to ask through.
      </.empty_state>

      <div id="registered-types" phx-update="stream" class="plugin-list">
        <article :for={{dom_id, t} <- @streams.types} id={dom_id} class="plugin-row">
          <div>
            <h2>{t.title || t.name}</h2>
            <div class="mt-2"><.type_badge type={t.name} version={t.version} /></div>
          </div>
          <div class="plugin-path">
            <div>{t.path}</div>
            <div :if={Plugin.usable?(t)} class="mt-1">
              entry {t.entry} ·
              <a
                href={~p"/plugins/#{t.name}/#{t.version}/#{t.entry}"}
                target="_blank"
                rel="noreferrer"
              >bundle ↗</a>
            </div>
          </div>
          <span class={["text-xs", t.error && "text-danger", !t.error && "text-ok"]}>
            {cond do
              t.error -> "broken"
              t.dev -> "development"
              true -> "ready"
            end}
          </span>
          <p :if={t.error} class="plugin-error">{t.error}</p>
        </article>
      </div>

      <section class="plugin-dirs">
        <h2>Plugin directories</h2>
        <ul id="type-dirs" class="font-mono">
          <li :for={d <- @dirs}>{d}</li>
        </ul>
      </section>
    </Layouts.app>
    """
  end
end

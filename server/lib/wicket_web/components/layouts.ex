defmodule WicketWeb.Layouts do
  @moduledoc """
  The root HTML skeleton (`layouts/root.html.heex`) and the app chrome every
  page renders inside: header with navigation, main column, flash messages.
  """
  use WicketWeb, :html

  embed_templates "layouts/*"

  @nav [
    {:inbox, "Inbox", "/"},
    {:history, "History", "/history"},
    {:types, "Types", "/types"}
  ]

  attr :flash, :map, required: true, doc: "the map of flash messages"
  attr :page, :atom, default: nil, doc: "which nav entry is current"
  slot :inner_block, required: true

  def app(assigns) do
    assigns = assign(assigns, :nav, @nav)

    ~H"""
    <header class="sticky top-0 z-10 flex items-center gap-6 border-b border-border bg-panel px-5 h-12">
      <.link navigate={~p"/"} class="font-semibold tracking-tight text-text hover:no-underline">
        wicket
      </.link>
      <nav class="flex items-center gap-1" aria-label="Main">
        <.link
          :for={{key, label, path} <- @nav}
          navigate={path}
          aria-current={@page == key && "page"}
          class={[
            "rounded-md px-2.5 py-1 text-[12.5px] hover:no-underline",
            @page == key && "bg-hover text-text",
            @page != key && "text-dim hover:text-text"
          ]}
        >
          {label}
        </.link>
      </nav>
    </header>

    <main class="px-5 py-6">
      <div class="mx-auto max-w-5xl space-y-4">
        {render_slot(@inner_block)}
      </div>
    </main>

    <.flash_group flash={@flash} />
    """
  end

  attr :flash, :map, required: true, doc: "the map of flash messages"
  attr :id, :string, default: "flash-group", doc: "the optional id of flash container"

  def flash_group(assigns) do
    ~H"""
    <div id={@id} aria-live="polite">
      <.flash kind={:info} flash={@flash} />
      <.flash kind={:error} flash={@flash} />

      <.flash
        id="client-error"
        kind={:error}
        title="Disconnected"
        phx-disconnected={
          show(".phx-client-error #client-error")
          |> JS.remove_attribute("hidden", to: ".phx-client-error #client-error")
        }
        phx-connected={hide("#client-error") |> JS.set_attribute({"hidden", ""})}
        hidden
      >
        Reconnecting to wicket
        <.icon name="hero-arrow-path" class="ml-1 size-3 motion-safe:animate-spin" />
      </.flash>

      <.flash
        id="server-error"
        kind={:error}
        title="Something went wrong"
        phx-disconnected={
          show(".phx-server-error #server-error")
          |> JS.remove_attribute("hidden", to: ".phx-server-error #server-error")
        }
        phx-connected={hide("#server-error") |> JS.set_attribute({"hidden", ""})}
        hidden
      >
        Reconnecting to wicket
        <.icon name="hero-arrow-path" class="ml-1 size-3 motion-safe:animate-spin" />
      </.flash>
    </div>
    """
  end

  @doc """
  Prefix for the document title: the pending-gate count, when there is one.
  """
  def title_prefix(count) when is_integer(count) and count > 0, do: "(#{count}) "
  def title_prefix(_), do: nil
end

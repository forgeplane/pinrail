defmodule WicketWeb.Layouts do
  @moduledoc """
  The root HTML skeleton (`layouts/root.html.heex`) and the app chrome every
  page renders inside: header with navigation, main column, flash messages.
  """
  use WicketWeb, :html

  embed_templates "layouts/*"

  @nav [
    {:inbox, "Inbox", "/", "hero-inbox"},
    {:history, "History", "/history", "hero-clock"},
    {:types, "Gate types", "/types", "hero-squares-2x2"}
  ]

  attr :flash, :map, required: true, doc: "the map of flash messages"
  attr :page, :atom, default: nil, doc: "which nav entry is current"
  attr :pending_count, :integer, default: 0
  attr :repositories, :list, default: []
  slot :inner_block, required: true

  def app(assigns) do
    assigns = assign(assigns, :nav, @nav)

    ~H"""
    <a href="#main-content" class="skip-link">Skip to content</a>
    <aside id="app-sidebar" class="app-sidebar" aria-label="Workspace">
      <.link navigate={~p"/"} class="app-brand" aria-label="wicket home">
        <span class="wicket-mark" aria-hidden="true"></span><span class="sidebar-label">wicket</span>
      </.link>
      <div class="workspace-name sidebar-label">
        <.icon name="hero-code-bracket" class="size-3.5" />Local workspace
      </div>
      <nav class="app-nav" aria-label="Main">
        <.link
          :for={{key, label, path, icon} <- @nav}
          navigate={path}
          aria-label={label}
          title={label}
          aria-current={@page == key && "page"}
          class={[@page == key && "is-active"]}
        >
          <.icon name={icon} class="size-4" />
          <span class="sidebar-label">{label}</span>
          <span :if={key == :inbox} class="nav-count sidebar-label">{@pending_count}</span>
        </.link>
      </nav>
      <section
        :if={@repositories != []}
        class="sidebar-repositories sidebar-label"
        aria-label="Repositories"
      >
        <h2>REPOSITORIES</h2>
        <.link :for={repo <- @repositories} navigate={~p"/?repo=#{repo}"} class="sidebar-repo-link">
          <span class="repo-square"></span>{repo}
        </.link>
      </section>
      <div class="sidebar-bottom">
        <div class="connection-state" title="Connection status">
          <span class="connection-dot"></span><span class="sidebar-label" data-connection-label>Connecting…</span>
        </div>
      </div>
    </aside>
    <div class="app-shell">
      <header class="app-topbar">
        <button
          id="sidebar-toggle"
          type="button"
          data-sidebar-toggle
          class="chrome-button"
          aria-label="Toggle sidebar"
          aria-controls="app-sidebar"
          aria-expanded="true"
          title="Toggle sidebar (⌘/Ctrl+B)"
        >
          <.icon name="hero-bars-3-bottom-left" class="size-4" />
        </button>
        <span class="text-xs text-dim">{case @page do
          :inbox -> "Inbox"
          :history -> "History"
          :types -> "Gate types"
          _ -> "Gate review"
        end}</span>
        <button type="button" data-shortcuts class="topbar-shortcuts">Keyboard shortcuts <kbd>?</kbd></button>
        <button
          id="theme-toggle"
          type="button"
          data-theme-toggle
          class="chrome-button"
          aria-label="Toggle light and dark theme"
          title="Switch theme (T)"
        >
          <.icon name="hero-sun" class="theme-sun size-4" />
          <.icon name="hero-moon" class="theme-moon size-4" />
        </button>
      </header>
      <main
        id="main-content"
        class={["app-main", @page == nil && "gate-main", @page == :inbox && "inbox-main"]}
        tabindex="-1"
      >
        {render_slot(@inner_block)}
      </main>
    </div>
    <dialog id="keyboard-help" class="app-dialog" aria-labelledby="keyboard-title">
      <h2 id="keyboard-title">Keyboard shortcuts</h2>
      <dl class="shortcut-list">
        <dt>Next / previous gate</dt><dd><kbd>J</kbd> <kbd>K</kbd></dd>
        <dt>Open focused gate</dt><dd><kbd>Enter</kbd></dd>
        <dt>Search inbox</dt><dd><kbd>/</kbd></dd>
        <dt>Toggle sidebar</dt><dd><kbd>⌘/Ctrl B</kbd></dd>
        <dt>Switch theme</dt><dd><kbd>T</kbd></dd>
        <dt>Hand over to the agent</dt><dd><kbd>⌘/Ctrl Enter</kbd></dd>
      </dl>
      <button type="button" data-close-dialog class="chrome-button">Close <kbd>Esc</kbd></button>
    </dialog>

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
end

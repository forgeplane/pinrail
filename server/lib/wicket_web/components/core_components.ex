defmodule WicketWeb.CoreComponents do
  @moduledoc """
  Small, Tailwind-only building blocks shared by every page. Styling comes from
  the theme tokens in `assets/css/app.css`; there is no component library.
  """
  use Phoenix.Component

  alias Phoenix.LiveView.JS

  @doc """
  Renders a flash notice for the given kind, if the flash map has one.
  """
  attr :id, :string, doc: "the optional id of flash container"
  attr :flash, :map, default: %{}, doc: "the map of flash messages to display"
  attr :title, :string, default: nil
  attr :kind, :atom, values: [:info, :error], doc: "used for styling and flash lookup"
  attr :rest, :global, doc: "the arbitrary HTML attributes to add to the flash container"

  slot :inner_block, doc: "the optional inner block that renders the flash message"

  def flash(assigns) do
    assigns = assign_new(assigns, :id, fn -> "flash-#{assigns.kind}" end)

    ~H"""
    <div
      :if={msg = render_slot(@inner_block) || Phoenix.Flash.get(@flash, @kind)}
      id={@id}
      phx-click={JS.push("lv:clear-flash", value: %{key: @kind}) |> hide("##{@id}")}
      role="alert"
      class="fixed top-3 right-3 z-50 w-80 max-w-[calc(100vw-1.5rem)]"
      {@rest}
    >
      <div class={[
        "flex items-start gap-3 rounded-md border bg-raised px-3.5 py-3 text-[12.5px] shadow-lg",
        @kind == :info && "border-accent/40 text-text",
        @kind == :error && "border-danger/50 text-text"
      ]}>
        <.icon
          :if={@kind == :info}
          name="hero-information-circle"
          class="size-4 shrink-0 text-accent"
        />
        <.icon
          :if={@kind == :error}
          name="hero-exclamation-circle"
          class="size-4 shrink-0 text-danger"
        />
        <div class="min-w-0 flex-1">
          <p :if={@title} class="font-semibold">{@title}</p>
          <p>{msg}</p>
        </div>
        <button type="button" class="cursor-pointer text-faint hover:text-text" aria-label="close">
          <.icon name="hero-x-mark" class="size-4" />
        </button>
      </div>
    </div>
    """
  end

  @doc """
  Renders a button, or a link styled as one when `href`/`navigate`/`patch` is given.

      <.button phx-click="go" variant="primary">Save</.button>
      <.button navigate={~p"/"}>Back</.button>
  """
  attr :rest, :global, include: ~w(href navigate patch method download name value disabled type)
  attr :class, :any, default: nil
  attr :variant, :string, default: "ghost", values: ~w(primary ghost danger)
  slot :inner_block, required: true

  def button(%{rest: rest} = assigns) do
    base =
      "inline-flex cursor-pointer items-center gap-1.5 rounded-md px-3 py-1.5 text-[12.5px] font-semibold whitespace-nowrap disabled:cursor-not-allowed disabled:opacity-50"

    variants = %{
      "primary" => "bg-[var(--button-bg)] text-white hover:brightness-110",
      "ghost" => "border border-border text-dim hover:border-border-strong hover:text-text",
      "danger" => "border border-danger/45 text-danger hover:bg-danger/10"
    }

    assigns =
      assign(assigns, :class, [base, Map.fetch!(variants, assigns.variant), assigns.class])

    if rest[:href] || rest[:navigate] || rest[:patch] do
      ~H"""
      <.link class={[@class, "hover:no-underline"]} {@rest}>
        {render_slot(@inner_block)}
      </.link>
      """
    else
      ~H"""
      <button class={@class} {@rest}>
        {render_slot(@inner_block)}
      </button>
      """
    end
  end

  @doc """
  Page heading with an optional subtitle and right-aligned actions.
  """
  attr :title, :string, required: true
  slot :subtitle
  slot :actions

  def page_header(assigns) do
    ~H"""
    <header class="page-head">
      <div>
        <h1 class="text-base font-semibold leading-7 text-text">{@title}</h1>
        <p :if={@subtitle != []} class="text-[12.5px] text-dim">{render_slot(@subtitle)}</p>
      </div>
      <div :if={@actions != []} class="flex-none">{render_slot(@actions)}</div>
    </header>
    """
  end

  @doc """
  Centered placeholder for a page with nothing to show.
  """
  attr :id, :string, required: true
  attr :title, :string, default: "Nothing here yet"
  attr :icon, :string, default: "hero-inbox"
  slot :inner_block
  slot :actions

  def empty_state(assigns) do
    ~H"""
    <section id={@id} class="empty-state" aria-labelledby={@id <> "-title"}>
      <div class="empty-emblem"><.icon name={@icon} class="size-7" /></div>
      <h2 id={@id <> "-title"}>{@title}</h2>
      <p>{render_slot(@inner_block)}</p>
      <div :if={@actions != []} class="empty-actions">{render_slot(@actions)}</div>
    </section>
    """
  end

  attr :field, Phoenix.HTML.FormField, required: true
  attr :label, :string, required: true
  attr :type, :string, default: "text"
  attr :class, :string, default: "app-input"
  attr :options, :list, default: []
  attr :rest, :global, include: ~w(placeholder autocomplete maxlength required disabled rows)

  def input(assigns) do
    ~H"""
    <label class="field-label" for={@field.id}>
      <span>{@label}</span>
      <input
        :if={@type not in ["select", "textarea"]}
        id={@field.id}
        name={@field.name}
        value={@field.value}
        type={@type}
        class={@class}
        {@rest}
      />
      <select :if={@type == "select"} id={@field.id} name={@field.name} class={@class} {@rest}>
        {Phoenix.HTML.Form.options_for_select(@options, @field.value)}
      </select>
      <textarea :if={@type == "textarea"} id={@field.id} name={@field.name} class={@class} {@rest}>{@field.value}</textarea>
    </label>
    """
  end

  @doc """
  Renders a [Heroicon](https://heroicons.com) by class name, e.g. `hero-x-mark`.
  Outline by default; `-solid` and `-mini` suffixes select the other styles.
  """
  attr :name, :string, required: true
  attr :class, :any, default: "size-4"

  def icon(%{name: "hero-" <> _} = assigns) do
    ~H"""
    <span class={[@name, @class]} />
    """
  end

  ## JS Commands

  def show(js \\ %JS{}, selector) do
    JS.show(js,
      to: selector,
      time: 200,
      transition:
        {"transition-all ease-out duration-200", "opacity-0 translate-y-1",
         "opacity-100 translate-y-0"}
    )
  end

  def hide(js \\ %JS{}, selector) do
    JS.hide(js,
      to: selector,
      time: 150,
      transition:
        {"transition-all ease-in duration-150", "opacity-100 translate-y-0",
         "opacity-0 translate-y-1"}
    )
  end
end

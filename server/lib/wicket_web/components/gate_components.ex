defmodule WicketWeb.GateComponents do
  @moduledoc """
  Components shared by the inbox, history and gate pages: badges, source
  lines, summary counts and the inbox card.
  """
  use Phoenix.Component
  use WicketWeb, :verified_routes

  import WicketWeb.Format

  alias Wicket.Gate

  attr :status, :atom, required: true

  def status_badge(assigns) do
    ~H"""
    <span class={[
      "inline-flex items-center rounded px-1.5 py-0.5 text-[10.5px] font-semibold uppercase tracking-wide",
      @status == :pending && "bg-accent-bg text-accent",
      @status == :decided && "bg-ok/15 text-ok",
      @status == :withdrawn && "bg-hover text-dim",
      @status == :expired && "bg-danger/10 text-danger"
    ]}>
      {@status}
    </span>
    """
  end

  attr :type, :string, required: true
  attr :version, :integer, default: nil
  attr :class, :any, default: nil

  def type_badge(assigns) do
    ~H"""
    <span class={[
      "inline-flex items-center gap-1 rounded border border-border bg-bg px-1.5 py-0.5 font-mono text-[11px] text-dim",
      @class
    ]}>
      {@type}<span :if={@version} class="text-faint">v{@version}</span>
    </span>
    """
  end

  attr :source, :map, required: true
  attr :class, :any, default: nil
  attr :link, :boolean, default: true, doc: "render source.url as a link; off inside another link"

  def source_line(assigns) do
    ~H"""
    <span class={["inline-flex flex-wrap items-center gap-x-2 text-[12px] text-dim", @class]}>
      <span :if={@source["repo"]}>{@source["repo"]}</span>
      <span :if={@source["workflow"]} class="text-faint">/</span>
      <span :if={@source["workflow"]}>{@source["workflow"]}</span>
      <span :if={@source["ref"]} class="font-mono text-faint">#{@source["ref"]}</span>
      <a
        :if={@link and @source["url"]}
        href={@source["url"]}
        target="_blank"
        rel="noreferrer"
        class="text-accent"
      >
        open ↗
      </a>
    </span>
    """
  end

  attr :summary, :map, default: nil

  def summary_counts(assigns) do
    counts =
      case assigns.summary do
        %{"counts" => counts} when is_list(counts) ->
          for [label, n] <- counts, do: {to_string(label), n}

        _ ->
          []
      end

    assigns =
      assign(assigns, counts: counts, subtitle: assigns.summary && assigns.summary["subtitle"])

    ~H"""
    <span
      :if={@counts != [] or @subtitle}
      class="inline-flex flex-wrap items-center gap-1.5 text-[11.5px]"
    >
      <span
        :for={{label, n} <- @counts}
        class={["rounded px-1.5 py-0.5 font-medium", severity_class(label)]}
      >
        {n} {label}
      </span>
      <span :if={@subtitle} class="text-faint">{@subtitle}</span>
    </span>
    """
  end

  defp severity_class("blocker"), do: "bg-sev-blocker/15 text-sev-blocker"
  defp severity_class("major"), do: "bg-sev-major/15 text-sev-major"
  defp severity_class("minor"), do: "bg-sev-minor/15 text-sev-minor"
  defp severity_class("nit"), do: "bg-sev-nit/15 text-sev-nit"
  defp severity_class(_), do: "bg-hover text-dim"

  attr :gate, Gate, required: true
  attr :now, DateTime, required: true

  def gate_card(assigns) do
    ~H"""
    <.link
      navigate={~p"/gates/#{@gate.id}"}
      id={"gate-#{@gate.id}"}
      class="block rounded-lg border border-border bg-panel px-4 py-3 hover:border-border-strong hover:bg-raised hover:no-underline"
    >
      <div class="flex items-start justify-between gap-4">
        <div class="min-w-0">
          <div class="truncate text-[13.5px] font-semibold text-text">{@gate.title}</div>
          <div class="mt-1 flex flex-wrap items-center gap-2">
            <.type_badge type={@gate.type} />
            <.summary_counts summary={@gate.summary} />
          </div>
        </div>
        <div class="shrink-0 text-right text-[11.5px] text-faint">
          <div title={stamp(@gate.created_at)}>{age(@gate.created_at, @now)}</div>
          <div :if={@gate.requested_by} class="mt-0.5">{@gate.requested_by}</div>
        </div>
      </div>
      <.source_line :if={@gate.source != %{}} source={@gate.source} link={false} class="mt-2" />
    </.link>
    """
  end
end

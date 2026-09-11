defmodule WicketWeb.GateLive do
  @moduledoc """
  One gate: the shell around the plugin's sandboxed iframe.

  The shell owns the header, the source link, the superseded chain, the
  agent-note box and the decision call. The plugin owns everything inside
  the frame. They talk through the `PluginBridge` hook:

    * shell → plugin: `init`, `violations`, `submitted` (as LiveView events
      `gate:init`, `gate:violations`, `gate:submitted` relayed by the hook)
    * plugin → shell: `ready`, `resize`, `draft`, `submit` (the hook pushes
      `submit` here)

  After a decision, or when the gate settles from elsewhere, the plugin is
  re-initialised read-only.

  The iframe carries `data-src`, not `src`: the hook sets `src` once it is
  mounted and listening, so the plugin's `ready` can never be posted before
  the shell can hear it.
  """
  use WicketWeb, :live_view

  alias Wicket.{Gate, GateError, Gates, Types}

  @impl true
  def mount(%{"id" => id}, _session, socket) do
    gate = Gates.get!(id)

    if connected?(socket) do
      Gates.subscribe(id)
      Gates.mark_viewed(id)
    end

    socket =
      socket
      |> assign(
        page: nil,
        agent_note: "",
        note_form: to_form(%{"agent_note" => ""}),
        violations: []
      )
      |> put_gate(gate)
      |> push_init()

    {:ok, socket}
  end

  @impl true
  def handle_event("agent_note", %{"agent_note" => note}, socket) do
    {:noreply, assign(socket, agent_note: note, note_form: to_form(%{"agent_note" => note}))}
  end

  def handle_event("plugin_ready", _, socket), do: {:noreply, socket |> refresh() |> push_init()}

  def handle_event("submit", %{"data" => data} = params, socket) do
    note =
      case Map.get(params, "agent_note", socket.assigns.agent_note) do
        value when is_binary(value) -> value
        _ -> socket.assigns.agent_note
      end

    socket = assign(socket, agent_note: note, note_form: to_form(%{"agent_note" => note}))

    case Gates.decide(socket.assigns.gate.id, data, agent_note: note) do
      {:ok, gate} ->
        socket =
          socket
          |> assign(violations: [])
          |> put_gate(gate)
          |> push_event("gate:submitted", %{decision: Gate.to_map(gate)["decision"]})
          |> put_flash(:info, "Decision recorded")

        {:noreply, socket}

      {:error, %GateError{reason: :invalid, violations: violations}} ->
        errors = Enum.map(violations, &%{path: &1.path, message: &1.message})

        {:noreply,
         socket
         |> assign(violations: violations)
         |> push_event("gate:violations", %{errors: errors})}

      {:error, %GateError{} = error} ->
        {:noreply, socket |> put_flash(:error, error.message) |> refresh()}
    end
  end

  @impl true
  def handle_info({:gate, _event, %Gate{id: id}}, %{assigns: %{gate: %Gate{id: id}}} = socket) do
    {:noreply, refresh(socket)}
  end

  def handle_info(_other, socket), do: {:noreply, socket}

  defp refresh(socket) do
    was_readonly = socket.assigns.readonly
    socket = put_gate(socket, Gates.get!(socket.assigns.gate.id))
    if socket.assigns.readonly and not was_readonly, do: push_init(socket), else: socket
  end

  defp put_gate(socket, %Gate{} = gate) do
    status = Gate.status(gate)

    plugin =
      case Types.fetch(gate.type, gate.type_version) do
        {:ok, plugin} -> plugin
        {:error, _} -> nil
      end

    assign(socket,
      gate: gate,
      status: status,
      readonly: status != :pending,
      plugin: plugin,
      chain: Gates.chain(gate),
      superseded_by: Gates.superseded_by(gate.id)
    )
    |> assign_title(gate.title)
  end

  defp push_init(%{assigns: %{plugin: nil}} = socket), do: socket

  defp push_init(socket) do
    %{gate: gate, plugin: plugin, readonly: readonly} = socket.assigns

    previous =
      case gate.supersedes && Gates.get(gate.supersedes) do
        {:ok, prev} -> Gate.to_map(prev)
        _ -> nil
      end

    push_event(socket, "gate:init", %{
      gate: Gate.to_map(gate),
      previous: previous,
      readonly: readonly,
      min_height: plugin.min_height
    })
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
      <.link navigate={~p"/"} class="flex items-center gap-2 text-xs text-dim"><.icon
        name="hero-arrow-left"
        class="size-3.5"
      />Back to inbox</.link>
      <header class="space-y-2">
        <div class="flex flex-wrap items-center gap-2">
          <.status_badge status={@status} />
          <.type_badge type={@gate.type} version={@gate.type_version} />
          <span class="font-mono text-[11px] text-faint">{@gate.id}</span>
        </div>
        <h1 class="text-base font-semibold leading-7 text-text">{@gate.title}</h1>
        <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-[12px] text-dim">
          <.source_line source={@gate.source} />
          <span :if={@gate.requested_by}>requested by {@gate.requested_by}</span>
          <span title={stamp(@gate.created_at)}>created {age(@gate.created_at)} ago</span>
          <span :if={@gate.expires_at} title={stamp(@gate.expires_at)}>expires {stamp(
            @gate.expires_at
          )}</span>
        </div>
        <div
          :if={@chain != [] or @superseded_by}
          id="gate-chain"
          class="flex flex-wrap items-center gap-x-3 text-[12px] text-dim"
        >
          <span :if={@superseded_by}>
            superseded by
            <.link navigate={~p"/gates/#{@superseded_by.id}"}>{@superseded_by.title}</.link>
          </span>
          <span :if={@chain != []}>
            previous rounds:
            <.link :for={prev <- @chain} navigate={~p"/gates/#{prev.id}"} class="ml-1">{prev.title}</.link>
          </span>
        </div>
      </header>

      <%!-- Everything above the frame lives in one permanent slot: inserting a
      sibling before the iframe would make morphdom move it, and a moved
      iframe reloads. --%>
      <div id="gate-status" class="space-y-4 empty:hidden">
        <div
          :if={@status == :withdrawn}
          id="gate-withdrawn"
          class="rounded-md border border-border bg-panel px-4 py-3 text-[12.5px] text-dim"
        >
          The requester withdrew this gate {age(@gate.withdrawn_at)} ago. Nothing was decided.
        </div>
        <div
          :if={@status == :expired}
          id="gate-expired"
          class="rounded-md border border-danger/40 bg-danger/5 px-4 py-3 text-[12.5px] text-dim"
        >
          This gate expired at {stamp(@gate.expires_at)} without a decision.
        </div>

        <section
          :if={@gate.decision}
          id="gate-decision"
          class="rounded-md border border-ok/40 bg-ok/5 px-4 py-3 text-[12.5px]"
        >
          <div class="text-dim">
            Decided by <span class="text-text">{@gate.decision.decided_by}</span>
            <span title={stamp(@gate.decision.decided_at)}>{age(@gate.decision.decided_at)} ago</span>
          </div>
          <div :if={@gate.agent_note} class="mt-2">
            <div class="text-[11px] font-bold uppercase tracking-[0.08em] text-faint">
              note to the agent
            </div>
            <p class="whitespace-pre-wrap text-text">{@gate.agent_note}</p>
          </div>
          <details class="mt-2">
            <summary class="cursor-pointer text-faint">decision data</summary>
            <pre class="mt-1 overflow-x-auto rounded bg-bg p-2 font-mono text-[11.5px] text-dim">{JSON.encode!(@gate.decision.data)}</pre>
          </details>
        </section>

        <div
          :if={@plugin == nil}
          id="gate-no-plugin"
          class="rounded-md border border-danger/40 bg-danger/5 px-4 py-3 text-[12.5px] text-danger"
        >
          The view for {@gate.type} v{@gate.type_version} is not available, so this gate cannot be rendered.
          No action can be authorized from this unavailable view. Recorded decisions remain preserved.
        </div>
      </div>

      <div
        :if={@plugin}
        id="plugin-frame-wrap"
        phx-update="ignore"
        class="overflow-hidden rounded-lg border border-border bg-panel"
      >
        <iframe
          id="plugin-frame"
          phx-hook="PluginBridge"
          data-gate-id={@gate.id}
          data-min-height={@plugin.min_height}
          sandbox="allow-scripts"
          referrerpolicy="no-referrer"
          data-src={~p"/plugins/#{@gate.type}/#{@gate.type_version}/#{@plugin.entry}"}
          title={@gate.title}
          class="block w-full border-0"
          style={"height: #{@plugin.min_height}px"}
        ></iframe>
      </div>

      <ul
        :if={@violations != []}
        id="gate-violations"
        class="space-y-1 rounded-md border border-danger/40 bg-danger/5 px-4 py-3 text-[12.5px] text-danger"
      >
        <li :for={v <- @violations}>
          <span class="font-mono">{if v.path == "", do: "/", else: v.path}</span>: {v.message}
        </li>
      </ul>

      <.form
        :if={@plugin != nil and not @readonly}
        for={@note_form}
        id="agent-note-form"
        phx-change="agent_note"
        phx-submit="agent_note"
        class="space-y-2"
      >
        <.input
          field={@note_form[:agent_note]}
          label="Note to the agent"
          type="textarea"
          rows="3"
          phx-debounce="200"
          placeholder="Add context for what the agent should do next…"
        />
        <p class="text-xs text-dim">Sent with your decision. ⌘/Ctrl+Enter submits.</p>
      </.form>
    </Layouts.app>
    """
  end
end

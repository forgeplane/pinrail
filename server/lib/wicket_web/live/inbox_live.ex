defmodule WicketWeb.InboxLive do
  @moduledoc """
  Pending gates, newest first, grouped by repo then workflow. Updates live as
  gates are created and decided.
  """
  use WicketWeb, :live_view

  alias Wicket.Gates

  @impl true
  def mount(_params, _session, socket) do
    {:ok, socket |> assign_title("Inbox") |> assign(page: :inbox) |> load()}
  end

  @impl true
  def handle_info({:gate, _event, _gate}, socket), do: {:noreply, load(socket)}

  defp load(socket) do
    groups =
      Gates.list(status: :pending, superseded: false)
      |> Enum.group_by(&(&1.source["repo"] || ""))
      |> Enum.sort_by(fn {repo, _} -> repo end)
      |> Enum.map(fn {repo, gates} ->
        workflows =
          gates
          |> Enum.group_by(&(&1.source["workflow"] || ""))
          |> Enum.sort_by(fn {wf, _} -> wf end)

        {repo, workflows}
      end)

    assign(socket, groups: groups, now: DateTime.utc_now())
  end

  @impl true
  def render(assigns) do
    ~H"""
    <Layouts.app flash={@flash} page={@page}>
      <.page_header title="Inbox">
        <:subtitle>{@pending_count} waiting for a decision</:subtitle>
      </.page_header>

      <.empty_state :if={@groups == []} id="inbox-empty">
        Nothing is waiting for a decision.
      </.empty_state>

      <section :for={{repo, workflows} <- @groups} id={"repo-#{repo}"} class="space-y-3">
        <h2 class="text-[11px] font-bold uppercase tracking-[0.08em] text-faint">
          {if repo == "", do: "no repo", else: repo}
        </h2>
        <div :for={{workflow, gates} <- workflows} class="space-y-2">
          <h3 :if={workflow != ""} class="text-[12px] font-medium text-dim">{workflow}</h3>
          <.gate_card :for={gate <- gates} gate={gate} now={@now} />
        </div>
      </section>
    </Layouts.app>
    """
  end
end

defmodule WicketWeb.HistoryLive do
  @moduledoc """
  Decided, withdrawn and expired gates with the same filters as the API.
  Filters live in the URL so a view can be linked.
  """
  use WicketWeb, :live_view

  alias Wicket.Gates

  @statuses ~w(decided withdrawn expired)
  @filter_keys ~w(status repo workflow ref type)

  @impl true
  def mount(_params, _session, socket) do
    {:ok, socket |> assign_title("History") |> assign(page: :history)}
  end

  @impl true
  def handle_params(params, _uri, socket) do
    filters =
      Map.take(params, @filter_keys) |> Enum.reject(fn {_k, v} -> v in [nil, ""] end) |> Map.new()

    {:noreply, socket |> assign(filters: filters) |> load()}
  end

  @impl true
  def handle_event("filter", params, socket) do
    filters =
      Map.take(params, @filter_keys) |> Enum.reject(fn {_k, v} -> v in [nil, ""] end) |> Map.new()

    {:noreply, push_patch(socket, to: ~p"/history?#{filters}")}
  end

  @impl true
  def handle_info({:gate, _event, _gate}, socket), do: {:noreply, load(socket)}

  defp load(%{assigns: %{filters: f}} = socket) do
    statuses =
      case f["status"] do
        s when s in @statuses -> [String.to_existing_atom(s)]
        _ -> [:decided, :withdrawn, :expired]
      end

    gates =
      Gates.list(
        [
          status: statuses,
          repo: f["repo"],
          workflow: f["workflow"],
          ref: f["ref"],
          type: f["type"]
        ]
        |> Enum.reject(fn {_k, v} -> is_nil(v) end)
      )

    assign(socket, gates: gates, now: DateTime.utc_now())
  end

  @impl true
  def render(assigns) do
    ~H"""
    <Layouts.app flash={@flash} page={@page}>
      <.page_header title="History">
        <:subtitle>{length(@gates)} gates</:subtitle>
      </.page_header>

      <form id="history-filters" phx-change="filter" class="flex flex-wrap items-end gap-2">
        <label class="flex flex-col gap-1 text-[11px] text-faint">
          status
          <select
            name="status"
            class="rounded-md border border-border bg-bg px-2 py-1 text-[12.5px] text-text"
          >
            <option value="">all</option>
            <option
              :for={s <- ~w(decided withdrawn expired)}
              value={s}
              selected={@filters["status"] == s}
            >
              {s}
            </option>
          </select>
        </label>
        <label
          :for={key <- ~w(repo workflow ref type)}
          class="flex flex-col gap-1 text-[11px] text-faint"
        >
          {key}
          <input
            type="text"
            name={key}
            value={@filters[key]}
            phx-debounce="300"
            class="w-32 rounded-md border border-border bg-bg px-2 py-1 text-[12.5px] text-text"
          />
        </label>
        <.button :if={@filters != %{}} navigate={~p"/history"}>clear</.button>
      </form>

      <.empty_state :if={@gates == []} id="history-empty">No gate matches.</.empty_state>

      <div :if={@gates != []} class="overflow-x-auto rounded-lg border border-border">
        <table id="history-table" class="w-full text-[12.5px]">
          <thead class="bg-panel text-left text-[11px] uppercase tracking-wide text-faint">
            <tr>
              <th class="px-3 py-2 font-semibold">gate</th>
              <th class="px-3 py-2 font-semibold">status</th>
              <th class="px-3 py-2 font-semibold">source</th>
              <th class="px-3 py-2 font-semibold">by</th>
              <th class="px-3 py-2 text-right font-semibold">when</th>
            </tr>
          </thead>
          <tbody>
            <tr :for={g <- @gates} id={"row-#{g.id}"} class="border-t border-border hover:bg-raised">
              <td class="px-3 py-2">
                <.link navigate={~p"/gates/#{g.id}"} class="font-medium text-text">{g.title}</.link>
                <.type_badge type={g.type} class="ml-2" />
              </td>
              <td class="px-3 py-2"><.status_badge status={Wicket.Gate.status(g, @now)} /></td>
              <td class="px-3 py-2"><.source_line source={g.source} /></td>
              <td class="px-3 py-2 text-dim">{g.decision && g.decision.decided_by}</td>
              <td class="px-3 py-2 text-right text-faint" title={stamp(settled_at(g))}>
                {age(settled_at(g), @now)}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </Layouts.app>
    """
  end

  defp settled_at(%{decision: %{decided_at: at}}), do: at
  defp settled_at(%{withdrawn_at: %DateTime{} = at}), do: at
  defp settled_at(%{expires_at: at}), do: at
end

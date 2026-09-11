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
    {:ok,
     socket
     |> assign_title("History")
     |> assign(page: :history)
     |> stream_configure(:history_gates, dom_id: &"row-#{&1.id}")}
  end

  @impl true
  def handle_params(params, _uri, socket) do
    filters =
      Map.take(params, @filter_keys) |> Enum.reject(fn {_k, v} -> v in [nil, ""] end) |> Map.new()

    {:noreply, socket |> assign(filters: filters, filter_form: to_form(filters)) |> load()}
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

    socket
    |> assign(gates_count: length(gates), gates_empty?: gates == [], now: DateTime.utc_now())
    |> stream(:history_gates, gates, reset: true)
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
      <.page_header title="History" />

      <.form
        for={@filter_form}
        id="history-filters"
        phx-change="filter"
        class="history-filters"
      >
        <.input
          field={@filter_form[:status]}
          label="Status"
          type="select"
          options={[
            {"All statuses", ""},
            {"Decided", "decided"},
            {"Withdrawn", "withdrawn"},
            {"Expired", "expired"}
          ]}
        />
        <.input
          :for={key <- ~w(repo workflow ref type)}
          field={@filter_form[key]}
          label={String.capitalize(key)}
          placeholder={String.capitalize(key) <> "…"}
          phx-debounce="200"
          class="app-input w-32"
        />
        <.button :if={@filters != %{}} navigate={~p"/history"}>Clear filters</.button>
      </.form>

      <.empty_state
        :if={@gates_empty?}
        id="history-empty"
        title={if @filters == %{}, do: "Your decisions belong here", else: "No matching decisions"}
        icon="hero-clock"
      >
        {if @filters == %{},
          do:
            "Every decision keeps its context. Completed, withdrawn, and expired gates will appear here.",
          else: "Nothing matches these filters. Clear them to explore the full record."}
        <:actions>
          <.button :if={@filters != %{}} navigate={~p"/history"}>Clear filters</.button><.button
            :if={@filters == %{}}
            navigate={~p"/"}
          >Back to inbox</.button>
        </:actions>
      </.empty_state>

      <div :if={not @gates_empty?} class="history-table-wrap">
        <table id="history-table" class="history-table">
          <thead class="history-table-head">
            <tr>
              <th>Gate / Requester</th><th>Outcome</th><th>Repository</th><th>Recorded</th><th>
                <span class="sr-only">Open gate</span>
              </th>
            </tr>
          </thead>
          <tbody id="history-rows" phx-update="stream">
            <tr
              :for={{dom_id, g} <- @streams.history_gates}
              id={dom_id}
              class="border-t border-border hover:bg-raised"
            >
              <td>
                <.link navigate={~p"/gates/#{g.id}"} class="history-title">{g.title}</.link><small>{g.requested_by}<span> · </span>{g.type}</small>
              </td>
              <td><.status_badge status={Wicket.Gate.status(g, @now)} /></td>
              <td>{g.source["repo"]}<small class="font-mono">{g.source["ref"]}</small></td>
              <td title={stamp(settled_at(g))}>
                {stamp(settled_at(g))}<small>{g.decision && "Decided by #{g.decision.decided_by}"}</small>
              </td>
              <td>
                <.link navigate={~p"/gates/#{g.id}"} aria-label={"Open #{g.title}"}><.icon
                  name="hero-arrow-up-right"
                  class="size-3.5"
                /></.link>
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

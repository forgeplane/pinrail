defmodule WicketWeb.InboxLive do
  @moduledoc "Pending gates in the approved compact inbox, with linkable filters."
  use WicketWeb, :live_view
  alias Wicket.Gates

  @impl true
  def mount(_params, _session, socket) do
    {:ok,
     socket
     |> assign_title("Inbox")
     |> assign(page: :inbox, query: "", filters: %{}, search_form: to_form(%{}, as: :inbox))
     |> stream_configure(:repos, dom_id: & &1.dom_id)
     |> load()}
  end

  @impl true
  def handle_info({:gate, _event, _gate}, socket), do: {:noreply, load(socket)}

  @impl true
  def handle_params(params, _uri, socket) do
    filters = clean_filters(params)

    {:noreply,
     socket
     |> assign(
       query: filters["q"] || "",
       filters: filters,
       search_form: to_form(filters, as: :inbox)
     )
     |> load()}
  end

  @impl true
  def handle_event("search", %{"inbox" => params}, socket) do
    filters = socket.assigns.filters |> Map.merge(params) |> clean_filters()
    {:noreply, push_patch(socket, to: ~p"/?#{filters}")}
  end

  defp clean_filters(params) do
    params
    |> Map.take(~w(q repo type rounds))
    |> Enum.reject(fn {_, value} -> value in [nil, ""] end)
    |> Map.new()
  end

  # A repo name is free text from the requester; keep the id readable but
  # make sure it is a valid one.
  defp dom_id(""), do: "repo-none"
  defp dom_id(repo), do: "repo-" <> String.replace(repo, ~r/[^A-Za-z0-9_-]/, "-")

  defp load(socket) do
    all = Gates.list(status: :pending, superseded: false)
    f = socket.assigns.filters

    groups =
      all
      |> Enum.filter(fn gate ->
        text =
          Enum.join(
            [
              gate.title,
              gate.type,
              gate.requested_by || "",
              gate.source["repo"] || "",
              gate.source["workflow"] || "",
              gate.source["ref"] || ""
            ],
            " "
          )

        String.contains?(String.downcase(text), String.downcase(socket.assigns.query)) &&
          (is_nil(f["repo"]) || gate.source["repo"] == f["repo"]) &&
          (is_nil(f["type"]) || gate.type == f["type"]) &&
          (f["rounds"] != "new" || not is_nil(gate.supersedes))
      end)
      |> Enum.group_by(&(&1.source["repo"] || ""))
      |> Enum.sort_by(fn {repo, _} -> repo end)
      |> Enum.map(fn {repo, gates} ->
        %{
          id: repo,
          dom_id: dom_id(repo),
          repo: repo,
          count: length(gates),
          workflows:
            gates |> Enum.group_by(&(&1.source["workflow"] || "")) |> Enum.sort_by(&elem(&1, 0))
        }
      end)

    socket
    |> assign(
      empty?: groups == [],
      now: DateTime.utc_now(),
      new_count: Enum.count(all, & &1.supersedes),
      type_options: all |> Enum.map(& &1.type) |> Enum.uniq() |> Enum.sort()
    )
    |> stream(:repos, groups, reset: true)
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
      <header class="inbox-head">
        <div class="heading-inline">
          <h1>Inbox</h1><span class="inbox-total">{@pending_count}</span>
        </div>
        <.form for={@search_form} id="inbox-search-form" phx-change="search" class="inbox-controls">
          <div class="search-wrap">
            <.input
              field={@search_form[:q]}
              label="Search inbox"
              type="search"
              placeholder="Search inbox…"
              phx-debounce="150"
            /><kbd>/</kbd>
          </div>
          <.input
            field={@search_form[:repo]}
            label="Repository"
            type="select"
            options={[{"All repositories", ""} | Enum.map(@repositories, &{&1, &1})]}
          />
          <.input
            field={@search_form[:type]}
            label="Gate type"
            type="select"
            options={[{"All types", ""} | Enum.map(@type_options, &{&1, &1})]}
          />
        </.form>
      </header>
      <nav class="list-tabs" aria-label="Inbox views">
        <.link
          patch={~p"/?#{Map.delete(@filters, "rounds")}"}
          class={[@filters["rounds"] != "new" && "active"]}
        >Pending <span>{@pending_count}</span></.link>
        <.link
          patch={~p"/?#{Map.put(@filters, "rounds", "new")}"}
          class={[@filters["rounds"] == "new" && "active"]}
        >New rounds <span>{@new_count}</span></.link>
        <span class="list-sort">Newest first</span>
      </nav>
      <.empty_state
        :if={@empty?}
        id="inbox-empty"
        title={if @filters == %{}, do: "All caught up", else: "No matching requests"}
        icon={if @filters == %{}, do: "hero-check", else: "hero-magnifying-glass"}
      >
        {if @filters == %{},
          do: "New requests will appear here when an agent needs you.",
          else: "Try a different search or clear your filters to see everything waiting."}
        <:actions>
          <.button :if={@filters == %{}} navigate={~p"/history"}>View history</.button><.button
            :if={@filters != %{}}
            patch={~p"/"}
          >Clear filters</.button>
        </:actions>
      </.empty_state>
      <div id="inbox-repos" phx-update="stream">
        <details :for={{dom_id, group} <- @streams.repos} id={dom_id} class="inbox-repo" open>
          <summary>
            <.icon name="hero-chevron-right" class="repo-chevron size-3" /><strong>{if group.repo ==
                                                                                         "",
                                                                                       do:
                                                                                         "No repository",
                                                                                       else:
                                                                                         group.repo}</strong><span>{group.count}</span>
          </summary>
          <div :for={{workflow, gates} <- group.workflows}>
            <h3 class="sr-only">{workflow}</h3>
            <.gate_card :for={gate <- gates} gate={gate} now={@now} />
          </div>
        </details>
      </div>
      <footer class="inbox-footer">
        <span><kbd>J</kbd> <kbd>K</kbd> move · <kbd>↵</kbd> open · <kbd>/</kbd> search</span>
      </footer>
    </Layouts.app>
    """
  end
end

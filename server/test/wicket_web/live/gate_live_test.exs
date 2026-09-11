defmodule WicketWeb.GateLiveTest do
  use WicketWeb.ConnCase
  import Wicket.GatesCase

  alias Wicket.Gates

  setup do
    reset!()
    :ok
  end

  test "renders the shell around the plugin iframe and pushes init", %{conn: conn} do
    gate =
      create_gate!(%{
        title: "MR !42",
        source: %{repo: "acme", workflow: "review", ref: "42", url: "https://example.com/mr/42"},
        requested_by: "agent"
      })

    {:ok, view, html} = live(conn, ~p"/gates/#{gate.id}")

    assert html =~ "MR !42 · wicket"
    assert has_element?(view, "h1", "MR !42")
    assert has_element?(view, "a[href='https://example.com/mr/42']")

    assert has_element?(
             view,
             "#plugin-frame[sandbox='allow-scripts'][data-src='/plugins/list/1/index.html'][data-gate-id='#{gate.id}']"
           )

    assert has_element?(view, "#agent-note-form")
    refute has_element?(view, "#gate-decision")

    # The hook sets src once mounted; a server-rendered src lets the plugin's
    # "ready" fire before the shell listens.
    refute has_element?(view, "#plugin-frame[src]")

    # Everything above the frame renders inside one permanent slot; a sibling
    # inserted before the iframe would make morphdom move it, and a moved
    # iframe reloads.
    assert slot_precedes_frame?(html)

    assert_push_event(view, "gate:init", %{
      gate: %{"id" => id, "payload" => %{"groups" => _}},
      readonly: false,
      previous: nil,
      min_height: 400
    })

    assert id == gate.id
    assert Enum.map(Gates.events(gate.id), & &1["event"]) == ["created", "viewed"]
  end

  test "submit records the decision with the agent note and flips read-only", %{conn: conn} do
    gate = create_gate!()
    {:ok, view, _html} = live(conn, ~p"/gates/#{gate.id}")

    view |> element("#agent-note-form") |> render_change(%{"agent_note" => "  ship it  "})
    render_hook(view, "submit", %{"data" => list_decision()})

    assert_push_event(view, "gate:submitted", %{
      decision: %{"decided_by" => "tester", "data" => %{"undecided" => [2]}}
    })

    assert has_element?(view, "#gate-status > #gate-decision", "tester")
    assert has_element?(view, "#gate-decision", "ship it")
    assert slot_precedes_frame?(render(view))
    refute has_element?(view, "#agent-note-form")
    assert render(view) =~ "Decision recorded"
    assert Gates.get!(gate.id).agent_note == "ship it"
  end

  test "an invalid submit relays violations and keeps the gate pending", %{conn: conn} do
    gate = create_gate!()
    {:ok, view, _html} = live(conn, ~p"/gates/#{gate.id}")

    render_hook(view, "submit", %{
      "data" => %{"decisions" => [%{"id" => 1, "action" => "maybe"}], "undecided" => []}
    })

    assert_push_event(view, "gate:violations", %{errors: [%{path: "/decisions/0/action"}]})
    assert has_element?(view, "#gate-violations", "/decisions/0/action")
    assert has_element?(view, "#agent-note-form")
    assert Wicket.Gate.status(Gates.get!(gate.id)) == :pending
  end

  test "a decided gate renders read-only with the decision", %{conn: conn} do
    gate = create_gate!()
    {:ok, _} = Gates.decide(gate.id, list_decision(), agent_note: "n")
    {:ok, view, _html} = live(conn, ~p"/gates/#{gate.id}")

    assert has_element?(view, "#gate-decision", "n")
    refute has_element?(view, "#agent-note-form")
    assert_push_event(view, "gate:init", %{readonly: true})
  end

  test "settling from elsewhere re-inits the plugin read-only", %{conn: conn} do
    gate = create_gate!()
    {:ok, view, _html} = live(conn, ~p"/gates/#{gate.id}")
    assert_push_event(view, "gate:init", %{readonly: false})

    {:ok, _} = Gates.withdraw(gate.id)
    assert has_element?(view, "#gate-withdrawn")
    refute has_element?(view, "#agent-note-form")
    assert_push_event(view, "gate:init", %{readonly: true})

    render_hook(view, "submit", %{"data" => list_decision()})
    assert render(view) =~ "no longer pending"
  end

  test "shows the chain and the previous round in init", %{conn: conn} do
    r1 = create_gate!(%{title: "Round 1"})
    r2 = create_gate!(%{title: "Round 2", supersedes: r1.id})
    {:ok, view, _html} = live(conn, ~p"/gates/#{r2.id}")
    assert has_element?(view, "#gate-chain a[href='/gates/#{r1.id}']", "Round 1")
    assert_push_event(view, "gate:init", %{previous: %{"id" => prev_id}})
    assert prev_id == r1.id

    {:ok, view, _html} = live(conn, ~p"/gates/#{r1.id}")
    assert has_element?(view, "#gate-chain a[href='/gates/#{r2.id}']", "Round 2")
  end

  test "a gate whose plugin version is gone still shows its data", %{conn: conn} do
    gate = create_gate!()
    File.rm_rf!(Wicket.Types.snapshot_dir("list", 1))
    {:ok, 1} = use_plugin_dirs([fixture_dir("dup")])

    {:ok, view, _html} = live(conn, ~p"/gates/#{gate.id}")
    assert has_element?(view, "#gate-no-plugin")
    refute has_element?(view, "#plugin-frame")
    refute_push_event(view, "gate:init", %{})
  end

  test "404 for an unknown gate", %{conn: conn} do
    assert_raise Wicket.GateError, fn -> live(conn, ~p"/gates/g_nope") end
  end

  defp slot_precedes_frame?(html) do
    {slot, _} = :binary.match(html, ~s(id="gate-status"))
    {frame, _} = :binary.match(html, ~s(id="plugin-frame-wrap"))
    slot < frame
  end
end

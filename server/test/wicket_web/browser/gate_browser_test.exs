defmodule WicketWeb.Browser.GateTest do
  @moduledoc """
  The gate page in a real browser: the hook, the sandboxed iframe, the
  postMessage bridge and the built-in list view, which the LiveView tests
  cannot see.
  """
  use PhoenixTest.Playwright.Case, async: false

  import Wicket.BrowserHelpers
  import Wicket.GatesCase

  alias Wicket.{Gate, Gates}

  @moduletag :playwright

  setup do
    reset!()
  end

  test "accept, reject with a reason, add a note, submit", %{conn: conn} do
    gate =
      create_gate!(%{
        title: "MR !42",
        payload: Map.put(list_payload(), "allow_additions", true)
      })

    conn
    |> visit("/gates/#{gate.id}")
    |> assert_has("h1", text: "MR !42")
    |> within_plugin(fn frame ->
      frame
      |> assert_has(".item", text: "#1 one")
      |> assert_has(".item", text: "#2 two")
      |> click_button("[data-id='1'] button", "Accept")
      |> click_button("[data-id='2'] button", "Reject")
      |> fill_in("note for item 2", with: "not worth a comment")
      |> click_button("+ add a note of your own")
      |> fill_in("addition 1", with: "please also check the migration")
      |> assert_has(".footer", text: "1 accepted · 1 rejected · 0 undecided")
      |> click_button("Submit decisions")
    end)
    |> assert_has("#gate-decision", text: "tester")
    |> refute_has("#agent-note-form")
    |> within_plugin(fn frame ->
      frame
      |> assert_has(".done", text: "1 accepted")
      |> assert_has("[data-id='2'] .note-ro", text: "not worth a comment")
      |> refute_has("button", text: "Accept")
    end)

    assert %Gate{decision: %{data: data}} = Gates.get!(gate.id)

    assert data == %{
             "decisions" => [
               %{"id" => 1, "action" => "accept"},
               %{"id" => 2, "action" => "reject", "note" => "not worth a comment"}
             ],
             "undecided" => [],
             "additions" => [%{"body" => "please also check the migration"}]
           }
  end

  test "submitting with undecided items asks first and reports them honestly", %{conn: conn} do
    gate = create_gate!()

    conn
    |> visit("/gates/#{gate.id}")
    |> within_plugin(fn frame ->
      frame
      |> click_button("[data-id='1'] button", "Accept")
      |> click_button("Submit decisions")
      |> assert_has(".footer", text: "1 left undecided")
      |> click_button("Submit anyway")
    end)
    |> assert_has("#gate-decision")

    assert %Gate{decision: %{data: %{"undecided" => [2]}}} = Gates.get!(gate.id)
  end

  test "a draft survives a reload", %{conn: conn} do
    gate = create_gate!()

    conn
    |> visit("/gates/#{gate.id}")
    |> within_plugin(fn frame ->
      frame
      |> click_button("[data-id='1'] button", "Accept")
      |> fill_in("note for item 1", with: "mention the COALESCE")
      |> assert_has("[data-id='1'].accepted")
    end)
    |> wait_for_draft(gate.id, "COALESCE")
    |> reload_page()
    |> within_plugin(fn frame ->
      frame
      |> assert_has("[data-id='1'].accepted")
      |> assert_has("[data-id='1'] textarea", text: "mention the COALESCE")
      |> assert_has(".footer", text: "1 accepted · 0 rejected · 1 undecided")
    end)
  end

  # Typing is debounced before it is posted as a draft; wait for it to land.
  defp wait_for_draft(conn, gate_id, needle) do
    evaluate(conn, """
    new Promise((resolve) => {
      const key = "wicket:draft:#{gate_id}";
      const tick = () => (sessionStorage.getItem(key) || "").includes(#{JSON.encode!(needle)}) ? resolve(true) : setTimeout(tick, 25);
      tick();
    })
    """)
  end

  test "an invalid submit is refused and the violations reach the frame", %{conn: conn} do
    {:ok, 2} = use_plugin_dirs([builtin_dir(), fixture_dir("misbehaving")])
    gate = create_gate!(%{type: "misbehaving", payload: %{}})

    conn
    |> visit("/gates/#{gate.id}")
    |> within_plugin(&click_button(&1, "Submit garbage"))
    |> assert_has("#gate-violations", text: "/ok")
    |> assert_has("#gate-violations", text: "/extra")
    |> within_plugin(&assert_has(&1, "#errors", text: "/ok: value is not of type boolean"))
    |> assert_has("#agent-note-form")

    assert Gate.status(Gates.get!(gate.id)) == :pending
  end

  test "the agent note travels with a keyboard-collected decision", %{conn: conn} do
    gate = create_gate!()

    conn
    |> visit("/gates/#{gate.id}")
    |> within_plugin(&click_button(&1, "accept all undecided"))
    |> fill_in("Note to the agent", with: "next round: tickets only")
    |> press("body", "Control+Enter")
    |> assert_has("#gate-decision", text: "next round: tickets only")

    assert Gates.get!(gate.id).agent_note == "next round: tickets only"
  end

  test "a gate settled elsewhere flips the open page read-only", %{conn: conn} do
    gate = create_gate!()

    conn =
      conn
      |> visit("/gates/#{gate.id}")
      |> within_plugin(&assert_has(&1, "button", text: "Submit decisions"))

    {:ok, _} = Gates.withdraw(gate.id)

    conn
    |> assert_has("#gate-withdrawn")
    |> refute_has("#agent-note-form")
    |> within_plugin(fn frame ->
      frame
      |> assert_has(".done", text: "Closed without a decision (withdrawn)")
      |> refute_has("button", text: "Submit decisions")
    end)
  end

  test "the previous round's verdicts show on a superseding gate", %{conn: conn} do
    r1 = create_gate!(%{title: "Round 1"})

    {:ok, _} =
      Gates.decide(r1.id, %{
        "decisions" => [%{"id" => 1, "action" => "reject", "note" => "dont nitpick"}],
        "undecided" => [2]
      })

    r2 = create_gate!(%{title: "Round 2", supersedes: r1.id})

    conn
    |> visit("/gates/#{r2.id}")
    |> assert_has("#gate-chain", text: "Round 1")
    |> within_plugin(fn frame ->
      frame
      |> assert_has("[data-id='1'] .previous", text: "reject: dont nitpick")
      |> assert_has("[data-id='2'] .previous", text: "undecided")
    end)
  end

  test "the frame is not reloaded when the decision panel appears", %{conn: conn} do
    gate = create_gate!()

    conn
    |> visit("/gates/#{gate.id}")
    |> within_plugin(&assert_has(&1, ".item", text: "#1 one"))
    |> evaluate(
      "window.__loads = 0; document.getElementById('plugin-frame').addEventListener('load', () => window.__loads++)"
    )
    |> within_plugin(fn frame ->
      frame |> click_button("accept all undecided") |> click_button("Submit decisions")
    end)
    |> assert_has("#gate-decision")
    |> evaluate("window.__loads", &assert(&1 == 0))
  end
end

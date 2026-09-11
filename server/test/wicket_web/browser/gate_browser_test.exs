defmodule WicketWeb.Browser.GateTest do
  @moduledoc """
  The gate page in a real browser: the hook, the sandboxed iframe and the
  postMessage bridge, which the LiveView tests cannot see.
  """
  use PhoenixTest.Playwright.Case, async: false

  import Wicket.BrowserHelpers
  import Wicket.GatesCase

  alias Wicket.{Gate, Gates}

  @moduletag :playwright

  setup do
    reset!()
  end

  test "the plugin renders the payload and decides from inside the frame", %{conn: conn} do
    gate = create_gate!(%{title: "MR !42"})

    conn
    |> visit("/gates/#{gate.id}")
    |> assert_has("h1", text: "MR !42")
    |> within_plugin(fn frame ->
      frame
      |> assert_has(".item", text: "#1 one")
      |> assert_has(".item", text: "#2 two")
      |> click_button("Submit (leave everything undecided)")
    end)
    |> assert_has("#gate-decision", text: "tester")
    |> refute_has("#agent-note-form")
    |> within_plugin(&assert_has(&1, "p", text: "Read-only"))

    assert %Gate{decision: %{data: %{"undecided" => [1, 2], "decisions" => []}}} =
             Gates.get!(gate.id)
  end

  test "an invalid submit is refused and the violations reach the frame", %{conn: conn} do
    gate = create_gate!()

    conn
    |> visit("/gates/#{gate.id}")
    |> within_plugin(&click_button(&1, "Submit something invalid"))
    |> assert_has("#gate-violations", text: "/decisions/0/action")
    |> within_plugin(&assert_has(&1, "#errors", text: "/decisions/0/action"))
    |> assert_has("#agent-note-form")

    assert Gate.status(Gates.get!(gate.id)) == :pending
  end

  test "the agent note travels with a keyboard-collected decision", %{conn: conn} do
    gate = create_gate!()

    conn
    |> visit("/gates/#{gate.id}")
    |> within_plugin(&assert_has(&1, ".item", text: "#1 one"))
    |> fill_in("note to the agent", with: "next round: tickets only")
    |> press("body", "Control+Enter")
    |> assert_has("#gate-decision", text: "next round: tickets only")

    assert Gates.get!(gate.id).agent_note == "next round: tickets only"
  end

  test "a gate settled elsewhere flips the open page read-only", %{conn: conn} do
    gate = create_gate!()

    conn =
      conn
      |> visit("/gates/#{gate.id}")
      |> within_plugin(&assert_has(&1, "button", text: "Submit"))

    {:ok, _} = Gates.withdraw(gate.id)

    conn
    |> assert_has("#gate-withdrawn")
    |> refute_has("#agent-note-form")
    |> within_plugin(&assert_has(&1, "p", text: "Read-only"))
  end

  test "the frame is not reloaded when the decision panel appears", %{conn: conn} do
    gate = create_gate!()

    conn
    |> visit("/gates/#{gate.id}")
    |> within_plugin(&assert_has(&1, ".item", text: "#1 one"))
    |> evaluate(
      "window.__loads = 0; document.getElementById('plugin-frame').addEventListener('load', () => window.__loads++)"
    )
    |> within_plugin(&click_button(&1, "Submit (leave everything undecided)"))
    |> assert_has("#gate-decision")
    |> evaluate("window.__loads", &assert(&1 == 0))
  end
end

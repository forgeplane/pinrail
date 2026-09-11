defmodule WicketWeb.InboxLiveTest do
  use WicketWeb.ConnCase
  import Wicket.GatesCase

  alias Wicket.Gates

  setup do
    reset!()
    :ok
  end

  test "empty inbox", %{conn: conn} do
    {:ok, view, html} = live(conn, ~p"/")
    assert html =~ "Inbox · wicket"
    assert has_element?(view, "nav a[aria-current=page]", "Inbox")
    assert has_element?(view, "#inbox-empty")
  end

  test "groups pending gates by repo and workflow, newest of a chain only", %{conn: conn} do
    a =
      create_gate!(%{
        title: "Alpha",
        source: %{repo: "acme", workflow: "review", ref: "1"},
        summary: %{"counts" => [["major", 2]], "subtitle" => "3 new"}
      })

    b = create_gate!(%{title: "Beta", source: %{repo: "acme", workflow: "triage"}})
    c = create_gate!(%{title: "Gamma", source: %{repo: "zeta", workflow: "review"}})
    old = create_gate!(%{title: "Old round"})
    _new = create_gate!(%{title: "New round", supersedes: old.id})
    decided = create_gate!(%{title: "Done"})
    {:ok, _} = Gates.decide(decided.id, list_decision())

    {:ok, view, html} = live(conn, ~p"/")
    assert html =~ "(4) Inbox · wicket"
    assert has_element?(view, "#repo-acme h3", "review")
    assert has_element?(view, "#repo-acme h3", "triage")
    assert has_element?(view, "#gate-#{a.id}", "Alpha")
    assert has_element?(view, "#gate-#{a.id}", "2 major")
    assert has_element?(view, "#gate-#{a.id}", "3 new")
    assert has_element?(view, "#gate-#{b.id}", "Beta")
    assert has_element?(view, "#repo-zeta #gate-#{c.id}", "Gamma")
    refute has_element?(view, "#gate-#{old.id}")
    refute has_element?(view, "#gate-#{decided.id}")

    # a card is a link; a link inside it is invalid HTML and breaks the card
    refute has_element?(view, "a a")
    assert has_element?(view, "#gate-#{a.id}", "acme")
  end

  test "updates live on create and decide", %{conn: conn} do
    {:ok, view, _html} = live(conn, ~p"/")
    assert has_element?(view, "#inbox-empty")

    gate = create_gate!(%{title: "Fresh"})
    assert has_element?(view, "#gate-#{gate.id}", "Fresh")
    assert page_title(view) == "(1) Inbox"

    {:ok, _} = Gates.decide(gate.id, list_decision())
    refute has_element?(view, "#gate-#{gate.id}")
    assert has_element?(view, "#inbox-empty")
  end

  test "card navigates to the gate", %{conn: conn} do
    gate = create_gate!()
    {:ok, view, _html} = live(conn, ~p"/")
    view |> element("#gate-#{gate.id}") |> render_click()
    assert_redirect(view, ~p"/gates/#{gate.id}")
  end
end

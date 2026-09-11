defmodule WicketWeb.HistoryLiveTest do
  use WicketWeb.ConnCase
  import Wicket.GatesCase

  alias Wicket.Gates

  setup do
    reset!()
    :ok
  end

  test "lists settled gates with filters in the url", %{conn: conn} do
    pending = create_gate!(%{title: "Pending"})
    decided = create_gate!(%{title: "Decided", source: %{repo: "acme"}})
    withdrawn = create_gate!(%{title: "Withdrawn", source: %{repo: "zeta"}})
    {:ok, _} = Gates.decide(decided.id, list_decision())
    {:ok, _} = Gates.withdraw(withdrawn.id)

    {:ok, view, _html} = live(conn, ~p"/history")
    assert has_element?(view, "#row-#{decided.id}", "tester")
    assert has_element?(view, "#row-#{withdrawn.id}", "withdrawn")
    refute has_element?(view, "#row-#{pending.id}")

    view |> element("#history-filters") |> render_change(%{"status" => "decided", "repo" => ""})
    assert_patch(view, ~p"/history?status=decided")
    assert has_element?(view, "#row-#{decided.id}")
    refute has_element?(view, "#row-#{withdrawn.id}")

    {:ok, view, _html} = live(conn, ~p"/history?repo=zeta")
    assert has_element?(view, "#row-#{withdrawn.id}")
    refute has_element?(view, "#row-#{decided.id}")

    {:ok, view, _html} = live(conn, ~p"/history?repo=nope")
    assert has_element?(view, "#history-empty")
  end

  test "updates live when a gate settles", %{conn: conn} do
    gate = create_gate!()
    {:ok, view, _html} = live(conn, ~p"/history")
    assert has_element?(view, "#history-empty")
    {:ok, _} = Gates.decide(gate.id, list_decision())
    assert has_element?(view, "#row-#{gate.id}")
  end
end

defmodule WicketWeb.PagesTest do
  use WicketWeb.ConnCase, async: true

  test "inbox renders with the nav marking it current", %{conn: conn} do
    {:ok, view, html} = live(conn, ~p"/")

    assert html =~ "<title"
    assert html =~ "Inbox · wicket"
    assert has_element?(view, "nav a[aria-current=page]", "Inbox")
    assert has_element?(view, "#inbox-empty")
  end

  test "history renders", %{conn: conn} do
    {:ok, view, _html} = live(conn, ~p"/history")
    assert has_element?(view, "nav a[aria-current=page]", "History")
    assert has_element?(view, "#history-empty")
  end

  test "types renders", %{conn: conn} do
    {:ok, view, _html} = live(conn, ~p"/types")
    assert has_element?(view, "nav a[aria-current=page]", "Types")
    assert has_element?(view, "#types-empty")
  end

  test "navigating between pages keeps the shell", %{conn: conn} do
    {:ok, view, _html} = live(conn, ~p"/")

    view
    |> element("nav a", "History")
    |> render_click()

    assert_redirect(view, ~p"/history")
  end
end

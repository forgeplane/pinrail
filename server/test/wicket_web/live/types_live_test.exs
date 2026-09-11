defmodule WicketWeb.TypesLiveTest do
  use WicketWeb.ConnCase
  import Wicket.GatesCase

  setup do
    reset!()
    :ok
  end

  @tag :capture_log
  test "lists types, dirs, and broken plugins; reload button", %{conn: conn} do
    {:ok, 3} = use_plugin_dirs([builtin_dir(), fixture_dir("broken")])
    {:ok, view, _html} = live(conn, ~p"/types")

    assert has_element?(view, "#type-list", "List")
    assert has_element?(view, "#type-list a[href='/plugins/list/1/index.html']")
    assert has_element?(view, "#type-noentry", "broken")
    assert has_element?(view, "#type-noentry", "entry index.html not found")
    assert has_element?(view, "#type-dirs li", fixture_dir("broken"))

    view |> element("button", "reload") |> render_click()
    assert render(view) =~ "Reloaded 3 types"
  end
end

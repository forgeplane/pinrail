defmodule WicketWeb.Browser.AppearanceTest do
  @moduledoc """
  The saved theme and sidebar are on the document before it is painted, so a
  page never opens in one appearance and jumps to another.

  Each test reads what the document looked like in its first animation frame,
  which runs before the browser paints it. Anything applied later, by the
  deferred bundle for instance, lands after that and shows up here as the
  default.
  """
  use PhoenixTest.Playwright.Case, async: false

  import Wicket.GatesCase

  alias PlaywrightEx.BrowserContext

  @moduletag :playwright
  # The sidebar's default follows the viewport, so pin one wide enough to open it.
  @moduletag browser_context_opts: [viewport: %{width: 1280, height: 720}]

  @record """
  window.__painted = null
  requestAnimationFrame(() => {
    const root = document.documentElement
    window.__painted ??= {theme: root.dataset.theme, sidebar: root.dataset.sidebar}
  })
  """

  setup %{conn: conn} do
    reset!()

    {:ok, _} =
      BrowserContext.add_init_script(conn.context_id, source: @record, timeout: :timer.seconds(5))

    :ok
  end

  test "a saved light theme is on the page in its first frame", %{conn: conn} do
    conn
    |> save(%{"wicket:theme" => "light"})
    |> visit("/history")
    |> assert_painted(%{"theme" => "light", "sidebar" => "expanded"})
  end

  test "a saved collapsed sidebar is on the page in its first frame", %{conn: conn} do
    conn
    |> save(%{"wicket:sidebar" => "collapsed"})
    |> visit("/history")
    |> assert_painted(%{"theme" => "dark", "sidebar" => "collapsed"})
  end

  test "with nothing saved the page opens dark with the sidebar out", %{conn: conn} do
    conn
    |> visit("/")
    |> assert_painted(%{"theme" => "dark", "sidebar" => "expanded"})
  end

  test "the toggles write what the next load reads back", %{conn: conn} do
    conn
    |> visit("/")
    |> click("#theme-toggle")
    |> click("#sidebar-toggle")
    |> assert_now(%{"theme" => "light", "sidebar" => "collapsed"})
    |> visit("/history")
    |> assert_painted(%{"theme" => "light", "sidebar" => "collapsed"})
  end

  defp save(conn, prefs) do
    conn
    |> visit("/")
    |> evaluate("prefs => prefs.forEach(([k, v]) => localStorage.setItem(k, v))",
      is_function: true,
      arg: Enum.map(prefs, &Tuple.to_list/1)
    )
  end

  defp assert_now(conn, expected) do
    evaluate(conn, "({...document.documentElement.dataset})", fn seen ->
      assert Map.take(seen, ["theme", "sidebar"]) == expected
    end)
  end

  defp assert_painted(conn, expected),
    do: evaluate(conn, "window.__painted", &assert(&1 == expected))
end

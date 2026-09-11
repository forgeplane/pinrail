defmodule WicketWeb.Browser.AppearanceTest do
  @moduledoc """
  The theme is in place before anything is painted, in the shell and in the
  gate view alike, so neither opens in one appearance and jumps to another.

  Each test reads what the document looked like in its first animation frame,
  which runs before the browser paints it. Anything applied later, by the
  deferred bundle or by a message across documents, lands after that and shows
  up here as the default.
  """
  use PhoenixTest.Playwright.Case, async: false

  import Wicket.GatesCase

  alias PlaywrightEx.BrowserContext

  @moduletag :playwright
  # The sidebar's default follows the viewport, so pin one wide enough to open it.
  @moduletag browser_context_opts: [viewport: %{width: 1280, height: 720}]

  @dark "rgb(24, 25, 27)"
  @light "rgb(255, 255, 255)"

  # Installed in every document, the gate view's included.
  #
  # In the view it does two things. It drops the shell's `appearance` message,
  # so the only theme the view can possibly be in is the one its URL carried,
  # which is what makes this deterministic: without that the two race, and the
  # message usually wins on a fast machine. Then it records the document at the
  # instant the SDK finishes loading, which is the ordering that matters and
  # the one the SDK promises: the theme is on the document before the view's
  # own script runs, so a view cannot render anything in the wrong one. The
  # view has no way to reach the test, so it posts what it saw up to the shell.
  @record """
  if (location.pathname.startsWith("/plugins/")) {
    const listen = window.addEventListener
    window.addEventListener = function (type, fn, opts) {
      const wrapped = type !== "message" ? fn
        : (e) => { if (!(e.data && e.data.type === "appearance")) fn(e) }
      return listen.call(this, type, wrapped, opts)
    }
    Object.defineProperty(window, "Wicket", {
      configurable: true,
      get() { return undefined },
      set(sdk) {
        parent.postMessage({__painted: {
          theme: document.documentElement.dataset.theme || null,
          background: document.body ? getComputedStyle(document.body).backgroundColor : null
        }}, "*")
        Object.defineProperty(window, "Wicket", {value: sdk, writable: true, configurable: true})
      }
    })
  } else {
    window.__painted = null
    window.__viewPainted = null
    requestAnimationFrame(() => {
      const root = document.documentElement
      window.__painted ??= {theme: root.dataset.theme, sidebar: root.dataset.sidebar}
    })
    addEventListener("message", (e) => {
      if (e.data && e.data.__painted) window.__viewPainted ??= e.data.__painted
    })
  }
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

  test "a gate view is in the light theme before its own script runs", %{conn: conn} do
    gate = create_gate!(%{title: "Light gate"})

    conn
    |> save(%{"wicket:theme" => "light"})
    |> visit("/gates/#{gate.id}")
    |> assert_has("h1", text: "Light gate")
    |> assert_frame_url_carries("light")
    |> assert_view_painted(%{"theme" => "light", "background" => @light})
  end

  test "a gate view is dark under a dark shell", %{conn: conn} do
    gate = create_gate!(%{title: "Dark gate"})

    conn
    |> visit("/gates/#{gate.id}")
    |> assert_has("h1", text: "Dark gate")
    |> assert_frame_url_carries("dark")
    |> assert_view_painted(%{"theme" => "dark", "background" => @dark})
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

  # The hook sets the frame's URL once the socket is up, which is after the
  # page has rendered, so wait for it rather than read it on the way past.
  defp assert_frame_url_carries(conn, theme) do
    evaluate(
      conn,
      """
      new Promise((resolve, reject) => {
        const deadline = Date.now() + 4000
        const poll = () => {
          const src = document.getElementById("plugin-frame")?.src
          if (src) return resolve(src)
          if (Date.now() > deadline) return reject(new Error("the frame was never given a URL"))
          setTimeout(poll, 20)
        }
        poll()
      })
      """,
      fn src -> assert String.ends_with?(src, "#wicket-theme=" <> theme) end
    )
  end

  # The view reports across documents, so wait for its word rather than guess.
  defp assert_view_painted(conn, expected) do
    evaluate(
      conn,
      """
      new Promise((resolve, reject) => {
        const deadline = Date.now() + 4000
        const poll = () => {
          if (window.__viewPainted) return resolve(window.__viewPainted)
          if (Date.now() > deadline) return reject(new Error("the view never reported a first paint"))
          setTimeout(poll, 20)
        }
        poll()
      })
      """,
      &assert(&1 == expected)
    )
  end
end

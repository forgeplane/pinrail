defmodule WicketWeb.FaviconTest do
  @moduledoc """
  The tab icon: the wicket mark, drawn once as an SVG and rasterised for
  browsers that want a file of pixels.
  """
  use WicketWeb.ConnCase

  test "the mark is served as an SVG that follows the theme", %{conn: conn} do
    conn = get(conn, "/favicon.svg")
    body = response(conn, 200)
    assert response_content_type(conn, :svg) =~ "image/svg+xml"

    # The same two strokes the sidebar draws, so the tab and the app agree.
    assert body =~ "M3 21V3h7v18 M14 21V3h7v18"
    assert body =~ "prefers-color-scheme: dark"
    assert body =~ "#a1a5ef"
  end

  test "there is a raster for browsers that do not take an SVG", %{conn: conn} do
    conn = get(conn, "/favicon.ico")
    body = response(conn, 200)
    # An icon file carrying more than one size, not a PNG under another name.
    assert <<0, 0, 1, 0, count, 0, _::binary>> = body
    assert count == 3
  end

  test "the page points at both", %{conn: conn} do
    html = html_response(get(conn, "/"), 200)
    assert html =~ ~s(<link rel="icon" href="/favicon.svg" type="image/svg+xml")
    assert html =~ ~s(<link rel="icon" href="/favicon.ico")
  end
end

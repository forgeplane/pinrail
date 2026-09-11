defmodule WicketWeb.IconsTest do
  @moduledoc """
  The icon set a plugin view draws from: one file per icon under the SDK path,
  so a view pays for the icons it names and for none of the others.
  """
  use WicketWeb.ConnCase

  @icons Path.expand("../../priv/static/sdk/v1/icons", __DIR__)

  test "an icon is served as a file, and a name with nothing behind it is not", %{conn: conn} do
    conn = get(conn, "/sdk/v1/icons/check.svg")
    body = response(conn, 200)
    assert response_content_type(conn, :svg) =~ "image/svg+xml"
    assert body =~ "<svg"
    # Drawn as a stroke in currentColor, which is what makes it work as a mask.
    assert body =~ ~s(stroke="currentColor")

    assert response(get(build_conn(), "/sdk/v1/icons/not-an-icon.svg"), 404)
  end

  test "the whole set is there, with the licence beside it" do
    assert File.dir?(@icons), "run mix assets.build"
    icons = Path.wildcard(Path.join(@icons, "*.svg"))
    assert length(icons) > 1000, "the set, not a hand-picked few"

    for name <- ~w(check x chevron-right circle-alert info clock file-diff git-branch) do
      assert File.exists?(Path.join(@icons, name <> ".svg")), "#{name} is missing"
    end

    licence = File.read!(Path.join(@icons, "LICENSE"))
    assert licence =~ "ISC License"
    assert licence =~ "Lucide"
  end
end

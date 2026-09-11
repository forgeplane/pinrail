defmodule WicketWeb.PluginControllerTest do
  use WicketWeb.ConnCase
  import Wicket.GatesCase

  alias Wicket.Types

  setup do
    reset!()
    :ok
  end

  test "serves the snapshot of a version once a gate used it, with the sandbox CSP", %{conn: conn} do
    # nothing snapshotted yet: 404 even though the plugin is registered
    assert response(get(conn, ~p"/plugins/list/1/index.html"), 404)

    create_gate!()
    conn = get(conn, ~p"/plugins/list/1/index.html")
    assert response(conn, 200) =~ "list gate"
    assert response_content_type(conn, :html) =~ "text/html"

    [csp] = get_resp_header(conn, "content-security-policy")
    assert csp =~ "default-src 'none'"
    assert csp =~ "connect-src 'none'"

    assert csp =~
             "script-src 'unsafe-inline' http://www.example.com:80/plugins/list/1/ http://www.example.com:80/sdk/"

    assert csp =~
             "style-src 'unsafe-inline' http://www.example.com:80/plugins/list/1/ http://www.example.com:80/sdk/"

    # A CSS mask is an image to the policy, so the icon set the app serves has
    # to be an image source; nothing else outside the plugin's own bundle is.
    assert csp =~
             "img-src data: blob: http://www.example.com:80/plugins/list/1/ http://www.example.com:80/sdk/"

    assert csp =~ "font-src data: http://www.example.com:80/plugins/list/1/"
    assert csp =~ "frame-ancestors 'self'"

    conn = get(build_conn(), ~p"/plugins/list/1/decision.schema.json")
    assert json_response(conn, 200)["required"] == ["decisions", "undecided"]
  end

  test "keeps serving the snapshot after the plugin is bumped", %{conn: conn} do
    create_gate!()
    {:ok, 1} = use_plugin_dirs([fixture_dir("dup")])
    assert {:ok, %{version: 9}} = Types.fetch("list")

    assert response(get(conn, ~p"/plugins/list/1/index.html"), 200) =~ "list gate"
    # v9 has no gate yet, so no snapshot to serve
    assert response(get(build_conn(), ~p"/plugins/list/9/index.html"), 404)
  end

  test "serves a dev plugin live without a snapshot", %{conn: conn} do
    {:ok, 1} = use_plugin_dirs([fixture_dir("dev")])
    assert response(get(conn, ~p"/plugins/live/1/index.html"), 200) =~ "<!doctype html>"
    refute File.exists?(Types.snapshot_dir("live", 1))
  end

  test "404 for unknown type, version, file, and traversal", %{conn: conn} do
    create_gate!()
    assert response(get(conn, "/plugins/nope/1/index.html"), 404)
    assert response(get(build_conn(), "/plugins/list/x/index.html"), 404)
    assert response(get(build_conn(), "/plugins/list/1/missing.html"), 404)
    assert response(get(build_conn(), "/plugins/list/1/../../gates/index.html"), 404)
    assert response(get(build_conn(), "/plugins/list/1/%2e%2e/%2e%2e/secret"), 404)
  end
end

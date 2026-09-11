defmodule WicketWeb.API.TypeControllerTest do
  use WicketWeb.ConnCase
  import Wicket.GatesCase

  setup do
    reset!()

    {:ok,
     conn: Phoenix.ConnTest.build_conn() |> put_req_header("content-type", "application/json")}
  end

  test "GET /api/types lists types with their schemas and dirs", %{conn: conn} do
    assert %{"dirs" => [dir], "types" => [type]} = json_response(get(conn, ~p"/api/types"), 200)
    assert dir == builtin_dir()

    assert %{
             "name" => "list",
             "version" => 1,
             "usable" => true,
             "error" => nil,
             "entry" => "index.html",
             "payload_schema" => %{"$ref" => "payload.schema.json"},
             "decision_schema" => %{"$ref" => "decision.schema.json"}
           } = type
  end

  test "POST /api/types/dirs registers a directory; duplicates are refused", %{conn: conn} do
    conn1 = post(conn, ~p"/api/types/dirs", %{"dir" => fixture_dir("good")})
    assert %{"ok" => true, "count" => 2, "dirs" => dirs} = json_response(conn1, 200)
    assert fixture_dir("good") in dirs

    conn2 = post(conn, ~p"/api/types/dirs", %{"dir" => fixture_dir("dup")})
    assert %{"error" => "bad_request", "message" => msg} = json_response(conn2, 400)
    assert msg =~ "defined at"

    assert %{"message" => "dir is required"} =
             json_response(post(conn, ~p"/api/types/dirs", %{}), 400)
  end

  test "POST /api/types/reload rescans", %{conn: conn} do
    assert %{"ok" => true, "count" => 1} = json_response(post(conn, ~p"/api/types/reload"), 200)
  end
end

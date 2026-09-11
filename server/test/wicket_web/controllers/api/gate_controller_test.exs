defmodule WicketWeb.API.GateControllerTest do
  use WicketWeb.ConnCase
  import Wicket.GatesCase

  alias Wicket.Gates

  setup do
    reset!()

    {:ok,
     conn: Phoenix.ConnTest.build_conn() |> put_req_header("content-type", "application/json")}
  end

  @create %{
    "type" => "list",
    "title" => "MR !42",
    "source" => %{"repo" => "acme", "workflow" => "review", "ref" => "42"},
    "requested_by" => "agent",
    "payload" => %{"intro" => "hi", "groups" => []}
  }

  describe "POST /api/gates" do
    test "creates and returns the envelope with payload", %{conn: conn} do
      conn = post(conn, ~p"/api/gates", @create)

      assert %{
               "id" => "g_" <> _,
               "status" => "pending",
               "type_version" => 1,
               "payload" => %{"intro" => "hi"}
             } =
               json_response(conn, 201)
    end

    test "422 with violations for a bad envelope or payload", %{conn: conn} do
      conn = post(conn, ~p"/api/gates", %{"type" => "list"})

      assert %{
               "error" => "invalid",
               "violations" => [%{"path" => "/title", "message" => "is required"}]
             } =
               json_response(conn, 422)

      conn = post(conn, ~p"/api/gates", %{@create | "payload" => %{"groups" => "no"}})
      assert %{"violations" => [%{"path" => "/payload/groups"}]} = json_response(conn, 422)

      conn = post(conn, ~p"/api/gates", %{@create | "type" => "nope"})
      assert %{"violations" => [%{"path" => "/type"}]} = json_response(conn, 422)
    end
  end

  describe "GET /api/gates" do
    test "lists newest first without payloads, with filters", %{conn: conn} do
      a = create_gate!(%{source: %{repo: "acme", workflow: "review", ref: "1"}})
      b = create_gate!(%{source: %{repo: "other", workflow: "review", ref: "2"}})
      {:ok, _} = Gates.decide(b.id, list_decision())

      assert [%{"id" => id_b, "status" => "decided"} = first, %{"id" => id_a}] =
               json_response(get(conn, ~p"/api/gates"), 200)

      assert {id_b, id_a} == {b.id, a.id}
      refute Map.has_key?(first, "payload")

      assert [%{"id" => ^id_a}] = json_response(get(conn, ~p"/api/gates?status=pending"), 200)
      assert [%{"id" => ^id_b}] = json_response(get(conn, ~p"/api/gates?repo=other"), 200)

      assert [%{"id" => ^id_b}, %{"id" => ^id_a}] =
               json_response(get(conn, ~p"/api/gates?status=pending,decided"), 200)

      assert [_] = json_response(get(conn, ~p"/api/gates?limit=1"), 200)
      assert [] = json_response(get(conn, ~p"/api/gates?ref=9"), 200)

      assert %{"violations" => [%{"path" => "/status"}]} =
               json_response(get(conn, ~p"/api/gates?status=bogus"), 422)
    end
  end

  describe "GET /api/gates/:id" do
    test "returns the envelope with payload and decision", %{conn: conn} do
      gate = create_gate!()

      assert %{"payload" => %{"groups" => _}, "decision" => nil} =
               json_response(get(conn, ~p"/api/gates/#{gate.id}"), 200)

      {:ok, _} = Gates.decide(gate.id, list_decision(), agent_note: "n")

      assert %{
               "status" => "decided",
               "agent_note" => "n",
               "decision" => %{"decided_by" => "tester", "data" => %{"undecided" => [2]}}
             } =
               json_response(get(conn, ~p"/api/gates/#{gate.id}"), 200)

      assert %{"error" => "not_found"} = json_response(get(conn, ~p"/api/gates/g_nope"), 404)
    end
  end

  describe "POST /api/gates/:id/decision" do
    test "records a valid decision", %{conn: conn} do
      gate = create_gate!()

      conn =
        post(conn, ~p"/api/gates/#{gate.id}/decision", %{
          "data" => list_decision(),
          "agent_note" => "ok"
        })

      assert %{"status" => "decided", "agent_note" => "ok"} = json_response(conn, 200)
    end

    test "422 keeps the gate pending; 409 once decided; 404 unknown", %{conn: conn} do
      gate = create_gate!()

      conn1 = post(conn, ~p"/api/gates/#{gate.id}/decision", %{"data" => %{"decisions" => []}})

      assert %{
               "error" => "invalid",
               "violations" => [%{"path" => "", "message" => "property 'undecided' is required"}]
             } =
               json_response(conn1, 422)

      conn2 = post(conn, ~p"/api/gates/#{gate.id}/decision", %{"agent_note" => "no data"})

      assert %{"violations" => [%{"path" => "/data", "message" => "is required"}]} =
               json_response(conn2, 422)

      assert %{"status" => "pending"} = json_response(get(conn, ~p"/api/gates/#{gate.id}"), 200)

      {:ok, _} = Gates.decide(gate.id, list_decision())
      conn3 = post(conn, ~p"/api/gates/#{gate.id}/decision", %{"data" => list_decision()})
      assert %{"error" => "not_pending"} = json_response(conn3, 409)

      conn4 = post(conn, ~p"/api/gates/g_nope/decision", %{"data" => list_decision()})
      assert %{"error" => "not_found"} = json_response(conn4, 404)
    end
  end

  describe "POST /api/gates/:id/withdraw" do
    test "withdraws once", %{conn: conn} do
      gate = create_gate!()

      assert %{"status" => "withdrawn"} =
               json_response(post(conn, ~p"/api/gates/#{gate.id}/withdraw"), 200)

      assert %{"error" => "not_pending"} =
               json_response(post(conn, ~p"/api/gates/#{gate.id}/withdraw"), 409)
    end
  end

  describe "GET /api/gates/:id/wait" do
    test "returns immediately for a settled gate", %{conn: conn} do
      gate = create_gate!()
      {:ok, _} = Gates.decide(gate.id, list_decision())

      assert %{"status" => "decided"} =
               json_response(get(conn, ~p"/api/gates/#{gate.id}/wait?timeout=5"), 200)
    end

    test "blocks until the decision lands", %{conn: conn} do
      gate = create_gate!()

      Task.start(fn ->
        Process.sleep(150)
        {:ok, _} = Gates.decide(gate.id, list_decision())
      end)

      started = System.monotonic_time(:millisecond)
      conn = get(conn, ~p"/api/gates/#{gate.id}/wait?timeout=5")
      assert %{"status" => "decided", "decision" => %{"data" => _}} = json_response(conn, 200)
      assert System.monotonic_time(:millisecond) - started >= 100
    end

    test "unblocks on withdraw", %{conn: conn} do
      gate = create_gate!()
      Task.start(fn -> Process.sleep(50) && Gates.withdraw(gate.id) end)

      assert %{"status" => "withdrawn"} =
               json_response(get(conn, ~p"/api/gates/#{gate.id}/wait?timeout=5"), 200)
    end

    test "204 on timeout, gate still pending", %{conn: conn} do
      gate = create_gate!()
      conn = get(conn, ~p"/api/gates/#{gate.id}/wait?timeout=0")
      assert response(conn, 204)

      assert %{"status" => "pending"} =
               json_response(get(build_conn(), ~p"/api/gates/#{gate.id}"), 200)
    end

    test "wakes when the gate expires on its own", %{conn: conn} do
      gate = create_gate!(%{expires_at: DateTime.add(DateTime.utc_now(), 300, :millisecond)})

      assert %{"status" => "expired"} =
               json_response(get(conn, ~p"/api/gates/#{gate.id}/wait?timeout=5"), 200)
    end

    test "404 for an unknown gate", %{conn: conn} do
      assert json_response(get(conn, ~p"/api/gates/g_nope/wait"), 404)
    end
  end
end

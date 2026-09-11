defmodule Wicket.GatesTest do
  use Wicket.GatesCase

  alias Wicket.{Gate, GateError, Gates, Gates.Index, Types}

  describe "create/1" do
    test "assigns a g_ ulid, normalises the source and stores the payload" do
      gate = create_gate!(%{source: %{repo: "acme", ref: 42, junk: "x"}, requested_by: "agent"})

      assert "g_" <> ulid = gate.id
      assert Wicket.ULID.valid?(ulid)
      assert gate.source == %{"repo" => "acme", "ref" => "42"}
      assert gate.requested_by == "agent"
      assert gate.type_version == 1
      assert Gate.status(gate) == :pending

      assert {:ok, %Gate{payload: %{"intro" => "hi"}}} = Gates.get(gate.id)
      assert [%Gate{id: id, payload: nil}] = Gates.list()
      assert id == gate.id
    end

    test "accepts string keys" do
      assert {:ok, %Gate{title: "s", source: %{"repo" => "r"}}} =
               Gates.create(%{
                 "type" => "list",
                 "title" => "s",
                 "source" => %{"repo" => "r"},
                 "payload" => %{"groups" => []}
               })
    end

    test "reports every envelope violation at once" do
      assert {:error, %GateError{reason: :invalid, violations: violations}} =
               Gates.create(%{title: "", expires_at: "soon", supersedes: "g_x", source: "nope"})

      assert Enum.map(violations, & &1.path) ==
               ["/type", "/title", "/expires_at", "/supersedes", "/source"]

      assert %{path: "/supersedes", message: "unknown gate g_x"} in violations
    end

    test "rejects an unknown type and a payload that fails the type's schema" do
      assert {:error,
              %GateError{violations: [%{path: "/type", message: "unknown gate type nope"}]}} =
               Gates.create(%{type: "nope", title: "t"})

      assert {:error, %GateError{violations: [%{path: "/payload", message: msg}]}} =
               Gates.create(%{type: "list", title: "t", payload: %{"intro" => "x"}})

      assert msg == "property 'groups' is required"
      assert Gates.list() == []

      assert {:error,
              %GateError{message: "validation failed:\n  /payload: property 'groups' is required"}} =
               Gates.create(%{type: "list", title: "t", payload: %{"intro" => "x"}})
    end

    test "snapshots the plugin version on first use" do
      refute File.exists?(Types.snapshot_dir("list", 1))
      create_gate!()
      assert File.regular?(Path.join(Types.snapshot_dir("list", 1), "manifest.json"))
    end

    test "broadcasts created" do
      Gates.subscribe()
      gate = create_gate!()
      assert_receive {:gate, :created, %Gate{id: id, payload: nil}}
      assert id == gate.id
    end
  end

  describe "list/1" do
    test "newest first, with filters" do
      a = create_gate!(%{source: %{repo: "acme", workflow: "review", ref: "1"}})
      b = create_gate!(%{source: %{repo: "acme", workflow: "triage", ref: "2"}})
      c = create_gate!(%{source: %{repo: "other", workflow: "review", ref: "1"}})

      assert ids(Gates.list()) == [c.id, b.id, a.id]
      assert ids(Gates.list(repo: "acme")) == [b.id, a.id]
      assert ids(Gates.list(repo: "acme", workflow: "review")) == [a.id]
      assert ids(Gates.list(ref: "1")) == [c.id, a.id]
      assert ids(Gates.list(type: "list")) == [c.id, b.id, a.id]
      assert ids(Gates.list(type: "other")) == []
      assert ids(Gates.list(limit: 2)) == [c.id, b.id]
    end

    test "filters by derived status" do
      pending = create_gate!()
      decided = create_gate!()
      withdrawn = create_gate!()
      expired = create_gate!(%{expires_at: DateTime.add(DateTime.utc_now(), -60)})
      {:ok, _} = Gates.decide(decided.id, list_decision())
      {:ok, _} = Gates.withdraw(withdrawn.id)

      assert ids(Gates.list(status: :pending)) == [pending.id]
      assert ids(Gates.list(status: :decided)) == [decided.id]
      assert ids(Gates.list(status: :withdrawn)) == [withdrawn.id]
      assert ids(Gates.list(status: :expired)) == [expired.id]
      assert ids(Gates.list(status: [:decided, :withdrawn])) == [withdrawn.id, decided.id]
    end

    test "superseded: false shows only the newest of a chain" do
      r1 = create_gate!()
      r2 = create_gate!(%{supersedes: r1.id})
      r3 = create_gate!(%{supersedes: r2.id})
      other = create_gate!()

      assert ids(Gates.list(superseded: false)) == [other.id, r3.id]
      assert Gates.pending_count() == 2
      assert ids(Gates.chain(r3)) == [r2.id, r1.id]
      assert Gates.chain(r1) == []
      assert Gates.superseded_by(r1.id).id == r2.id
      assert Gates.superseded_by(r3.id) == nil
    end
  end

  describe "decide/3" do
    test "records the decision with the configured user and broadcasts" do
      gate = create_gate!()
      Gates.subscribe(gate.id)

      assert {:ok, decided} = Gates.decide(gate.id, list_decision(), agent_note: "  hi  ")
      assert decided.decision.decided_by == "tester"
      assert decided.decision.data == list_decision()
      assert decided.agent_note == "hi"
      assert Gate.status(decided) == :decided
      assert_receive {:gate, :decided, %Gate{decision: %{decided_by: "tester"}}}

      assert {:ok, %Gate{decision: %{data: %{"undecided" => [2]}}}} = Gates.get(gate.id)
      assert Enum.map(Gates.events(gate.id), & &1["event"]) == ["created", "decided"]
    end

    test "is append-only: a second decision is refused" do
      gate = create_gate!()
      {:ok, _} = Gates.decide(gate.id, list_decision(), decided_by: "a")

      assert {:error, %GateError{reason: :not_pending}} =
               Gates.decide(gate.id, list_decision(), decided_by: "b")

      assert {:ok, %Gate{decision: %{decided_by: "a"}}} = Gates.get(gate.id)
    end

    test "an invalid decision is refused with violations and the gate stays pending" do
      gate = create_gate!()

      assert {:error, %GateError{reason: :invalid, violations: [%{path: "/decisions/0/action"}]}} =
               Gates.decide(gate.id, %{
                 "decisions" => [%{"id" => 1, "action" => "maybe"}],
                 "undecided" => []
               })

      assert {:error, %GateError{reason: :invalid, violations: [%{path: "", message: msg}]}} =
               Gates.decide(gate.id, "garbage")

      assert msg == "must be a JSON object"
      assert Gate.status(Gates.get!(gate.id)) == :pending
      assert Enum.map(Gates.events(gate.id), & &1["event"]) == ["created"]
    end

    test "refuses withdrawn, expired and unknown gates" do
      withdrawn = create_gate!()
      {:ok, _} = Gates.withdraw(withdrawn.id)

      assert {:error, %GateError{reason: :not_pending}} =
               Gates.decide(withdrawn.id, list_decision())

      expired = create_gate!(%{expires_at: DateTime.add(DateTime.utc_now(), -1)})

      assert {:error, %GateError{reason: :not_pending}} =
               Gates.decide(expired.id, list_decision())

      assert {:error, %GateError{reason: :not_found}} = Gates.decide("g_missing", list_decision())
      assert_raise GateError, fn -> Gates.get!("g_missing") end
    end

    test "racing decisions: exactly one wins" do
      gate = create_gate!()

      results =
        1..8
        |> Task.async_stream(fn n ->
          Gates.decide(gate.id, list_decision(), decided_by: "u#{n}")
        end)
        |> Enum.map(fn {:ok, r} -> r end)

      assert Enum.count(results, &match?({:ok, _}, &1)) == 1
      assert Enum.count(results, &match?({:error, %GateError{reason: :not_pending}}, &1)) == 7
    end

    test "validates against the version the gate was created under after a plugin bump" do
      gate = create_gate!()
      {:ok, 1} = use_plugin_dirs([fixture_dir("dup")])
      assert {:ok, %{version: 9}} = Types.fetch("list")

      assert {:ok, %Gate{type_version: 1, decision: %{}}} = Gates.decide(gate.id, list_decision())
    end
  end

  describe "withdraw/1" do
    test "marks withdrawn, broadcasts, and cannot be decided after" do
      gate = create_gate!()
      Gates.subscribe()
      assert {:ok, %Gate{withdrawn_at: %DateTime{}}} = Gates.withdraw(gate.id)
      assert_receive {:gate, :withdrawn, %Gate{}}
      assert {:error, %GateError{reason: :not_pending}} = Gates.withdraw(gate.id)
      assert Gate.status(Gates.get!(gate.id)) == :withdrawn
    end
  end

  describe "sweep_expired/0" do
    test "logs and broadcasts each expiry once" do
      Gates.subscribe()
      gate = create_gate!(%{expires_at: DateTime.add(DateTime.utc_now(), -1)})
      _fresh = create_gate!(%{expires_at: DateTime.add(DateTime.utc_now(), 3600)})

      assert [%Gate{id: id}] = Gates.sweep_expired()
      assert id == gate.id
      assert_receive {:gate, :expired, %Gate{id: ^id}}
      assert [] = Gates.sweep_expired()
      assert Enum.map(Gates.events(gate.id), & &1["event"]) == ["created", "expired"]
    end
  end

  describe "index" do
    test "is rebuilt from the files after a restart" do
      a = create_gate!()
      b = create_gate!()
      {:ok, _} = Gates.decide(a.id, list_decision())

      assert Index.reload() == 2
      assert ids(Gates.list(status: :decided)) == [a.id]
      assert ids(Gates.list(status: :pending)) == [b.id]
    end

    @tag :capture_log
    test "skips a corrupt gate directory instead of failing the scan" do
      a = create_gate!()
      dir = Wicket.Store.gate_dir(Wicket.data_dir(), "g_corrupt")
      File.mkdir_p!(dir)
      File.write!(Path.join(dir, "gate.json"), "{not json")

      assert Index.reload() == 1
      assert ids(Gates.list()) == [a.id]
    end
  end

  defp ids(gates), do: Enum.map(gates, & &1.id)
end

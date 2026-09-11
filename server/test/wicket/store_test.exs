defmodule Wicket.StoreTest do
  use Wicket.GatesCase

  alias Wicket.{Gate, Store}

  defp root, do: Wicket.data_dir()

  defp gate(id) do
    %Gate{
      id: id,
      type: "list",
      title: "t",
      source: %{},
      created_at: ~U[2026-09-11 10:00:00Z],
      payload: %{"x" => 1}
    }
  end

  test "write_gate is write-once and logs the created event" do
    assert :ok = Store.write_gate(root(), gate("g_A"))
    assert {:error, :eexist} = Store.write_gate(root(), gate("g_A"))
    assert [%{"event" => "created"}] = Store.events(root(), "g_A")
    assert File.ls!(Store.gate_dir(root(), "g_A")) |> Enum.sort() == ["events.jsonl", "gate.json"]
  end

  test "decision.json is created exclusively: a second writer loses" do
    :ok = Store.write_gate(root(), gate("g_B"))
    d = %{"decided_by" => "a", "decided_at" => "2026-09-11T10:01:00Z", "data" => %{}}
    assert :ok = Store.write_decision(root(), "g_B", d)
    assert {:error, :eexist} = Store.write_decision(root(), "g_B", %{d | "decided_by" => "b"})
    assert {:ok, %Gate{decision: %{decided_by: "a"}}} = Store.load(root(), "g_B")
  end

  test "withdrawn.json likewise" do
    :ok = Store.write_gate(root(), gate("g_C"))
    assert :ok = Store.write_withdrawn(root(), "g_C", %{"withdrawn_at" => "2026-09-11T10:01:00Z"})
    assert {:error, :eexist} = Store.write_withdrawn(root(), "g_C", %{"withdrawn_at" => "x"})
    assert {:ok, %Gate{withdrawn_at: ~U[2026-09-11 10:01:00Z]}} = Store.load(root(), "g_C")
  end

  test "writing a decision for an unknown gate is not_found, not a crash" do
    assert {:error, :not_found} = Store.write_decision(root(), "g_nope", %{})
    assert {:error, :not_found} = Store.load(root(), "g_nope")
  end

  test "no temp file survives a write" do
    :ok = Store.write_gate(root(), gate("g_D"))
    refute Enum.any?(File.ls!(Store.gate_dir(root(), "g_D")), &String.contains?(&1, ".tmp."))
  end

  test "list_ids ignores foreign entries and sorts" do
    :ok = Store.write_gate(root(), gate("g_02"))
    :ok = Store.write_gate(root(), gate("g_01"))
    File.mkdir_p!(Path.join(Store.gates_dir(root()), "junk"))
    assert Store.list_ids(root()) == ["g_01", "g_02"]
  end
end

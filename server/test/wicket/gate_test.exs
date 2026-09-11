defmodule Wicket.GateTest do
  use ExUnit.Case, async: true

  alias Wicket.Gate

  defp gate(attrs \\ %{}) do
    struct!(
      Gate,
      Map.merge(
        %{
          id: "g_1",
          type: "list",
          title: "t",
          source: %{"repo" => "acme"},
          created_at: ~U[2026-09-11 10:00:00Z]
        },
        attrs
      )
    )
  end

  test "status is derived from decision, withdrawal and expiry" do
    now = ~U[2026-09-11 12:00:00Z]
    assert Gate.status(gate(), now) == :pending
    assert Gate.status(gate(%{expires_at: ~U[2026-09-11 13:00:00Z]}), now) == :pending
    assert Gate.status(gate(%{expires_at: ~U[2026-09-11 11:00:00Z]}), now) == :expired
    assert Gate.status(gate(%{withdrawn_at: now}), now) == :withdrawn

    decided = gate(%{decision: %{decided_by: "x", decided_at: now, data: %{}}})
    assert Gate.status(decided, now) == :decided
    # a decision wins over an elapsed expiry: it was recorded while pending
    assert Gate.status(%{decided | expires_at: ~U[2026-09-11 11:00:00Z]}, now) == :decided
  end

  test "envelope round-trips through json" do
    g = gate(%{expires_at: ~U[2026-09-12 10:00:00Z], payload: %{"a" => 1}, supersedes: "g_0"})
    json = g |> Gate.envelope_json() |> JSON.encode!() |> JSON.decode!()
    assert Gate.from_json(json) == g
    refute Map.has_key?(json, "status")
    refute Map.has_key?(json, "decision")
  end

  test "to_map carries status and decision, and can drop the payload" do
    g = gate(%{payload: %{"a" => 1}})
    assert %{"status" => "pending", "decision" => nil, "payload" => %{"a" => 1}} = Gate.to_map(g)
    refute Map.has_key?(Gate.to_map(g, payload: false), "payload")
    assert JSON.encode!(g) =~ ~s("status":"pending")
  end

  test "normalize_source keeps known keys as strings" do
    assert Gate.normalize_source(%{repo: "r", ref: 42, bogus: 1, url: nil}) ==
             %{"repo" => "r", "ref" => "42", "url" => nil}

    assert Gate.normalize_source(nil) == %{}
  end
end

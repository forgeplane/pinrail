defmodule Wicket.GatesCase do
  @moduledoc """
  Test case for anything touching the gate store: starts every test from an
  empty data directory and a rebuilt index. Tests using it cannot be async.
  """
  use ExUnit.CaseTemplate

  using do
    quote do
      import Wicket.GatesCase
    end
  end

  setup do
    File.rm_rf!(Wicket.Store.gates_dir(Wicket.data_dir()))
    Wicket.Gates.Index.reload()
    :ok
  end

  @doc "Creates a gate with sensible defaults, merged with `attrs`."
  def create_gate!(attrs \\ %{}) do
    defaults = %{
      type: "list",
      title: "a gate",
      source: %{repo: "acme", workflow: "review", ref: "1"},
      payload: %{"intro" => "hi", "groups" => []}
    }

    {:ok, gate} = Wicket.Gates.create(Map.merge(defaults, Map.new(attrs)))
    gate
  end
end

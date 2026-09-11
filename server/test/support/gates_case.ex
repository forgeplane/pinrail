defmodule Wicket.GatesCase do
  @moduledoc """
  Test case for anything touching the gate store or the type registry: starts
  every test from an empty data directory, the built-in plugins only, and a
  rebuilt index. Tests using it cannot be async.
  """
  use ExUnit.CaseTemplate

  using do
    quote do
      import Wicket.GatesCase
    end
  end

  @builtin Path.expand("../../priv/plugins", __DIR__)
  @fixtures Path.expand("plugins", __DIR__)

  setup do
    File.rm_rf!(Wicket.Store.gates_dir(Wicket.data_dir()))
    File.rm_rf!(Path.join(Wicket.data_dir(), "plugins"))
    File.rm_rf!(Wicket.Types.config_dir())
    use_plugin_dirs([@builtin])
    Wicket.Gates.Index.reload()
    :ok
  end

  @doc "Points the registry at these directories (absolute) and reloads."
  def use_plugin_dirs(dirs) do
    Application.put_env(:wicket, :plugin_dirs, dirs)
    Wicket.Types.reload()
  end

  @doc "A fixture plugin directory under test/support/plugins."
  def fixture_dir(name), do: Path.join(@fixtures, name)

  def builtin_dir, do: @builtin

  @doc "A valid list payload with items 1 and 2."
  def list_payload do
    %{
      "intro" => "hi",
      "groups" => [
        %{
          "title" => "g",
          "items" => [%{"id" => 1, "title" => "one"}, %{"id" => 2, "title" => "two"}]
        }
      ]
    }
  end

  @doc "Creates a gate with sensible defaults, merged with `attrs`."
  def create_gate!(attrs \\ %{}) do
    defaults = %{
      type: "list",
      title: "a gate",
      source: %{repo: "acme", workflow: "review", ref: "1"},
      payload: list_payload()
    }

    {:ok, gate} = Wicket.Gates.create(Map.merge(defaults, Map.new(attrs)))
    gate
  end

  @doc "A decision that accepts item 1 and leaves 2 undecided."
  def list_decision,
    do: %{"decisions" => [%{"id" => 1, "action" => "accept"}], "undecided" => [2]}
end

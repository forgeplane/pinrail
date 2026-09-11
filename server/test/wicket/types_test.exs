defmodule Wicket.TypesTest do
  use Wicket.GatesCase

  alias Wicket.{GateError, Types}
  alias Wicket.Types.Plugin

  describe "registry" do
    test "ships the built-in list type" do
      assert [%Plugin{name: "list", version: 1, title: "List", error: nil}] = Types.all()
      assert {:ok, %Plugin{name: "list"}} = Types.fetch("list")
      assert {:ok, %Plugin{name: "list"}} = Types.fetch("list", 1)
    end

    test "loads plugins from added directories, with cross-file refs" do
      assert {:ok, 2} = use_plugin_dirs([builtin_dir(), fixture_dir("good")])
      assert {:ok, %Plugin{name: "echo", version: 2, min_height: 320}} = Types.fetch("echo")
      assert Enum.map(Types.all(), & &1.name) == ["echo", "list"]
    end

    test "unknown and broken types are :invalid errors at /type" do
      assert {:error, %GateError{reason: :invalid, violations: [%{path: "/type", message: msg}]}} =
               Types.fetch("nope")

      assert msg =~ "unknown gate type nope"
      assert {:error, %GateError{violations: [%{path: "/type"}]}} = Types.fetch("list", 7)
    end

    @tag :capture_log
    test "a broken plugin is listed with its error and cannot be fetched" do
      {:ok, 3} = use_plugin_dirs([builtin_dir(), fixture_dir("broken")])
      by_name = Map.new(Types.all(), &{&1.name, &1})

      assert by_name["badschema"].error ==
               "payload_schema: cannot load missing.json: no such file or directory"

      assert by_name["noentry"].error == "entry index.html not found"
      refute Plugin.usable?(by_name["noentry"])

      assert {:error, %GateError{violations: [%{path: "/type", message: msg}]}} =
               Types.fetch("noentry")

      assert msg =~ "not usable"
    end

    test "a duplicate name across directories fails the reload naming both paths" do
      assert {:error, msg} = use_plugin_dirs([builtin_dir(), fixture_dir("dup")])
      assert msg =~ "gate type list is defined at"
      assert msg =~ Path.join(builtin_dir(), "list")
      assert msg =~ Path.join(fixture_dir("dup"), "list")
      # the previous registry survives
      assert {:ok, %Plugin{version: 1}} = Types.fetch("list")
    end

    test "add_dir persists across a reload and rolls back on failure" do
      assert {:ok, 2} = Types.add_dir(fixture_dir("good"))
      assert fixture_dir("good") in Types.dirs()
      assert {:ok, 2} = Types.reload()
      assert {:ok, _} = Types.fetch("echo")

      assert {:error, msg} = Types.add_dir(fixture_dir("dup"))
      assert msg =~ "defined at"
      refute fixture_dir("dup") in Types.dirs()
      assert {:ok, _} = Types.fetch("echo")

      assert {:error, msg} = Types.add_dir("/nonexistent/dir")
      assert msg =~ "not a directory"
    end
  end

  test "directory persistence errors do not change the registry" do
    File.mkdir_p!(Path.dirname(Types.config_dir()))
    File.write!(Types.config_dir(), "not a directory")
    assert {:error, message} = Types.add_dir(fixture_dir("good"))
    assert message =~ "Could not save"
    assert {:ok, _} = Types.fetch("list")
    assert {:error, _} = Types.fetch("echo")
  end

  describe "validation" do
    test "payload violations point into the payload" do
      {:ok, list} = Types.fetch("list")
      assert :ok = Types.validate_payload(list, list_payload())

      bad = %{"groups" => [%{"title" => "g", "items" => [%{"id" => "x"}]}], "bogus" => 1}

      assert {:error, %GateError{reason: :invalid, violations: violations}} =
               Types.validate_payload(list, bad)

      assert %{path: "/bogus", message: "unexpected property"} in violations

      assert %{path: "/groups/0/items/0/id", message: "value is not of type integer"} in violations

      assert %{path: "/groups/0/items/0", message: "property 'title' is required"} in violations
      refute Enum.any?(violations, &(&1.message =~ "did not conform"))
    end

    test "non-object data is one violation at the root" do
      {:ok, list} = Types.fetch("list")

      assert {:error, %GateError{violations: [%{path: "", message: "must be a JSON object"}]}} =
               Types.validate_payload(list, [1, 2])
    end

    test "cross-file refs validate" do
      {:ok, _} = use_plugin_dirs([fixture_dir("good")])
      {:ok, echo} = Types.fetch("echo")
      assert :ok = Types.validate_payload(echo, %{"items" => [%{"id" => 1}]})

      assert {:error, %GateError{violations: [%{path: "/items/0", message: msg}]}} =
               Types.validate_payload(echo, %{"items" => [%{}]})

      assert msg == "property 'id' is required"

      assert :ok = Types.validate_decision(echo, %{"ok" => true, "item" => %{"id" => 3}})

      assert {:error, %GateError{violations: [%{path: "/item/id"}]}} =
               Types.validate_decision(echo, %{"ok" => true, "item" => %{"id" => "3"}})
    end

    test "list decisions are the schema and nothing more" do
      {:ok, list} = Types.fetch("list")
      assert :ok = Types.validate_decision(list, list_decision())

      # ids the payload never had, or nothing listed as undecided: the plugin's
      # business and its requester's, not wicket's
      assert :ok =
               Types.validate_decision(list, %{
                 "decisions" => [%{"id" => 99, "action" => "accept"}],
                 "undecided" => []
               })

      assert {:error, %GateError{violations: [%{path: "/decisions/0/action"}]}} =
               Types.validate_decision(list, %{
                 "decisions" => [%{"id" => 1, "action" => "maybe"}],
                 "undecided" => [2]
               })

      assert {:error, %GateError{violations: [%{path: "", message: msg}]}} =
               Types.validate_decision(list, %{"decisions" => []})

      assert msg == "property 'undecided' is required"
    end
  end

  describe "snapshots" do
    test "the first use of a version copies the plugin into the data dir; dev plugins are served live" do
      {:ok, list} = Types.fetch("list")
      dest = Types.snapshot_dir("list", 1)
      refute File.exists?(dest)

      assert {:ok, ^dest} = Types.ensure_snapshot(list)
      assert File.regular?(Path.join(dest, "index.html"))
      assert File.regular?(Path.join(dest, "decision.schema.json"))
      assert {:ok, ^dest} = Types.ensure_snapshot(list)
      assert Types.bundle_dir(list) == dest

      {:ok, _} = use_plugin_dirs([fixture_dir("dev")])
      {:ok, live} = Types.fetch("live")
      assert {:ok, path} = Types.ensure_snapshot(live)
      assert path == live.path
      refute File.exists?(Types.snapshot_dir("live", 1))
    end

    test "an old version keeps validating from its snapshot after the plugin is bumped" do
      {:ok, list} = Types.fetch("list")
      {:ok, _} = Types.ensure_snapshot(list)

      # bump: the live registry now has list v9 (from the dup fixture, alone)
      {:ok, 1} = use_plugin_dirs([fixture_dir("dup")])
      assert {:ok, %Plugin{version: 9}} = Types.fetch("list")

      assert {:ok, %Plugin{version: 1, path: path} = old} = Types.fetch("list", 1)
      assert path == Types.snapshot_dir("list", 1)
      assert :ok = Types.validate_decision(old, list_decision())
      assert {:error, %GateError{violations: [%{path: "/type"}]}} = Types.fetch("list", 5)
    end
  end
end

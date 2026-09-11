defmodule Wicket.ServerInfoTest do
  use ExUnit.Case, async: false

  alias Wicket.ServerInfo

  test "does not advertise when the endpoint is not serving" do
    # test config has server: false unless the browser suite is on
    if Phoenix.Endpoint.server?(:wicket, WicketWeb.Endpoint) do
      assert %{"url" => "http://127.0.0.1:4002", "port" => 4002, "pid" => pid} = ServerInfo.info()
      assert is_integer(pid)
    else
      refute File.exists?(ServerInfo.path())
      assert ServerInfo.info() == nil
    end
  end
end

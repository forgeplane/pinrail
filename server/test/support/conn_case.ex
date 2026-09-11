defmodule WicketWeb.ConnCase do
  @moduledoc """
  Test case for controller and LiveView tests: builds a conn and imports the
  `Phoenix.ConnTest` and `Phoenix.LiveViewTest` helpers.
  """

  use ExUnit.CaseTemplate

  using do
    quote do
      @endpoint WicketWeb.Endpoint

      use WicketWeb, :verified_routes

      import Plug.Conn
      import Phoenix.ConnTest
      import Phoenix.LiveViewTest
      import WicketWeb.ConnCase
    end
  end

  setup _tags do
    {:ok, conn: Phoenix.ConnTest.build_conn()}
  end
end

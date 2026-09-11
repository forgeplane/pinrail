defmodule WicketWeb.PageController do
  use WicketWeb, :controller

  def home(conn, _params) do
    render(conn, :home)
  end
end

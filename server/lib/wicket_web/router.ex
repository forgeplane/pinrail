defmodule WicketWeb.Router do
  use WicketWeb, :router

  pipeline :browser do
    plug :accepts, ["html"]
    plug :fetch_session
    plug :fetch_live_flash
    plug :put_root_layout, html: {WicketWeb.Layouts, :root}
    plug :protect_from_forgery
    plug :put_secure_browser_headers
  end

  pipeline :api do
    plug :accepts, ["json"]
  end

  scope "/", WicketWeb do
    pipe_through :browser

    live "/", InboxLive
    live "/history", HistoryLive
    live "/types", TypesLive
  end
end

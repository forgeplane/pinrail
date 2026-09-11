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

  scope "/api", WicketWeb.API do
    pipe_through :api

    get "/gates", GateController, :index
    post "/gates", GateController, :create
    get "/gates/:id", GateController, :show
    get "/gates/:id/wait", GateController, :wait
    post "/gates/:id/decision", GateController, :decide
    post "/gates/:id/withdraw", GateController, :withdraw

    get "/types", TypeController, :index
    post "/types/reload", TypeController, :reload
    post "/types/dirs", TypeController, :add_dir
  end

  # The plugin bundle the gate page's iframe loads. Not under /api on purpose.
  scope "/plugins", WicketWeb do
    get "/:type/:version/*path", PluginController, :show
  end
end

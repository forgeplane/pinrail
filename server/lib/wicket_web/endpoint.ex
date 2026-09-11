defmodule WicketWeb.Endpoint do
  use Phoenix.Endpoint, otp_app: :wicket

  @session_options [
    store: :cookie,
    key: "_wicket_key",
    signing_salt: "zssxJZHb",
    same_site: "Lax"
  ]

  socket "/live", Phoenix.LiveView.Socket,
    websocket: [connect_info: [session: @session_options]],
    longpoll: [connect_info: [session: @session_options]]

  # The SDK and the icon set, served to gate views. A view runs in a sandboxed
  # frame with an opaque origin, and a browser fetches a CSS mask image under
  # CORS, so an icon has to be readable from that origin. These are public,
  # immutable files; scripts and stylesheets need nothing extra, only the
  # icons do.
  plug Plug.Static,
    at: "/sdk",
    from: {:wicket, "priv/static/sdk"},
    gzip: not code_reloading?,
    headers: %{"access-control-allow-origin" => "*"}

  plug Plug.Static,
    at: "/",
    from: :wicket,
    gzip: not code_reloading?,
    only: WicketWeb.static_paths(),
    raise_on_missing_only: code_reloading?

  if code_reloading? do
    socket "/phoenix/live_reload/socket", Phoenix.LiveReloader.Socket
    plug Phoenix.LiveReloader
    plug Phoenix.CodeReloader
  end

  plug Plug.RequestId
  plug Plug.Telemetry, event_prefix: [:phoenix, :endpoint]

  plug Plug.Parsers,
    parsers: [:urlencoded, :multipart, :json],
    pass: ["*/*"],
    json_decoder: Phoenix.json_library()

  plug Plug.MethodOverride
  plug Plug.Head
  plug Plug.Session, @session_options
  plug WicketWeb.Router
end

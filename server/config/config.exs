import Config

config :wicket, WicketWeb.Endpoint,
  url: [host: "localhost"],
  # Loopback only: wicket is a single-user localhost tool.
  http: [ip: {127, 0, 0, 1}, port: 4747],
  adapter: Bandit.PhoenixAdapter,
  render_errors: [
    formats: [html: WicketWeb.ErrorHTML, json: WicketWeb.ErrorJSON],
    layout: false
  ],
  pubsub_server: Wicket.PubSub,
  live_view: [signing_salt: "rFc96k5X"]

config :phoenix_live_view,
  root_tag_attribute: "phx-r"

config :esbuild,
  version: "0.25.4",
  wicket: [
    args:
      ~w(js/app.js --bundle --target=es2022 --outdir=../priv/static/assets/js --external:/fonts/* --external:/images/* --alias:@=.),
    cd: Path.expand("../assets", __DIR__),
    env: %{"NODE_PATH" => [Path.expand("../deps", __DIR__), Mix.Project.build_path()]}
  ],
  # The plugin SDK, served to plugins at /sdk/v1/wicket-plugin.js. Source of
  # truth is wicket_sdk/ at the repo root; this only copies it into priv.
  sdk: [
    args:
      ~w(../../wicket_sdk/src/wicket-plugin.js --target=es2022 --outfile=../priv/static/sdk/v1/wicket-plugin.js),
    cd: Path.expand("../assets", __DIR__)
  ]

config :tailwind,
  version: "4.3.0",
  wicket: [
    args: ~w(
      --input=assets/css/app.css
      --output=priv/static/assets/css/app.css
    ),
    cd: Path.expand("..", __DIR__),
    env: %{"NODE_PATH" => [Path.expand("../deps", __DIR__), Mix.Project.build_path()]}
  ]

config :logger, :default_formatter,
  format: "$time $metadata[$level] $message\n",
  metadata: [:request_id]

config :phoenix, :json_library, JSON

import_config "#{config_env()}.exs"

import Config

# Every test run gets a throwaway data directory under server/tmp (gitignored).
config :wicket, data_dir: Path.expand("../tmp/test-data", __DIR__)
config :wicket, user: "tester"
config :wicket, expiry_interval_ms: :timer.hours(1)
config :wicket, config_dir: Path.expand("../tmp/test-config", __DIR__)
config :wicket, plugin_dirs: [Path.expand("../priv/plugins", __DIR__)]

config :wicket, WicketWeb.Endpoint,
  http: [ip: {127, 0, 0, 1}, port: 4002],
  secret_key_base: "U3sPGxgLqgJWuPyUR/bqDayOXU2CnVvFJxngnAr0wAomVJY7D3gLdq9tO9Vte6EO",
  # test_helper.exs turns the server on only when browser tests are included
  server: false

# Browser tests (`mix test.browser`) drive headless Chromium through Playwright,
# installed under assets/ (see README).
config :phoenix_test,
  otp_app: :wicket,
  playwright: [browser: :chromium, headless: true]

config :logger, level: :warning

config :phoenix, :plug_init_mode, :runtime

config :phoenix_live_view,
  enable_expensive_runtime_checks: true

config :phoenix,
  sort_verified_routes_query_params: true

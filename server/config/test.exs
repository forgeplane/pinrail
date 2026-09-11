import Config

# Every test run gets a throwaway data directory under server/tmp (gitignored).
config :wicket, data_dir: Path.expand("../tmp/test-data", __DIR__)
config :wicket, user: "tester"

config :wicket, WicketWeb.Endpoint,
  http: [ip: {127, 0, 0, 1}, port: 4002],
  secret_key_base: "U3sPGxgLqgJWuPyUR/bqDayOXU2CnVvFJxngnAr0wAomVJY7D3gLdq9tO9Vte6EO",
  server: false

config :logger, level: :warning

config :phoenix, :plug_init_mode, :runtime

config :phoenix_live_view,
  enable_expensive_runtime_checks: true

config :phoenix,
  sort_verified_routes_query_params: true

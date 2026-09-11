import Config

config :wicket, WicketWeb.Endpoint,
  check_origin: false,
  code_reloader: true,
  debug_errors: true,
  secret_key_base: "K9/WmYyq+tY390Td20UyreytfTG0kCffArPcs5CMyOFHZqNwoyVG9eIuv8DZyf94",
  watchers: [
    esbuild: {Esbuild, :install_and_run, [:wicket, ~w(--sourcemap=inline --watch)]},
    sdk: {Esbuild, :install_and_run, [:sdk, ~w(--watch)]},
    tailwind: {Tailwind, :install_and_run, [:wicket, ~w(--watch)]}
  ]

config :logger, :default_formatter, format: "[$level] $message\n"

config :phoenix, :stacktrace_depth, 20

config :phoenix, :plug_init_mode, :runtime

config :phoenix_live_view,
  debug_heex_annotations: true,
  debug_attributes: true,
  enable_expensive_runtime_checks: true

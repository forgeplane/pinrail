import Config

# The data directory holds gates, decisions and plugin snapshots. Resolution
# order: WICKET_DATA_DIR, then $XDG_DATA_HOME/wicket, then ~/.local/share/wicket.
# Tests pin it in config/test.exs.
unless config_env() == :test do
  data_dir =
    System.get_env("WICKET_DATA_DIR") ||
      Path.join(
        System.get_env("XDG_DATA_HOME") || Path.join(System.user_home!(), ".local/share"),
        "wicket"
      )

  config :wicket, data_dir: data_dir

  if port = System.get_env("WICKET_PORT") do
    config :wicket, WicketWeb.Endpoint, http: [port: String.to_integer(port)]
  end

  if config_env() == :prod do
    # A release always serves; there is no other reason to start it.
    config :wicket, WicketWeb.Endpoint, server: true

    # The cookie-signing secret lives in the data dir and is generated on first
    # boot, so a localhost install needs no environment setup.
    secret_path = Path.join(data_dir, "secret_key_base")

    secret_key_base =
      case File.read(secret_path) do
        {:ok, secret} ->
          String.trim(secret)

        {:error, _} ->
          secret = Base.encode64(:crypto.strong_rand_bytes(48))
          File.mkdir_p!(data_dir)
          File.write!(secret_path, secret <> "\n")
          File.chmod!(secret_path, 0o600)
          secret
      end

    config :wicket, WicketWeb.Endpoint, secret_key_base: secret_key_base
  end
end

if config_env() == :dev do
  config :wicket, WicketWeb.Endpoint,
    live_reload: [
      web_console_logger: true,
      patterns: [
        ~r"priv/static/(?!uploads/).*\.(js|css|png|jpeg|jpg|gif|svg)$"E,
        ~r"lib/wicket_web/router\.ex$"E,
        ~r"lib/wicket_web/(controllers|live|components)/.*\.(ex|heex)$"E
      ]
    ]
end

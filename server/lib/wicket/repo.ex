defmodule Wicket.Repo do
  use Ecto.Repo,
    otp_app: :wicket,
    adapter: Ecto.Adapters.SQLite3
end

defmodule Wicket.Application do
  @moduledoc false

  use Application

  @impl true
  def start(_type, _args) do
    File.mkdir_p!(Wicket.data_dir())

    children = [
      WicketWeb.Telemetry,
      {Phoenix.PubSub, name: Wicket.PubSub},
      Wicket.Gates.Index,
      Wicket.Types.Registry,
      Wicket.Gates.Expiry,
      WicketWeb.Endpoint
    ]

    Supervisor.start_link(children, strategy: :one_for_one, name: Wicket.Supervisor)
  end

  @impl true
  def config_change(changed, _new, removed) do
    WicketWeb.Endpoint.config_change(changed, removed)
    :ok
  end
end

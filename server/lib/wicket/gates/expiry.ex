defmodule Wicket.Gates.Expiry do
  @moduledoc """
  Periodically sweeps expired gates so the inbox and waiters hear about them.
  Status is derived, so this is only about the event and the broadcast.
  """
  use GenServer

  @default_interval :timer.seconds(30)

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  @impl true
  def init(_opts) do
    interval = Application.get_env(:wicket, :expiry_interval_ms, @default_interval)
    :timer.send_interval(interval, :sweep)
    {:ok, %{}}
  end

  @impl true
  def handle_info(:sweep, state) do
    Wicket.Gates.sweep_expired()
    {:noreply, state}
  end
end

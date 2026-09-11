defmodule Wicket do
  @moduledoc """
  wicket is an approval-gate service for agent workflows: a workflow creates a
  gate carrying a JSON payload and blocks; a human decides it in the browser;
  the workflow resumes with the decision.

  This module holds the few application-wide accessors.
  """

  @doc """
  The directory holding gates, decisions and plugin snapshots.

  Configured under `config :wicket, :data_dir`; resolved in `config/runtime.exs`.
  """
  @spec data_dir() :: Path.t()
  def data_dir, do: Application.fetch_env!(:wicket, :data_dir)
end

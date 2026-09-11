defmodule Mix.Tasks.Wicket.Icons do
  @shortdoc "Puts the icon set where plugin views can reach it"

  @moduledoc """
  Copies the Lucide icons into `priv/static/sdk/v1/icons`, which the app serves
  as `/sdk/v1/icons/<name>.svg`.

  One file per icon, so a view downloads only the icons it names and the whole
  set costs a plugin nothing. The set comes from the pinned `:lucide`
  dependency and the licence travels with it, so nothing here is checked in and
  `mix.lock` decides which icons a build serves.

  The task is part of `mix assets.build`. Run it alone after changing the
  pinned version.
  """

  use Mix.Task

  @source Path.expand("../../../deps/lucide/icons", __DIR__)
  @licence Path.expand("../../../../wicket_sdk/licenses/lucide-icons.txt", __DIR__)
  @target Path.expand("../../../priv/static/sdk/v1/icons", __DIR__)

  @impl Mix.Task
  def run(_args) do
    unless File.dir?(@source) do
      Mix.raise("""
      No icons at #{@source}.

      They come from the :lucide dependency; run `mix deps.get` first.
      """)
    end

    File.mkdir_p!(@target)
    icons = Path.wildcard(Path.join(@source, "*.svg"))

    for icon <- icons do
      File.cp!(icon, Path.join(@target, Path.basename(icon)))
    end

    File.cp!(@licence, Path.join(@target, "LICENSE"))
    Mix.shell().info("wicket.icons: #{length(icons)} icons in priv/static/sdk/v1/icons")
  end
end

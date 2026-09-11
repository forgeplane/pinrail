defmodule WicketTest do
  use ExUnit.Case, async: true

  test "data_dir is configured and exists after boot" do
    dir = Wicket.data_dir()
    assert String.ends_with?(dir, "tmp/test-data")
    assert File.dir?(dir)
  end
end

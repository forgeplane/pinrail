defmodule Wicket.ULIDTest do
  use ExUnit.Case, async: true

  alias Wicket.ULID

  test "26 crockford characters" do
    id = ULID.generate()
    assert String.length(id) == 26
    assert ULID.valid?(id)
    refute ULID.valid?("g_" <> id)
    refute ULID.valid?(String.replace(id, ~r/./, "I", global: false))
  end

  test "orders by time" do
    earlier = ULID.generate(1_000_000)
    later = ULID.generate(1_000_001)
    assert earlier < later
  end

  test "next/0 is strictly increasing even within one millisecond" do
    ids = for _ <- 1..500, do: ULID.next()
    assert ids == Enum.sort(ids)
    assert length(Enum.uniq(ids)) == 500
    assert Enum.all?(ids, &ULID.valid?/1)
  end

  test "is unique within a millisecond" do
    ids = for _ <- 1..500, do: ULID.generate(42)
    assert length(Enum.uniq(ids)) == 500
  end
end

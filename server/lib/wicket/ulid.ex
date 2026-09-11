defmodule Wicket.ULID do
  @moduledoc """
  ULIDs: 48 bits of millisecond timestamp plus 80 bits of entropy, Crockford
  base32 encoded to 26 characters, so lexicographic order is creation order.

  `next/0` is what the app uses. Its entropy half is a per-boot random salt
  followed by `:erlang.unique_integer([:monotonic, :positive])`, so ids minted
  in the same millisecond still sort in creation order, with no process to
  coordinate through. `generate/1` is the fully random form for one-off use.
  """

  @alphabet ~c"0123456789ABCDEFGHJKMNPQRSTVWXYZ"
  @salt_key {__MODULE__, :salt}

  @doc "The next ULID: greater than any this VM returned before."
  @spec next() :: String.t()
  def next do
    counter = :erlang.unique_integer([:monotonic, :positive])
    encode(System.system_time(:millisecond), <<salt()::32, counter::unsigned-size(48)>>)
  end

  @doc "A ULID for the given time (defaults to now) with fresh random bits."
  @spec generate(integer()) :: String.t()
  def generate(time_ms \\ System.system_time(:millisecond)),
    do: encode(time_ms, :crypto.strong_rand_bytes(10))

  @doc "True if the string is a syntactically valid ULID."
  @spec valid?(term()) :: boolean()
  def valid?(<<_::binary-size(26)>> = s), do: String.match?(s, ~r/^[0-7][0-9A-HJKMNP-TV-Z]{25}$/)
  def valid?(_), do: false

  defp salt do
    case :persistent_term.get(@salt_key, nil) do
      nil ->
        <<salt::32>> = :crypto.strong_rand_bytes(4)
        :persistent_term.put(@salt_key, salt)
        salt

      salt ->
        salt
    end
  end

  defp encode(time_ms, <<_::80>> = entropy) do
    bits = <<0::2, time_ms::unsigned-size(48), entropy::binary>>
    for <<chunk::5 <- bits>>, into: "", do: <<Enum.at(@alphabet, chunk)>>
  end
end
